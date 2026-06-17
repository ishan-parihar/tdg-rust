//! Axum HTTP/SSE server for MCP transport
//!
//! Port of `mcp/tdg_mcp_server.py`.
//! Endpoints:
//!   POST /mcp        — JSON-RPC 2.0 (initialize, tools/list, tools/call)
//!   GET  /sse         — SSE stream (initializes session)
//!   POST /tools/{name} — REST fallback
//!   GET  /health       — Health check

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tokio_stream::wrappers::ReceiverStream;
use tower_http::cors::{Any, CorsLayer};

use crate::db::ConnectionPool;
use crate::mcp::protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::mcp::tools;

/// Shared application state
pub struct AppState {
    pub pool: ConnectionPool,
    pub initialized: RwLock<bool>,
}

/// Start the MCP server
pub async fn start_server(
    pool: ConnectionPool,
    port: u16,
) -> anyhow::Result<()> {
    let state = Arc::new(AppState {
        pool,
        initialized: RwLock::new(false),
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/mcp", post(mcp_handler))
        .route("/sse", get(sse_handler))
        .route("/tools/{tool_name}", post(tool_rest_handler))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("MCP server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Health check endpoint
async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "server": "tdg-mcp-rust",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Landing page
async fn index() -> Html<&'static str> {
    Html("<html><body><h1>TDG MCP Server (Rust)</h1><p>Endpoints: /mcp (JSON-RPC), /sse, /tools/{name}, /health</p></body></html>")
}

/// JSON-RPC 2.0 handler — POST /mcp
async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let response = handle_rpc(&state, request).await;
    Json(response)
}

/// SSE handler — GET /sse
async fn sse_handler(
    State(state): State<Arc<AppState>>,
) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel(32);

    // Mark initialized
    {
        let mut init = state.initialized.write().await;
        *init = true;
    }

    tokio::spawn(async move {
        // Send initial endpoint event
        let _ = tx
            .send(Ok(Event::default().event("endpoint").data("/mcp")))
            .await;

        // Keep the stream alive
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let _ = tx
                .send(Ok(Event::default().event("ping").data("keepalive")))
                .await;
        }
    });

    Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

/// REST fallback — POST /tools/{tool_name}
async fn tool_rest_handler(
    State(state): State<Arc<AppState>>,
    Path(tool_name): Path<String>,
    Json(args): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Use with_connection to execute the tool
    let result = state.pool.with_connection(|conn| {
        tools::dispatch_tool(conn, &tool_name, &args)
    });

    match result {
        Ok(tool_result) => Ok(Json(json!(tool_result))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )),
    }
}

/// Handle a JSON-RPC request
async fn handle_rpc(state: &AppState, request: JsonRpcRequest) -> JsonRpcResponse {
    let id = request.id.clone();

    match request.method.as_str() {
        "initialize" => {
            let mut init = state.initialized.write().await;
            *init = true;

            JsonRpcResponse::success(
                id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {},
                        "logging": {}
                    },
                    "serverInfo": {
                        "name": "tdg-mcp-rust",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            )
        }

        "notifications/initialized" => JsonRpcResponse::success(id, json!(null)),

        "tools/list" => {
            let defs = tools::tool_definitions();
            let tools_list: Vec<Value> = defs
                .iter()
                .map(|d| {
                    json!({
                        "name": d.name,
                        "description": d.description,
                        "inputSchema": d.input_schema,
                    })
                })
                .collect();

            JsonRpcResponse::success(id, json!({ "tools": tools_list }))
        }

        "tools/call" => {
            let tool_name = request.params["name"].as_str().unwrap_or("");
            let args = &request.params["arguments"];

            // Use with_connection to execute the tool
            let result = state.pool.with_connection(|conn| {
                tools::dispatch_tool(conn, tool_name, args)
            });

            match result {
                Ok(tool_result) => {
                    let is_error = tool_result.error.is_some();
                    let content = if let Some(data) = tool_result.data {
                        json!([{"type": "text", "text": serde_json::to_string_pretty(&data).unwrap_or_default()}])
                    } else if let Some(err) = tool_result.error {
                        json!([{"type": "text", "text": format!("Error: {}", err)}])
                    } else {
                        json!([{"type": "text", "text": "No result"}])
                    };

                    JsonRpcResponse::success(
                        id,
                        json!({
                            "content": content,
                            "isError": is_error,
                        }),
                    )
                }
                Err(e) => JsonRpcResponse::error(id, -32603, format!("Tool error: {}", e)),
            }
        }

        "ping" => JsonRpcResponse::success(id, json!({})),

        method => JsonRpcResponse::error(id, -32601, format!("Method not found: {}", method)),
    }
}
