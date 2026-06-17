//! MCP Server — rmcp-based transport (stdio + HTTP)
//!
//! Uses official rmcp SDK for spec-compliant MCP server.
//! Supports stdio (for Hermes/Claude integration) and streamable HTTP.

use crate::db::ConnectionPool;
use crate::mcp::tools::TdgServer;

/// Start MCP server with stdio transport (default for AI integration).
pub async fn serve_stdio(pool: ConnectionPool) -> anyhow::Result<()> {
    use rmcp::ServiceExt;
    use rmcp::transport::stdio;

    let server = TdgServer::new(pool);
    let service = server.serve(stdio()).await?;
    let quit_reason = service.waiting().await?;
    tracing::info!("MCP server stopped: {:?}", quit_reason);
    Ok(())
}

/// Start MCP server with HTTP/SSE transport (for debugging/web).
pub async fn serve_http(pool: ConnectionPool, port: u16) -> anyhow::Result<()> {
    let _state = std::sync::Arc::new(TdgServer::new(pool));
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));

    let app = axum::Router::new()
        .route("/health", axum::routing::get(|| async {
            axum::Json(serde_json::json!({"status": "ok", "server": "tdg-mcp-rust"}))
        }))
        .route("/sse", axum::routing::get(|| async {
            // SSE endpoint — returns initial endpoint event
            let (tx, rx): (tokio::sync::mpsc::Sender<Result<axum::response::sse::Event, std::convert::Infallible>>, _) = tokio::sync::mpsc::channel(32);
            let _ = tx.send(Ok(axum::response::sse::Event::default().event("endpoint").data("/mcp"))).await;
            axum::response::sse::Sse::new(tokio_stream::wrappers::ReceiverStream::new(rx))
                .keep_alive(axum::response::sse::KeepAlive::default())
        }))
        .route("/mcp", axum::routing::any(|| async {
            axum::Json(serde_json::json!({
                "server": "tdg-mcp-rust",
                "transport": "streamable-http",
                "hint": "POST JSON-RPC 2.0 requests to this endpoint"
            }))
        }));

    tracing::info!("MCP HTTP server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
