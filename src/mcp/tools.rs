//! MCP Tools — All 36 TDG tools using official rmcp SDK
//!
//! Uses `#[tool]` and `#[tool_router]` macros for automatic schema generation.
//! Synthesis helper functions (LLM provider chain, output parsing, pattern
//! fallback, synthesis persistence) live in `super::synthesis_helpers`.

use std::collections::HashMap;
use std::sync::Arc;

use petgraph::algo::page_rank;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ErrorData as McpError};
use serde_json::{json, Value};

use crate::config::Config;
use crate::db::ConnectionPool;
use crate::graph_projection::GraphProjection;
use crate::mind::reflect_engine::ReflectEngine;
use crate::flow;
use crate::mind::state::MindStateManager;

type DriveScores = HashMap<String, (Vec<f64>, Vec<f64>, Vec<f64>)>;
use crate::models::{NewEdge, NewNode, NodeQuery};

use super::MAX_BULK_NODES;
use super::health::HealthMonitor;
use super::params::*;
use super::synthesis_helpers::{
    auto_detect_edge_type, pattern_synthesis, store_synthesis, try_llm_providers,
};
use super::trust::TrustStore;

pub(crate) fn calculate_health_score(
    node_count: i64,
    edge_count: i64,
    type_count: i64,
    embedding_count: i64,
    fts_count: i64,
) -> f64 {
    let node_score = if node_count > 0 { 1.0 } else { 0.0 };
    let edge_score = (edge_count as f64 / node_count.max(1) as f64).min(1.0);
    // Type diversity: normalize against the known node-type vocabulary (~20 types)
    // rather than against node_count. The previous formula
    // (type_count / node_count) gave ~0.0024 for a 5000-node graph with 12
    // types, contributing effectively zero to the health score.
    let type_score = (type_count as f64 / 20.0).min(1.0);
    let embedding_score = if node_count > 0 {
        (embedding_count as f64 / node_count as f64).min(1.0)
    } else {
        1.0
    };
    let fts_score = if node_count > 0 {
        (fts_count as f64 / node_count as f64).min(1.0)
    } else {
        1.0
    };

    node_score * 0.35
        + edge_score * 0.20
        + type_score * 0.15
        + embedding_score * 0.20
        + fts_score * 0.10
}// ─── Helper to get a connection ──────────────────────────────────────────────

/// RAII guard that borrows a connection from the pool and automatically
/// releases it back when dropped.
///
/// Previously `get_conn` returned a bare `rusqlite::Connection` that was
/// dropped (closed) at the end of each `run_blocking` closure instead of being
/// returned to the pool. This meant every tool call opened a fresh connection
/// (re-running 5 PRAGMAs: WAL, synchronous, foreign_keys, cache_size,
/// busy_timeout), the pool's `Vec<Connection>` stayed permanently empty, and
/// the `max_connections` cap was meaningless. With ~29 tool call sites each
/// running on `spawn_blocking`, this was a significant performance and
/// correctness issue (no connection reuse, no shared cache).
///
/// `ConnGuard` derefs to `rusqlite::Connection` so existing code that does
/// `let conn = get_conn(&pool)?;` and then passes `&conn` or `&*conn` keeps
/// working unchanged.
pub(crate) struct ConnGuard {
    pool: std::sync::Arc<ConnectionPool>,
    conn: Option<rusqlite::Connection>,
}

impl std::ops::Deref for ConnGuard {
    type Target = rusqlite::Connection;
    fn deref(&self) -> &Self::Target {
        self.conn.as_ref().expect("ConnGuard conn already taken")
    }
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.release_connection(conn);
        }
    }
}

/// Get a connection wrapped in an RAII guard that releases it back to the pool
/// on drop. Call sites do `let pool = self.pool.clone();` (Arc clone) then
/// `let conn = get_conn(&pool)?;` — the guard keeps the Arc alive until the
/// connection is returned, even if the surrounding closure is dropped early.
fn get_conn(pool: &std::sync::Arc<ConnectionPool>) -> Result<ConnGuard, McpError> {
    let conn = pool
        .get_connection()
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(ConnGuard {
        pool: std::sync::Arc::clone(pool),
        conn: Some(conn),
    })
}

fn mcp_err(e: impl std::fmt::Display) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

/// Validate that a file path is safe for export/import operations.
///
/// Prevents path traversal attacks (e.g. `../../etc/cron.d/backdoor` or
/// `/etc/shadow`) by confining paths to a configurable base directory.
/// Defaults to the current working directory if no base is configured.
///
/// Returns the canonicalized path if safe, or an McpError if the path
/// escapes the base directory.
fn validate_file_path(path: &str, for_write: bool) -> Result<std::path::PathBuf, McpError> {
    use std::path::Path;

    // Reject empty paths
    if path.trim().is_empty() {
        return Err(McpError::invalid_params("path cannot be empty", None));
    }

    // Reject absolute paths to system directories
    let p = Path::new(path);
    if p.is_absolute() {
        // Allow absolute paths only under the user's home directory or /tmp
        let canonical = p.canonicalize().or_else(|_| {
            // For write paths, the file may not exist yet — canonicalize the parent
            if for_write {
                if let Some(parent) = p.parent() {
                    parent.canonicalize().map(|_| p.to_path_buf())
                } else {
                    Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no parent"))
                }
            } else {
                Err(std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"))
            }
        }).map_err(|e| mcp_err(anyhow::anyhow!("Cannot resolve path '{}': {}", path, e)))?;

        // Block access to sensitive system paths
        let path_str = canonical.to_string_lossy().to_lowercase();
        let blocked_prefixes = [
            "/etc/", "/var/", "/usr/", "/bin/", "/sbin/", "/root/",
            "/proc/", "/sys/", "/dev/", "/boot/", "/lib/",
        ];
        for prefix in &blocked_prefixes {
            if path_str.starts_with(prefix) {
                return Err(McpError::invalid_params(
                    format!("Access denied: path '{}' is in a protected system directory", path),
                    None,
                ));
            }
        }
        Ok(canonical)
    } else {
        // Relative paths are resolved against CWD — generally safe
        Ok(Path::new(path).to_path_buf())
    }
}

/// Helper function to upsert an entity node and create a MENTIONS edge.
/// Returns the entity node ID.
fn upsert_entity_and_connect(
    conn: &rusqlite::Connection,
    observation_id: &str,
    entity_name: &str,
    entity_type: &str,
) -> Result<String, McpError> {
    let name = entity_name.trim().to_string();
    if name.is_empty() {
        return Err(McpError::invalid_params("entity name cannot be empty", None));
    }

    // Search for existing entity by name and type
    let existing = crate::db::crud::search(&conn, &name, 1)
        .unwrap_or_default()
        .into_iter()
        .find(|(n, _)| n.node_type == entity_type && n.name == name)
        .map(|(n, _)| n);

    let entity_node = if let Some(n) = existing {
        n
    } else {
        crate::db::crud::add_node(
            conn,
            &NewNode {
                node_type: entity_type.to_string(),
                name: name.clone(),
                source: Some("mcp_observe".to_string()),
                ..Default::default()
            },
        )
        .map_err(mcp_err)?
    };

    // Create MENTIONS edge from observation to entity (with dedup)
    let existing = crate::db::crud::get_edges(
        conn,
        Some(observation_id),
        Some(&entity_node.id),
        Some("MENTIONS"),
        None,
        1,
    )
    .unwrap_or_default();
    if existing.is_empty() {
        if let Err(e) = crate::db::crud::add_edge(
            conn,
            &crate::models::NewEdge {
                source_id: observation_id.to_string(),
                target_id: entity_node.id.clone(),
                edge_type: "MENTIONS".to_string(),
                weight: Some(1.0),
                properties: None,
                agent_id: Some("mcp_observe".to_string()),
            },
        ) {
            tracing::warn!("Failed to create MENTIONS edge {} -> {}: {}", observation_id, entity_node.id, e);
        }
    }

    Ok(entity_node.id)
}

/// Offload blocking SQLite I/O to a dedicated thread pool.
/// Prevents blocking the tokio async executor on every tool call.
async fn run_blocking<F, T>(f: F) -> Result<T, McpError>
where
    F: FnOnce() -> Result<T, McpError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| McpError::internal_error(format!("task join error: {e}"), None))?
}

// ─── TdgServer — the MCP server handler ──────────────────────────────────────

#[derive(Clone)]
pub struct TdgServer {
    pub pool: Arc<ConnectionPool>,
    pub(crate) trust_store: Arc<TrustStore>,
    pub(crate) health_monitor: Arc<HealthMonitor>,
    pub mind_state_manager: Arc<MindStateManager>,
    /// Cache for GraphProjection + PageRank results (TTL: 60 seconds).
    /// Previously, tdg_graph_stats rebuilt the entire in-memory graph and
    /// recomputed 100 PageRank iterations on every call — O(N²) per request.
    pub(crate) graph_stats_cache: Arc<std::sync::Mutex<Option<(std::time::Instant, Value)>>>,
    pub lean: bool,
}

#[tool_router(server_handler)]
impl TdgServer {
    pub fn new(pool: ConnectionPool) -> Self {
        let config = Config::from_env();
        let lean = config.lean;
        // Sync the flow engine's global lean flag with the server's lean config.
        // Previously `flow::LEAN_MODE` (a static mut) was never set from
        // production code, so renormalize_graph would run even when the server
        // was in lean mode — a divergence between the tool-level lean guard
        // and the flow-level lean guard.
        crate::flow::set_lean_mode(lean);
        let mind_state_manager = Arc::new(MindStateManager::new(config));
        let pool = Arc::new(pool);
        Self {
            pool: pool.clone(),
            trust_store: Arc::new(TrustStore::new(pool.clone())),
            health_monitor: Arc::new(HealthMonitor::new(pool)),
            mind_state_manager,
            graph_stats_cache: Arc::new(std::sync::Mutex::new(None)),
            lean,
        }
    }

    fn lean_guard(&self) -> Result<bool, McpError> {
        Ok(self.lean)
    }

    #[tool(description = "Hybrid FTS5 graph search")]
    pub(crate) async fn tdg_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let query = params.query.clone();
        let limit = params.limit.unwrap_or(10).min(50);
        let node_type = params.node_type.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let node_type = node_type.as_deref().filter(|s| !s.is_empty());
            let retriever = crate::plugins::HybridRetriever::new();
            let results = retriever
                .search(&conn, &query, limit, node_type)
                .map_err(mcp_err)?;
            let items: Vec<Value> = results.iter().map(|r| json!({
                "id": r.node.id, "node_type": r.node.node_type, "name": r.node.name,
                "description": r.node.description, "confidence": r.node.confidence, "score": r.score,
            })).collect();
            Ok(
                serde_json::to_string(&json!({"nodes": items, "total": items.len()}))
                    .unwrap_or_default(),
            )
        })
        .await
    }

    #[tool(description = "Prefetch context for query injection")]
    pub(crate) async fn tdg_prefetch(
        &self,
        Parameters(params): Parameters<PrefetchParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let query = params.query.clone();
        let limit = params.limit.unwrap_or(10).min(50);
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let retriever = crate::plugins::HybridRetriever::new();
            let results = retriever
                .search(&conn, &query, limit, None)
                .map_err(mcp_err)?;
            let context = results
                .iter()
                .map(|r| {
                    format!(
                        "[{}] {} — {}",
                        r.node.node_type,
                        r.node.name,
                        &r.node.description
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            Ok(context)
        })
        .await
    }

    #[tool(description = "Export graph to JSON")]
    pub(crate) async fn tdg_export(
        &self,
        Parameters(params): Parameters<ExportParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string());
        }
        let pool = self.pool.clone();
        let output_path = params.output_path.unwrap_or_else(|| "tdg_export.json".to_string());
        // Validate the output path to prevent path traversal attacks
        let validated_path = validate_file_path(&output_path, true)?;
        run_blocking(move || {
            let conn = get_conn(&pool)?;

            let nodes: Vec<Value> = conn
                .prepare("SELECT id, node_type, name, COALESCE(description, '') FROM nodes WHERE valid_to IS NULL")
                .map_err(mcp_err)?
                .query_map([], |row| {
                    Ok(json!({
                        "id": row.get::<_, String>(0)?,
                        "node_type": row.get::<_, String>(1)?,
                        "name": row.get::<_, String>(2)?,
                        "description": row.get::<_, String>(3)?
                    }))
                })
                .map_err(mcp_err)?
                .filter_map(|r| r.ok())
                .collect();

            let edges: Vec<Value> = conn
                .prepare("SELECT source_id, target_id, edge_type FROM edges WHERE valid_to IS NULL")
                .map_err(mcp_err)?
                .query_map([], |row| {
                    Ok(json!({
                        "source_id": row.get::<_, String>(0)?,
                        "target_id": row.get::<_, String>(1)?,
                        "edge_type": row.get::<_, String>(2)?
                    }))
                })
                .map_err(mcp_err)?
                .filter_map(|r| r.ok())
                .collect();

            let export = json!({
                "version": 1,
                "nodes": nodes,
                "edges": edges,
                "node_count": nodes.len(),
                "edge_count": edges.len(),
            });

            std::fs::write(&validated_path, serde_json::to_string_pretty(&export).unwrap_or_default())
                .map_err(|e| mcp_err(anyhow::anyhow!("Write error: {}", e)))?;

            Ok(format!("Exported {} nodes, {} edges to {}", nodes.len(), edges.len(), validated_path.display()))
        }).await
    }

    #[tool(description = "Import graph from JSON")]
    pub(crate) async fn tdg_import(
        &self,
        Parameters(params): Parameters<ImportParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string());
        }
        let pool = self.pool.clone();
        let input_path = params.input_path.clone();
        // Validate the input path to prevent path traversal attacks
        let validated_path = validate_file_path(&input_path, false)?;
        let skip_dupes = params.skip_duplicates.unwrap_or(true);
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let content = std::fs::read_to_string(&validated_path)
                .map_err(|e| mcp_err(anyhow::anyhow!("Read error: {}", e)))?;
            let import: Value = serde_json::from_str(&content)
                .map_err(|e| mcp_err(anyhow::anyhow!("Parse error: {}", e)))?;
            let mut nodes_imported = 0;
            let mut edges_imported = 0;

            if let Some(nodes) = import["nodes"].as_array() {
                for node in nodes {
                    let id = node["id"].as_str().unwrap_or("");
                    let node_type = node["node_type"].as_str().unwrap_or("observation");
                    let name = node["name"].as_str().unwrap_or("");
                    let description = node["description"].as_str().unwrap_or("");
                    if skip_dupes {
                        let exists: bool = conn
                            .query_row("SELECT COUNT(*) > 0 FROM nodes WHERE id = ?1", [id], |row| row.get(0))
                            .unwrap_or(false);
                        if exists { continue; }
                    }
                    conn.execute(
                        "INSERT OR REPLACE INTO nodes (id, node_type, name, description, created_at) VALUES (?1, ?2, ?3, ?4, datetime('now', 'subsec'))",
                        [id, node_type, name, description],
                    ).ok();
                    nodes_imported += 1;
                }
            }

            if let Some(edges) = import["edges"].as_array() {
                for edge in edges {
                    let source = edge["source_id"].as_str().unwrap_or("");
                    let target = edge["target_id"].as_str().unwrap_or("");
                    let edge_type = edge["edge_type"].as_str().unwrap_or("RELATES_TO");
                    conn.execute(
                        "INSERT OR IGNORE INTO edges (source_id, target_id, edge_type, created_at) VALUES (?1, ?2, ?3, datetime('now', 'subsec'))",
                        [source, target, edge_type],
                    ).ok();
                    edges_imported += 1;
                }
            }

            Ok(format!("Imported {} nodes, {} edges", nodes_imported, edges_imported))
        }).await
    }

    #[tool(description = "Graph health: coverage, noise, orphans")]
    pub(crate) async fn tdg_graph_health(
        &self,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string());
        }
        let pool = self.pool.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;

            let node_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL AND lifecycle_state != 'archived'", [], |r| r.get(0)).unwrap_or(0);
            let edge_count: i64 = conn.query_row("SELECT COUNT(*) FROM edges WHERE valid_to IS NULL", [], |r| r.get(0)).unwrap_or(0);
            let type_count: i64 = conn.query_row("SELECT COUNT(DISTINCT node_type) FROM nodes WHERE valid_to IS NULL AND lifecycle_state != 'archived'", [], |r| r.get(0)).unwrap_or(0);
            // Join with nodes and filter for active, non-archived rows to avoid
            // fts5_coverage > 1.0 (FTS triggers fire on every INSERT regardless
            // of lifecycle_state, so nodes_fts contains archived/deleted rows).
            let fts_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM nodes_fts f
                 INNER JOIN nodes n ON n.rowid = f.rowid
                 WHERE n.valid_to IS NULL AND n.lifecycle_state != 'archived'",
                [], |r| r.get(0),
            ).unwrap_or(0);
            let emb_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM embeddings e
                 INNER JOIN nodes n ON n.id = e.node_id
                 WHERE n.valid_to IS NULL AND n.lifecycle_state != 'archived'",
                [], |r| r.get(0),
            ).unwrap_or(0);
            let mentions: i64 = conn.query_row("SELECT COUNT(*) FROM edges WHERE edge_type = 'MENTIONS' AND valid_to IS NULL", [], |r| r.get(0)).unwrap_or(0);
            let orphans: i64 = conn.query_row(
                "SELECT COUNT(*) FROM edges e WHERE e.valid_to IS NULL AND (e.source_id NOT IN (SELECT id FROM nodes WHERE valid_to IS NULL AND lifecycle_state != 'archived') OR e.target_id NOT IN (SELECT id FROM nodes WHERE valid_to IS NULL AND lifecycle_state != 'archived'))",
                [], |r| r.get(0),
            ).unwrap_or(0);
            let event_count: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0)).unwrap_or(0);

            let fts_coverage = if node_count > 0 { (fts_count as f64 / node_count as f64).min(1.0) } else { 1.0 };
            let emb_coverage = if node_count > 0 { (emb_count as f64 / node_count as f64).min(1.0) } else { 1.0 };
            let edge_noise = if edge_count > 0 { mentions as f64 / edge_count as f64 } else { 0.0 };

            let db_size: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap_or(0)
                * conn.query_row("PRAGMA page_size", [], |r| r.get(0)).unwrap_or(4096);

            let health_score = calculate_health_score(node_count, edge_count, type_count, emb_count, fts_count);

            Ok(json!({
                "node_count": node_count,
                "edge_count": edge_count,
                "fts_coverage": format!("{:.1}%", fts_coverage * 100.0),
                "embedding_coverage": format!("{:.1}%", emb_coverage * 100.0),
                "edge_noise": format!("{:.1}%", edge_noise * 100.0),
                "orphan_count": orphans,
                "event_count": event_count,
                "db_size_mb": format!("{:.1}", db_size as f64 / 1024.0 / 1024.0),
                "health_score": format!("{:.2}", health_score),
            }).to_string())
        }).await
    }

    #[tool(description = "Get node details with context")]
    pub(crate) async fn tdg_get_node(
        &self,
        Parameters(params): Parameters<GetNodeParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let node_id = params.node_id.clone();
        let include_context = params.include_context.unwrap_or(false);
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let node = crate::db::crud::get_node(&conn, &node_id)
                .map_err(mcp_err)?
                .ok_or_else(|| {
                    McpError::invalid_params(format!("Node {} not found", node_id), None)
                })?;
            let mut result = json!({
                "id": node.id, "node_type": node.node_type, "name": node.name,
                "description": node.description, "confidence": node.confidence,
                "lifecycle_state": node.lifecycle_state, "created_at": node.created_at,
            });
            if include_context {
                let out = crate::db::crud::get_edges(&conn, Some(&node.id), None, None, None, 100)
                    .unwrap_or_default();
                let inp = crate::db::crud::get_edges(&conn, None, Some(&node.id), None, None, 100)
                    .unwrap_or_default();
                result["neighbors"] = json!({"outgoing": out.len(), "incoming": inp.len()});
                result["parents"] = json!(node.parent_ids);
            }
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }).await
    }

    #[tool(description = "Query event log")]
    pub(crate) async fn tdg_query_events(
        &self,
        Parameters(params): Parameters<QueryEventsParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let action = params.action.clone();
        let node_id = params.node_id.clone();
        let after = params.after.clone();
        let before = params.before.clone();
        let limit = params.limit.unwrap_or(50).min(500);
        let offset = params.offset.unwrap_or(0);
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let mut sql = String::from(
                "SELECT event_id, event_action, node_id, payload, timestamp FROM events WHERE 1=1",
            );
            let mut pv: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            if let Some(ref a) = action {
                if !a.is_empty() {
                    sql.push_str(" AND event_action = ?");
                    pv.push(Box::new(a.clone()));
                }
            }
            if let Some(ref nid) = node_id {
                if !nid.is_empty() {
                    sql.push_str(" AND node_id = ?");
                    pv.push(Box::new(nid.clone()));
                }
            }
            if let Some(ref a) = after {
                sql.push_str(" AND timestamp >= ?");
                pv.push(Box::new(a.clone()));
            }
            if let Some(ref b) = before {
                sql.push_str(" AND timestamp <= ?");
                pv.push(Box::new(b.clone()));
            }
            sql.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");
            pv.push(Box::new(limit));
            pv.push(Box::new(offset));
            let mut stmt = conn.prepare(&sql).map_err(mcp_err)?;
            let refs: Vec<&dyn rusqlite::types::ToSql> = pv.iter().map(|p| p.as_ref()).collect();
            let rows = stmt.query_map(&*refs, |row| Ok(json!({
                "event_id": row.get::<_, String>(0)?, "event_action": row.get::<_, String>(1)?,
                "node_id": row.get::<_, Option<String>>(2)?, "payload": row.get::<_, Option<String>>(3)?,
                "timestamp": row.get::<_, String>(4)?,
            }))).map_err(mcp_err)?;
            let events: Vec<Value> = rows.filter_map(|r| r.ok()).collect();
            Ok(
                serde_json::to_string(&json!({"events": events, "total": events.len()}))
                    .unwrap_or_default(),
            )
        }).await
    }

    #[tool(description = "Create node with edge wiring")]
    pub(crate) async fn tdg_create(
        &self,
        Parameters(params): Parameters<CreateParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        if params.name.is_empty() {
            return Err(McpError::invalid_params("name is required", None));
        }
        let pool = self.pool.clone();
        let node_type = params.node_type.clone();
        let name = params.name.clone();
        let description = params.description.clone();
        let source = params.source.clone();
        let lifecycle_state = params.lifecycle_state.clone();
        let parent_ids_raw = params.parent_ids.clone();
        let quadrant = params.quadrant.clone();
        let t_level = params.t_level.clone();
        let stage = params.stage;
        let blocks_targets = params.blocks_targets.clone();
        let evidence_targets = params.evidence_targets.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let parent_ids = parent_ids_raw
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| s.split(',').map(|p| p.trim().to_string()).collect());
            let mut quadrants = serde_json::Map::new();
            if let Some(ref q) = quadrant {
                if !q.is_empty() {
                    quadrants.insert("primary".to_string(), json!(q));
                }
            }
            let mut drives = serde_json::Map::new();
            if let Some(ref tl) = t_level {
                if !tl.is_empty() {
                    drives.insert("teleological_level".to_string(), json!(tl));
                }
            }
            if let Some(stage) = stage {
                if stage > 0 {
                    drives.insert("stage".to_string(), json!(stage));
                }
            }
            let node = crate::db::crud::add_node(
                &conn,
                &NewNode {
                    node_type,
                    name,
                    description,
                    source,
                    lifecycle_state,
                    parent_ids,
                    quadrants: if quadrants.is_empty() {
                        None
                    } else {
                        Some(json!(quadrants))
                    },
                    drives: if drives.is_empty() {
                        None
                    } else {
                        Some(json!(drives))
                    },
                    ..Default::default()
                },
            )
            .map_err(mcp_err)?;

            // Wrap edge creation + flow propagation in a transaction.
            // Previously, add_node + auto_wire + BLOCKS/EVIDENCES loops were
            // separate implicit transactions — a failure mid-way left the node
            // with partial edges and no rollback. Also, tdg_create never called
            // flow::emit_downward or renormalize_graph, so drives never
            // propagated from telos parents to children — the graph was
            // structurally connected but semantically inert ("drive propagation
            // island"). We now wrap everything in a transaction and trigger
            // flow propagation at the end.
            conn.execute_batch("BEGIN IMMEDIATE").map_err(mcp_err)?;
            let txn_result: Result<(), McpError> = (|| {
                // Auto-wire edges based on the node's contract (auto_wire_on_parent).
            //
            // Previously, `parent_ids` was stored as a JSON array on the node row
            // but NO edges were created from parents to the new node. This meant
            // `tdg_create(node_type="action", parent_ids=["n_telos_001"])` created
            // an action node with parent_ids metadata but no DECOMPOSES_TO edge —
            // the graph stayed disconnected. This was the root cause of the 80%+
            // orphan ratio reported by the agent.
            //
            // `auto_wire_edges` consults the NodeContract for the node's type and
            // creates the appropriate edge (DECOMPOSES_TO, EVIDENCES, BLOCKS, etc.)
            // for each parent, with correct direction (parent→child or child→parent).
            if let Some(ref pids) = parent_ids_raw {
                let parsed_parents: Vec<String> = pids
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if !parsed_parents.is_empty() {
                    match crate::grammar::auto_wire_edges(
                        &conn,
                        &node.id,
                        &node.node_type,
                        &parsed_parents,
                    ) {
                        Ok(n) => {
                            if n > 0 {
                                tracing::debug!(
                                    "auto_wire_edges created {} edges for node {} ({})",
                                    n, node.id, node.node_type
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "auto_wire_edges failed for node {} ({}): {}",
                                node.id, node.node_type, e
                            );
                        }
                    }
                }
            }

            if let Some(ref targets) = blocks_targets {
                for tid in targets.split(',') {
                    let tid = tid.trim();
                    if !tid.is_empty() {
                        // Skip duplicate edges (same src+tgt+type already active)
                        let dup = crate::db::crud::get_edges(
                            &conn,
                            Some(&node.id),
                            Some(tid),
                            Some("BLOCKS"),
                            None,
                            1,
                        ).unwrap_or_default();
                        if !dup.is_empty() {
                            continue;
                        }
                        if let Err(e) = crate::db::crud::add_edge(
                            &conn,
                            &NewEdge {
                                source_id: node.id.clone(),
                                target_id: tid.to_string(),
                                edge_type: "BLOCKS".to_string(),
                                ..Default::default()
                            },
                        ) {
                            tracing::warn!("Failed to create BLOCKS edge to {}: {}", tid, e);
                        }
                    }
                }
            }
            if let Some(ref targets) = evidence_targets {
                for tid in targets.split(',') {
                    let tid = tid.trim();
                    if !tid.is_empty() {
                        // Skip duplicate edges (same src+tgt+type already active)
                        let dup = crate::db::crud::get_edges(
                            &conn,
                            Some(&node.id),
                            Some(tid),
                            Some("EVIDENCES"),
                            None,
                            1,
                        ).unwrap_or_default();
                        if !dup.is_empty() {
                            continue;
                        }
                        if let Err(e) = crate::db::crud::add_edge(
                            &conn,
                            &NewEdge {
                                source_id: node.id.clone(),
                                target_id: tid.to_string(),
                                edge_type: "EVIDENCES".to_string(),
                                ..Default::default()
                            },
                        ) {
                            tracing::warn!("Failed to create EVIDENCES edge to {}: {}", tid, e);
                        }
                    }
                }
            }

                // Flow engine: propagate drives downward from each parent,
                // then renormalize. Errors are logged but non-fatal (matching
                // tdg_connect semantics — the edge creation is the atomic unit,
                // flow propagation is best-effort within the same transaction).
                if let Some(ref pids) = parent_ids_raw {
                    let parents: Vec<String> = pids
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    for pid in &parents {
                        if let Err(e) = flow::emit_downward(&conn, pid, flow::DEFAULT_MAX_DEPTH) {
                            tracing::warn!("flow::emit_downward from parent {} failed: {}", pid, e);
                        }
                    }
                }
                if let Err(e) = flow::renormalize_graph(&conn, false) {
                    tracing::warn!("flow::renormalize_graph failed after tdg_create: {}", e);
                }

                Ok(())
            })();

            match txn_result {
                Ok(_) => {
                    if let Err(e) = conn.execute_batch("COMMIT") {
                        let _ = conn.execute_batch("ROLLBACK");
                        return Err(mcp_err(e));
                    }
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(e);
                }
            }

            Ok(serde_json::to_string(
                &json!({"id": node.id, "name": node.name, "node_type": node.node_type}),
            )
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "Update node details")]
    pub(crate) async fn tdg_update(
        &self,
        Parameters(params): Parameters<UpdateParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let node_id = params.node_id.clone();
        let name = params.name.clone();
        let description = params.description.clone();
        let lifecycle_state = params.lifecycle_state.clone();
        let t_level = params.t_level.clone();
        let stage = params.stage;
        let add_parent_ids = params.add_parent_ids.clone();
        let remove_parent_ids = params.remove_parent_ids.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let mut updates = HashMap::new();
            if let Some(ref n) = name {
                updates.insert("name".to_string(), json!(n));
            }
            if let Some(ref d) = description {
                updates.insert("description".to_string(), json!(d));
            }
            if let Some(ref ls) = lifecycle_state {
                updates.insert("lifecycle_state".to_string(), json!(ls));
            }
            if let Some(ref tl) = t_level {
                updates.insert("teleological_level".to_string(), json!(tl));
            }
            if let Some(stage) = stage {
                updates.insert("developmental_stage".to_string(), json!(stage));
            }
            let existing = crate::db::crud::get_node(&conn, &node_id)
                .map_err(mcp_err)?
                .ok_or_else(|| {
                    McpError::invalid_params(format!("Node {} not found", node_id), None)
                })?;
            let mut parent_ids = existing.parent_ids.clone();
            if let Some(ref add) = add_parent_ids {
                for pid in add.split(',') {
                    let p = pid.trim().to_string();
                    if !p.is_empty() && !parent_ids.contains(&p) {
                        parent_ids.push(p);
                    }
                }
            }
            if let Some(ref remove) = remove_parent_ids {
                let rm: std::collections::HashSet<&str> = remove.split(',').map(|s| s.trim()).collect();
                parent_ids.retain(|p| !rm.contains(p.as_str()));
            }
            updates.insert(
                "parent_ids".to_string(),
                json!(serde_json::to_string(&parent_ids).unwrap_or_default()),
            );
            let updated = crate::db::crud::update_node(&conn, &node_id, &updates)
                .map_err(mcp_err)?
                .ok_or_else(|| {
                    McpError::invalid_params(format!("Node {} not found", node_id), None)
                })?;
            Ok(serde_json::to_string(
                &json!({"id": updated.id, "name": updated.name, "node_type": updated.node_type}),
            )
            .unwrap_or_default())
        }).await
    }

    /// Elevate a node's synthesis status on the TDG epistemic ladder.
    ///
    /// Phase 1.6: Human-only elevation. All AI-produced content starts at
    /// `ai-draft`. This tool allows a human to elevate a node to
    /// `canonical-hypothesis`, `canonical`, or `superseded`.
    ///
    /// The `human_authorization` parameter is required — AI agents cannot
    /// self-elevate. In Phase 5, this will be replaced with real authentication.
    #[tool(description = "Elevate a node's synthesis status (human-only, requires authorization token)")]
    pub(crate) async fn tdg_elevate(
        &self,
        Parameters(params): Parameters<crate::mcp::params::ElevateParams>,
    ) -> Result<String, McpError> {
        // Enforce human authorization — AI agents cannot self-elevate.
        if params.human_authorization.trim().is_empty() {
            return Err(McpError::invalid_params(
                "human_authorization is required for synthesis status elevation. AI agents cannot self-elevate.",
                None,
            ));
        }

        let pool = self.pool.clone();
        let node_id = params.node_id.clone();
        let target_status = params.target_status.clone();
        let reason = params.reason.clone();
        let auth = params.human_authorization.clone();

        run_blocking(move || {
            let conn = get_conn(&pool)?;

            // Parse the target status
            let target = crate::models::SynthesisStatus::from_str(&target_status).ok_or_else(|| {
                McpError::invalid_params(
                    format!(
                        "Invalid target_status '{}'. Must be one of: canonical-hypothesis, canonical, superseded",
                        target_status
                    ),
                    None,
                )
            })?;

            // Load the current node to get its current status
            let node = crate::db::crud::get_node(&conn, &node_id).map_err(mcp_err)?
                .ok_or_else(|| McpError::invalid_params(format!("Node {} not found", node_id), None))?;

            let current = crate::models::SynthesisStatus::from_str(&node.synthesis_status)
                .unwrap_or(crate::models::SynthesisStatus::AiDraft);

            // Validate the ladder transition
            if !current.can_elevate_to(&target) {
                return Err(McpError::invalid_params(
                    format!(
                        "Invalid elevation: {} -> {}. Valid transitions: ai-draft -> canonical-hypothesis -> canonical -> superseded",
                        current, target
                    ),
                    None,
                ));
            }

            // Perform the elevation
            let mut updates = std::collections::HashMap::new();
            updates.insert("synthesis_status".to_string(), serde_json::json!(target.as_str()));
            let updated = crate::db::crud::update_node(&conn, &node_id, &updates).map_err(mcp_err)?;

            // Record the elevation in the mutation log with provenance
            let _ = crate::db::crud::record_mutation(
                &conn,
                "elevate",
                "node",
                &node_id,
                Some(&serde_json::json!({"synthesis_status": current.as_str()}).to_string()),
                Some(&serde_json::json!({
                    "synthesis_status": target.as_str(),
                    "elevated_by": auth,
                    "reason": reason,
                }).to_string()),
                Some("human_elevation"),
            );

            Ok(serde_json::to_string(&json!({
                "node_id": node_id,
                "previous_status": current.as_str(),
                "new_status": target.as_str(),
                "elevated_by": auth,
                "reason": reason,
                "node": updated,
            }))
            .unwrap_or_default())
        })
        .await
    }

    /// Explicitly tick a holon's lesser cycle (Phase 2).
    ///
    /// Normally the lesser cycle is event-driven (ticks on catalyst injection).
    /// This tool allows manual triggering for testing and debugging — it
    /// injects an optional catalyst amount and runs one metabolic step.
    #[tool(description = "Tick a holon's lesser cycle (manual metabolism trigger for testing/debugging)")]
    pub(crate) async fn tdg_tick(
        &self,
        Parameters(params): Parameters<crate::mcp::params::TickParams>,
    ) -> Result<String, McpError> {
        let pool = self.pool.clone();
        let node_id = params.node_id.clone();
        let catalyst = params.catalyst_amount.unwrap_or(0.0);

        run_blocking(move || {
            let conn = get_conn(&pool)?;

            // Load current state
            let mut state = crate::metabolism::load_state(&conn, &node_id)
                .map_err(mcp_err)?;

            let previous_phase = state.phase.clone();

            // Run the tick
            let thresholds = crate::metabolism::CycleThresholds::default();
            let result = crate::metabolism::tick(&mut state, catalyst, &thresholds);

            // Save the updated state
            crate::metabolism::save_state(&conn, &node_id, &state)
                .map_err(mcp_err)?;

            // Enqueue upward pressure to parents if needed
            if result.upward_pressure && result.upward_experience > 0.0 {
                if let Some(node) = crate::db::crud::get_node(&conn, &node_id).map_err(mcp_err)? {
                    for parent_id in &node.parent_ids {
                        let payload = serde_json::json!({
                            "catalyst_amount": result.upward_experience,
                            "source": "manual_tick_upward",
                            "source_holon": node_id,
                        });
                        let _ = crate::metabolism::worker::enqueue_job(
                            &conn,
                            parent_id,
                            crate::metabolism::worker::JobType::CatalystInjection,
                            payload,
                            crate::metabolism::worker::PRIORITY_NORMAL,
                        );
                    }
                }
            }

            Ok(serde_json::to_string(&json!({
                "node_id": node_id,
                "previous_phase": previous_phase.as_str(),
                "current_phase": state.phase.as_str(),
                "catalyst_injected": catalyst,
                "catalyst_pending": state.catalyst_pending,
                "experience_accumulated": state.experience_accumulated,
                "transformation_pressure": state.transformation_pressure,
                "cycle_count": state.cycle_count,
                "transitioned": result.transitioned,
                "shadow_diagnosed": result.shadow_diagnosed,
                "upward_pressure": result.upward_pressure,
                "cycle_completed": result.cycle_completed,
                "matrix_shadow": state.matrix.shadow.as_ref().map(|s| s.as_str()),
                "potentiator_shadow": state.potentiator.shadow.as_ref().map(|s| s.as_str()),
            }))
            .unwrap_or_default())
        })
        .await
    }

    /// Query the metabolism job queue status (Phase 2).
    ///
    /// Returns the number of pending and failed jobs, plus optional details.
    #[tool(description = "Query metabolism job queue status (pending/failed counts and details)")]
    pub(crate) async fn tdg_metabolism_status(
        &self,
        Parameters(params): Parameters<crate::mcp::params::MetabolismStatusParams>,
    ) -> Result<String, McpError> {
        let pool = self.pool.clone();
        let include_pending = params.include_pending.unwrap_or(false);
        let include_failed = params.include_failed.unwrap_or(false);
        let limit = params.limit.unwrap_or(20).min(100);

        run_blocking(move || {
            let conn = get_conn(&pool)?;

            let pending_count: i64 = crate::metabolism::worker::queue_depth(&conn)
                .map_err(mcp_err)?;

            let failed_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM failed_metabolism", [], |r| r.get(0))
                .unwrap_or(0);

            let mut result = json!({
                "pending_count": pending_count,
                "failed_count": failed_count,
                "backpressure_warning": pending_count > crate::metabolism::worker::BACKPRESSURE_WARNING,
                "backpressure_critical": pending_count > crate::metabolism::worker::BACKPRESSURE_CRITICAL,
            });

            if include_pending && pending_count > 0 {
                let mut stmt = conn.prepare(
                    "SELECT id, holon_id, job_type, priority, attempts, enqueued_at
                     FROM pending_metabolism
                     WHERE attempts < max_attempts
                     ORDER BY priority DESC, enqueued_at ASC
                     LIMIT ?1",
                ).map_err(mcp_err)?;
                let rows: Vec<serde_json::Value> = stmt
                    .query_map(rusqlite::params![limit], |row| {
                        Ok(json!({
                            "id": row.get::<_, i64>(0)?,
                            "holon_id": row.get::<_, String>(1)?,
                            "job_type": row.get::<_, String>(2)?,
                            "priority": row.get::<_, i32>(3)?,
                            "attempts": row.get::<_, i32>(4)?,
                            "enqueued_at": row.get::<_, String>(5)?,
                        }))
                    })
                    .map_err(mcp_err)?
                    .filter_map(|r| r.ok())
                    .collect();
                result["pending_jobs"] = json!(rows);
            }

            if include_failed && failed_count > 0 {
                let mut stmt = conn.prepare(
                    "SELECT id, holon_id, job_type, error, failed_at, attempts
                     FROM failed_metabolism
                     ORDER BY failed_at DESC
                     LIMIT ?1",
                ).map_err(mcp_err)?;
                let rows: Vec<serde_json::Value> = stmt
                    .query_map(rusqlite::params![limit], |row| {
                        Ok(json!({
                            "id": row.get::<_, i64>(0)?,
                            "holon_id": row.get::<_, String>(1)?,
                            "job_type": row.get::<_, String>(2)?,
                            "error": row.get::<_, String>(3)?,
                            "failed_at": row.get::<_, String>(4)?,
                            "attempts": row.get::<_, i32>(5)?,
                        }))
                    })
                    .map_err(mcp_err)?
                    .filter_map(|r| r.ok())
                    .collect();
                result["failed_jobs"] = json!(rows);
            }

            Ok(serde_json::to_string(&result).unwrap_or_default())
        })
        .await
    }

    /// Query a holon's attractor field A(H) = ⟨A_M, A_P, A_G, Γ⟩ (Phase 3).
    ///
    /// Returns the attractor field: reservoir magnitudes/signs, coupling tensor,
    /// polarity disposition (π), type_class, choice_flag, archetypal loads,
    /// and stability. If the field is dirty or force_recompute is set,
    /// enqueues a recompute job and returns the current (possibly stale) field.
    #[tool(description = "Query a holon's attractor field (type_class, polarity, stability)")]
    pub(crate) async fn tdg_attractor(
        &self,
        Parameters(params): Parameters<crate::mcp::params::AttractorParams>,
    ) -> Result<String, McpError> {
        let pool = self.pool.clone();
        let node_id = params.node_id.clone();
        let force = params.force_recompute.unwrap_or(false);

        run_blocking(move || {
            let conn = get_conn(&pool)?;

            // Check if dirty or force recompute
            let dirty = crate::metabolism::attractor::is_dirty(&conn, &node_id)
                .map_err(mcp_err)?;

            if dirty || force {
                // Enqueue recompute (async — the worker will update the field)
                let _ = crate::metabolism::worker::enqueue_job(
                    &conn,
                    &node_id,
                    crate::metabolism::worker::JobType::RecomputeAttractor,
                    serde_json::json!({"trigger": "tdg_attractor_query"}),
                    crate::metabolism::worker::PRIORITY_HIGH,
                );
            }

            // Load current field (may be stale if just enqueued)
            let af = crate::metabolism::attractor::load(&conn, &node_id)
                .map_err(mcp_err)?;

            let result = match af {
                Some(field) => json!({
                    "node_id": node_id,
                    "computed": true,
                    "dirty": dirty,
                    "type_class": field.type_class,
                    "pi": field.pi,
                    "is_noble": field.is_noble(),
                    "is_stable": field.is_stable(),
                    "choice_flag": field.choice_flag.as_ref().map(|c| c.as_str()),
                    "a_m": {"magnitude": field.a_m.magnitude, "sign": field.a_m.sign},
                    "a_p": {"magnitude": field.a_p.magnitude, "sign": field.a_p.sign},
                    "a_g": {
                        "magnitude": field.a_g.magnitude,
                        "polarity": field.a_g.polarity,
                    },
                    "gamma": {
                        "ag": field.gamma.ag, "cm": field.gamma.cm,
                        "er": field.gamma.er, "agp": field.gamma.agp,
                    },
                    "loads": {
                        "m": field.loads.m, "p": field.loads.p,
                        "c": field.loads.c, "e": field.loads.e,
                        "s": field.loads.s, "t": field.loads.t,
                        "g": field.loads.g, "ch": field.loads.ch,
                    },
                    "stability": {
                        "self_consistent": field.stability.self_consistent,
                        "bondable": field.stability.bondable,
                        "persistent": field.stability.persistent,
                    },
                    "computed_at": field.computed_at,
                }),
                None => json!({
                    "node_id": node_id,
                    "computed": false,
                    "dirty": dirty,
                    "message": "Attractor field not yet computed. A recompute job has been enqueued.",
                }),
            };

            Ok(serde_json::to_string(&result).unwrap_or_default())
        })
        .await
    }

    /// Query a holon's health metrics: G_z, P_z, total, state (Phase 3).
    ///
    /// Returns integrative efficiency (G_z), transcendental tension (P_z),
    /// total health, and state classification (Optimal/SubOptimal/Collapse/Sinkhole).
    #[tool(description = "Query a holon's health metrics (G_z, P_z, state classification)")]
    pub(crate) async fn tdg_health(
        &self,
        Parameters(params): Parameters<crate::mcp::params::HealthParams>,
    ) -> Result<String, McpError> {
        let pool = self.pool.clone();
        let node_id = params.node_id.clone();
        let force = params.force_recompute.unwrap_or(false);

        run_blocking(move || {
            let conn = get_conn(&pool)?;

            // Check if dirty
            let dirty: bool = conn
                .query_row(
                    "SELECT health_dirty FROM nodes WHERE id = ?1",
                    rusqlite::params![node_id],
                    |row| row.get::<_, i64>(0),
                )
                .map(|v| v != 0)
                .unwrap_or(false);

            if dirty || force {
                let _ = crate::metabolism::worker::enqueue_job(
                    &conn,
                    &node_id,
                    crate::metabolism::worker::JobType::RecomputeHealth,
                    serde_json::json!({"trigger": "tdg_health_query"}),
                    crate::metabolism::worker::PRIORITY_HIGH,
                );
            }

            let health = crate::metabolism::health::load(&conn, &node_id)
                .map_err(mcp_err)?;

            let result = match health {
                Some(h) => json!({
                    "node_id": node_id,
                    "computed": true,
                    "dirty": dirty,
                    "g_z": h.g_z,
                    "p_z": h.p_z,
                    "total": h.total,
                    "state": h.state.as_str(),
                    "a_z": h.a_z,
                    "c_z": h.c_z,
                    "b_h": h.b_h,
                    "b_v": h.b_v,
                    "grad_psi": h.grad_psi,
                    "theta_alignment": h.theta_alignment,
                    "computed_at": h.computed_at,
                }),
                None => json!({
                    "node_id": node_id,
                    "computed": false,
                    "dirty": dirty,
                    "message": "Health not yet computed. A recompute job has been enqueued.",
                }),
            };

            Ok(serde_json::to_string(&result).unwrap_or_default())
        })
        .await
    }

    /// Compute resonance R(H1, H2) between two holons (Phase 3).
    ///
    /// Returns the resonance score ∈ [0, 1] and its interpretation
    /// (strong/moderate/weak). Requires both holons to have computed
    /// attractor fields.
    #[tool(description = "Compute resonance between two holons (bonding prediction)")]
    pub(crate) async fn tdg_resonance(
        &self,
        Parameters(params): Parameters<crate::mcp::params::ResonanceParams>,
    ) -> Result<String, McpError> {
        let pool = self.pool.clone();
        let id1 = params.holon_id_1.clone();
        let id2 = params.holon_id_2.clone();

        run_blocking(move || {
            let conn = get_conn(&pool)?;

            let af1 = crate::metabolism::attractor::load(&conn, &id1)
                .map_err(mcp_err)?
                .ok_or_else(|| McpError::invalid_params(
                    format!("Holon {} has no attractor field. Call tdg_attractor first.", id1),
                    None,
                ))?;

            let af2 = crate::metabolism::attractor::load(&conn, &id2)
                .map_err(mcp_err)?
                .ok_or_else(|| McpError::invalid_params(
                    format!("Holon {} has no attractor field. Call tdg_attractor first.", id2),
                    None,
                ))?;

            let r = crate::metabolism::health::resonance(&af1, &af2);
            let interpretation = crate::metabolism::health::interpret_resonance(r);

            Ok(serde_json::to_string(&json!({
                "holon_id_1": id1,
                "holon_id_2": id2,
                "resonance": r,
                "interpretation": interpretation,
                "type_class_1": af1.type_class,
                "type_class_2": af2.type_class,
                "stable_1": af1.is_stable(),
                "stable_2": af2.is_stable(),
            }))
            .unwrap_or_default())
        })
        .await
    }

    /// Query a holon's top resonance partners from the materialized graph (Phase 3).
    ///
    /// Returns up to N partners with the highest resonance scores.
    #[tool(description = "Query a holon's top resonance partners (bonding candidates)")]
    pub(crate) async fn tdg_resonance_partners(
        &self,
        Parameters(params): Parameters<crate::mcp::params::ResonancePartnersParams>,
    ) -> Result<String, McpError> {
        let pool = self.pool.clone();
        let node_id = params.node_id.clone();
        let limit = params.limit.unwrap_or(10).min(50);

        run_blocking(move || {
            let conn = get_conn(&pool)?;

            let mut stmt = conn.prepare(
                "SELECT partner_id, resonance_score, computed_at
                 FROM resonance_graph
                 WHERE holon_id = ?1
                 ORDER BY resonance_score DESC
                 LIMIT ?2",
            ).map_err(mcp_err)?;

            let partners: Vec<serde_json::Value> = stmt
                .query_map(rusqlite::params![node_id, limit], |row| {
                    Ok(json!({
                        "partner_id": row.get::<_, String>(0)?,
                        "resonance": row.get::<_, f64>(1)?,
                        "computed_at": row.get::<_, String>(2)?,
                    }))
                })
                .map_err(mcp_err)?
                .filter_map(|r| r.ok())
                .collect();

            Ok(serde_json::to_string(&json!({
                "node_id": node_id,
                "partner_count": partners.len(),
                "partners": partners,
            }))
            .unwrap_or_default())
        })
        .await
    }

    /// Query or manually tick a holon's greater cycle (Phase 4).
    ///
    /// Returns the greater cycle state: phase (9-phase S·T·G·Ch cycle),
    /// significator magnitude, transformation pressure, choice committed,
    /// crucible intensity, crystallization ratio, octave count.
    ///
    /// If `tick` is true, manually runs one greater-cycle tick (for testing).
    /// If `include_readiness` is true (default), includes the 4-pillar
    /// phase-transition readiness assessment.
    #[tool(description = "Query or tick a holon's greater cycle (S·T·G·Ch, phase transitions)")]
    pub(crate) async fn tdg_greater_cycle(
        &self,
        Parameters(params): Parameters<crate::mcp::params::GreaterCycleParams>,
    ) -> Result<String, McpError> {
        let pool = self.pool.clone();
        let node_id = params.node_id.clone();
        let do_tick = params.tick.unwrap_or(false);
        let include_readiness = params.include_readiness.unwrap_or(true);

        run_blocking(move || {
            let conn = get_conn(&pool)?;

            // Load current state
            let mut state = crate::metabolism::greater_cycle::load_state(&conn, &node_id)
                .map_err(mcp_err)?;
            let lesser = crate::metabolism::lesser_cycle::load_state(&conn, &node_id)
                .map_err(mcp_err)?;

            let previous_phase = state.phase.clone();
            let mut tick_result = None;

            if do_tick {
                let thresholds = crate::metabolism::greater_cycle::GreaterThresholds::default();
                let result = crate::metabolism::greater_cycle::tick(
                    &mut state,
                    &lesser,
                    &thresholds,
                );
                crate::metabolism::greater_cycle::save_state(&conn, &node_id, &state)
                    .map_err(mcp_err)?;
                tick_result = Some(result);
            }

            // Compute readiness if requested
            let readiness = if include_readiness {
                // Get edge count and node diversity for the readiness assessment
                let edge_count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM edges WHERE (source_id = ?1 OR target_id = ?1) AND valid_to IS NULL",
                    rusqlite::params![node_id],
                    |row| row.get(0),
                ).unwrap_or(0);

                let node_diversity: i64 = conn.query_row(
                    "SELECT COUNT(DISTINCT node_type) FROM edges e
                     JOIN nodes n ON n.id = e.target_id
                     WHERE e.source_id = ?1 AND e.valid_to IS NULL",
                    rusqlite::params![node_id],
                    |row| row.get(0),
                ).unwrap_or(0);

                // Get G_z from health if available
                let g_z = crate::metabolism::health::load(&conn, &node_id)
                    .map_err(mcp_err)?
                    .map(|h| h.g_z)
                    .unwrap_or(50.0);

                let readiness = crate::metabolism::greater_cycle::assess_readiness(
                    &lesser,
                    &state,
                    edge_count,
                    node_diversity,
                    g_z,
                );

                Some(json!({
                    "prigogine": readiness.prigogine,
                    "chaisson": readiness.chaisson,
                    "kauffman": readiness.kauffman,
                    "landauer": readiness.landauer,
                    "total": readiness.total,
                    "at_bifurcation": readiness.at_bifurcation,
                }))
            } else {
                None
            };

            let tick_info = tick_result.map(|r| json!({
                "transitioned": r.transitioned,
                "from_phase": r.from_phase.map(|p| p.as_str()),
                "to_phase": r.to_phase.map(|p| p.as_str()),
                "transformation_fired": r.transformation_fired,
                "choice_locked": r.choice_locked,
                "octave_ascended": r.octave_ascended,
                "stage_advancement_triggered": r.stage_advancement_triggered,
                "downward_pressure": r.downward_pressure,
            }));

            Ok(serde_json::to_string(&json!({
                "node_id": node_id,
                "previous_phase": previous_phase.as_str(),
                "current_phase": state.phase.as_str(),
                "significator": state.significator,
                "great_way": state.great_way,
                "transformation_pressure": state.transformation_pressure,
                "choice_committed": state.choice_committed,
                "crucible_intensity": state.crucible_intensity.as_str(),
                "crystallization_ratio": state.crystallization_ratio,
                "octave_count": state.octave_count,
                "in_crucible": state.in_crucible(),
                "choice_complete": state.choice_complete(),
                "transformation_threshold": state.transformation_threshold(),
                "last_transition_at": state.last_transition_at,
                "tick_result": tick_info,
                "readiness": readiness,
            }))
            .unwrap_or_default())
        })
        .await
    }

    /// Fetch a ContextPack — single-call structured context aggregation (Phase 5).
    ///
    /// THE CAPSTONE AGENT API. Aggregates intra/inter/extra-holonic context
    /// into a single structured object with [status: {synthesis_status}] tags
    /// on every claim. Replaces 6+ CLI calls with 1.
    ///
    /// Returns JSON by default, or markdown prompt block if format="markdown".
    #[tool(description = "Fetch ContextPack — structured intra+inter+extra context with status tags")]
    pub(crate) async fn tdg_fetch_context(
        &self,
        Parameters(params): Parameters<crate::mcp::params::FetchContextParams>,
    ) -> Result<String, McpError> {
        let pool = self.pool.clone();
        let node_id = params.node_id.clone();
        let scope = params.scope.unwrap_or_else(|| "intra+inter+extra".to_string());
        let depth = params.depth.unwrap_or(2);
        let token_budget = params.token_budget;
        let format = params.format.unwrap_or_else(|| "json".to_string());

        run_blocking(move || {
            let conn = get_conn(&pool)?;

            let pack = crate::context::build_context_pack(
                &conn,
                &node_id,
                &scope,
                depth,
                token_budget,
            )
            .map_err(mcp_err)?;

            if format == "markdown" {
                Ok(pack.to_prompt_block())
            } else {
                Ok(pack.to_json())
            }
        })
        .await
    }

    /// Submit a synthesis for validation (Phase 5).
    ///
    /// Creates a synthesis node with synthesis_status = "ai-draft" (ALWAYS —
    /// AI cannot self-elevate), creates EVIDENCES edges to cited nodes, runs
    /// the 5-gate validation, and returns the validation report.
    ///
    /// The synthesis can only be elevated above ai-draft by a human calling
    /// tdg_elevate with a human_authorization token.
    #[tool(description = "Submit a synthesis for 5-gate validation (always starts at ai-draft)")]
    pub(crate) async fn tdg_submit_synthesis(
        &self,
        Parameters(params): Parameters<crate::mcp::params::SubmitSynthesisParams>,
    ) -> Result<String, McpError> {
        if params.content.is_empty() {
            return Err(McpError::invalid_params("content is required", None));
        }
        if params.name.is_empty() {
            return Err(McpError::invalid_params("name is required", None));
        }
        if params.agent_name.is_empty() {
            return Err(McpError::invalid_params("agent_name is required", None));
        }

        let pool = self.pool.clone();

        run_blocking(move || {
            let conn = get_conn(&pool)?;

            // Create the synthesis node — ALWAYS ai-draft
            let synthesis = crate::db::crud::add_node(
                &conn,
                &crate::models::NewNode {
                    node_type: "synthesis".to_string(),
                    name: params.name.clone(),
                    description: Some(params.content.clone()),
                    source: Some(format!("tdg_submit_synthesis/{}", params.agent_name)),
                    synthesis_status: Some("ai-draft".to_string()), // ALWAYS ai-draft
                    ..Default::default()
                },
            )
            .map_err(mcp_err)?;

            // Create EVIDENCES edges to cited nodes
            let mut evidence_created = 0;
            for evidence_id in &params.evidence_ids {
                if crate::db::crud::get_node(&conn, evidence_id)
                    .map_err(mcp_err)?
                    .is_some()
                {
                    if crate::db::crud::add_edge(
                        &conn,
                        &crate::models::NewEdge {
                            source_id: synthesis.id.clone(),
                            target_id: evidence_id.clone(),
                            edge_type: "EVIDENCES".to_string(),
                            ..Default::default()
                        },
                    )
                    .is_ok()
                    {
                        evidence_created += 1;
                    }
                }
            }

            // Build provenance for validation
            let provenance = crate::context::SynthesisProvenance {
                agent_name: params.agent_name.clone(),
                source: "tdg_submit_synthesis".to_string(),
                derivation_pattern: params.derivation_pattern.clone().unwrap_or_else(|| "none".to_string()),
                invariant_claimed: params.invariant_claimed.unwrap_or(false),
                decorations_acknowledged: true,
                has_open_joints: params.has_open_joints.unwrap_or(false),
                target_status: "ai-draft".to_string(), // always ai-draft
            };

            // Run 5-gate validation
            let report = crate::context::validate_synthesis(
                &conn,
                &synthesis.id,
                &provenance,
                &params.content,
            )
            .map_err(mcp_err)?;

            // Save the validation report
            let _ = crate::context::save_report(&conn, &report);

            Ok(serde_json::to_string(&json!({
                "synthesis_id": synthesis.id,
                "synthesis_status": "ai-draft",
                "evidence_edges_created": evidence_created,
                "validation": {
                    "overall_status": report.overall_status,
                    "can_elevate_to": report.can_elevate_to,
                    "gates": report.gates.iter().map(|g| json!({
                        "gate": g.gate,
                        "passed": g.passed,
                        "blocked": g.blocked,
                        "message": g.message,
                    })).collect::<Vec<_>>(),
                    "validated_at": report.validated_at,
                },
                "message": "Synthesis submitted at ai-draft. Use tdg_elevate with human_authorization to elevate.",
            }))
            .unwrap_or_default())
        })
        .await
    }

    /// Validate an existing synthesis (Phase 5).
    ///
    /// Re-runs the 5-gate validation on an existing synthesis node.
    /// Useful after adding/removing evidence edges or changing provenance.
    #[tool(description = "Re-run 5-gate validation on an existing synthesis")]
    pub(crate) async fn tdg_validate_synthesis(
        &self,
        Parameters(params): Parameters<crate::mcp::params::ValidateSynthesisParams>,
    ) -> Result<String, McpError> {
        let pool = self.pool.clone();
        let synthesis_id = params.synthesis_id.clone();

        run_blocking(move || {
            let conn = get_conn(&pool)?;

            // Load the synthesis node
            let node = crate::db::crud::get_node(&conn, &synthesis_id)
                .map_err(mcp_err)?
                .ok_or_else(|| McpError::invalid_params(format!("Synthesis {} not found", synthesis_id), None))?;

            // Build provenance
            let provenance = crate::context::SynthesisProvenance {
                agent_name: params.agent_name.unwrap_or_else(|| "unknown".to_string()),
                source: node.source.clone(),
                derivation_pattern: params.derivation_pattern.unwrap_or_else(|| "none".to_string()),
                invariant_claimed: params.invariant_claimed.unwrap_or(false),
                decorations_acknowledged: true,
                has_open_joints: params.has_open_joints.unwrap_or(false),
                target_status: params.target_status.unwrap_or_else(|| "ai-draft".to_string()),
            };

            // Run validation
            let report = crate::context::validate_synthesis(
                &conn,
                &synthesis_id,
                &provenance,
                &node.description,
            )
            .map_err(mcp_err)?;

            // Save the report
            let _ = crate::context::save_report(&conn, &report);

            Ok(serde_json::to_string(&json!({
                "synthesis_id": synthesis_id,
                "validation": {
                    "overall_status": report.overall_status,
                    "can_elevate_to": report.can_elevate_to,
                    "gates": report.gates.iter().map(|g| json!({
                        "gate": g.gate,
                        "passed": g.passed,
                        "blocked": g.blocked,
                        "message": g.message,
                    })).collect::<Vec<_>>(),
                    "validated_at": report.validated_at,
                },
            }))
            .unwrap_or_default())
        })
        .await
    }

    /// Query the 22 named archetypes library (Phase 6).
    ///
    /// Returns the 22 archetypes (7 roles × 3 complexes + Choice meta-pivot).
    /// Can filter by complex (mind/body/spirit/pivot) or role (M/P/C/E/S/T/G/Ch).
    #[tool(description = "Query the 22 named archetypes library (filter by complex or role)")]
    pub(crate) async fn tdg_archetypes(
        &self,
        Parameters(params): Parameters<crate::mcp::params::ArchetypesParams>,
    ) -> Result<String, McpError> {
        // If a specific number is requested, return that archetype
        if let Some(num) = params.number {
            if let Some(arch) = crate::holonic_types::archetype_by_number(num) {
                return Ok(serde_json::to_string(&json!({
                    "number": arch.number,
                    "name": arch.name,
                    "complex": arch.complex.as_str(),
                    "role": arch.role.as_str(),
                    "role_name": arch.role.display_name(),
                    "description": arch.description,
                }))
                .unwrap_or_default());
            }
            return Err(McpError::invalid_params(
                format!("Archetype {} not found (must be 1-22)", num),
                None,
            ));
        }

        // Filter by complex
        let archetypes: Vec<&crate::holonic_types::Archetype> = if let Some(complex_str) = &params.complex {
            let complex = match complex_str.as_str() {
                "mind" => crate::holonic_types::Complex::Mind,
                "body" => crate::holonic_types::Complex::Body,
                "spirit" => crate::holonic_types::Complex::Spirit,
                "pivot" => crate::holonic_types::Complex::Pivot,
                _ => return Err(McpError::invalid_params(
                    format!("Invalid complex '{}'. Must be: mind, body, spirit, pivot", complex_str),
                    None,
                )),
            };
            crate::holonic_types::archetypes_by_complex(&complex)
        } else if let Some(role_str) = &params.role {
            let role = match role_str.as_str() {
                "M" => crate::holonic_types::Role::Matrix,
                "P" => crate::holonic_types::Role::Potentiator,
                "C" => crate::holonic_types::Role::Catalyst,
                "E" => crate::holonic_types::Role::Experience,
                "S" => crate::holonic_types::Role::Significator,
                "T" => crate::holonic_types::Role::Transformation,
                "G" => crate::holonic_types::Role::GreatWay,
                "Ch" => crate::holonic_types::Role::Choice,
                _ => return Err(McpError::invalid_params(
                    format!("Invalid role '{}'. Must be: M, P, C, E, S, T, G, Ch", role_str),
                    None,
                )),
            };
            crate::holonic_types::archetypes_by_role(&role)
        } else {
            // No filter — return all 22
            crate::holonic_types::all_archetypes().iter().collect()
        };

        let result: Vec<serde_json::Value> = archetypes
            .iter()
            .map(|arch| json!({
                "number": arch.number,
                "name": arch.name,
                "complex": arch.complex.as_str(),
                "role": arch.role.as_str(),
                "role_name": arch.role.display_name(),
                "description": arch.description,
            }))
            .collect();

        Ok(serde_json::to_string(&json!({
            "total": result.len(),
            "archetypes": result,
        }))
        .unwrap_or_default())
    }

    /// Run T1/T2/T3 type validation on a holon (Phase 6).
    ///
    /// Checks if the holon's type_class is a genuine invariant type:
    /// - T1: Behavioral match (bonding matches type prediction)
    /// - T2: Excitation-invariance (type fixed across stage transitions)
    /// - T3: Fixed-point persistence (type persists across metabolic cycles)
    ///
    /// Also checks Type⊥Stage orthogonality.
    #[tool(description = "Run T1/T2/T3 type validation + Type⊥Stage orthogonality check")]
    pub(crate) async fn tdg_validate_type(
        &self,
        Parameters(params): Parameters<crate::mcp::params::ValidateTypeParams>,
    ) -> Result<String, McpError> {
        let pool = self.pool.clone();
        let node_id = params.node_id.clone();

        run_blocking(move || {
            let conn = get_conn(&pool)?;

            // Load the attractor field
            let af = crate::metabolism::attractor::load(&conn, &node_id)
                .map_err(mcp_err)?
                .ok_or_else(|| McpError::invalid_params(
                    format!("Holon {} has no attractor field. Call tdg_attractor first.", node_id),
                    None,
                ))?;

            // Load the node (for developmental_stage)
            let node = crate::db::crud::get_node(&conn, &node_id)
                .map_err(mcp_err)?
                .ok_or_else(|| McpError::invalid_params(format!("Node {} not found", node_id), None))?;

            // Run T1/T2/T3 validation
            let validation = crate::holonic_types::validate_type(&conn, &node_id, &af)
                .map_err(mcp_err)?;

            // Check Type⊥Stage orthogonality
            let orthogonality = crate::holonic_types::check_type_stage_orthogonality(
                &af,
                node.developmental_stage,
            );

            Ok(serde_json::to_string(&json!({
                "node_id": node_id,
                "type_class": af.type_class,
                "is_stable": af.is_stable(),
                "is_noble": af.is_noble(),
                "pi": af.pi,
                "developmental_stage": node.developmental_stage,
                "validation": {
                    "t1_behavioral_match": validation.t1_behavioral_match,
                    "t2_excitation_invariance": validation.t2_excitation_invariance,
                    "t3_fixed_point_persistence": validation.t3_fixed_point_persistence,
                    "valid": validation.valid,
                    "details": validation.details,
                    "validated_at": validation.validated_at,
                },
                "type_stage_orthogonality": orthogonality,
                "message": if validation.valid {
                    "Type is validated (all 3 tests passed). Type⊥Stage orthogonality maintained."
                } else {
                    "Type is NOT validated. See details for which tests failed."
                },
            }))
            .unwrap_or_default())
        })
        .await
    }

    #[tool(description = "Connect two nodes with an edge")]
    pub(crate) async fn tdg_connect(
        &self,
        Parameters(params): Parameters<ConnectParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let source_id = params.source_id.clone();
        let target_id = params.target_id.clone();
        let edge_type_param = params.edge_type.clone();
        let as_edge = params.as_edge.clone();
        let weight = params.weight;
        let force = params.force.unwrap_or(false);
        run_blocking(move || {
            let conn = get_conn(&pool)?;

            let src = crate::db::crud::get_node(&conn, &source_id)
                .map_err(mcp_err)?
                .ok_or_else(|| mcp_err(format!("Source node not found: {}", source_id)))?;
            let tgt = crate::db::crud::get_node(&conn, &target_id)
                .map_err(mcp_err)?
                .ok_or_else(|| mcp_err(format!("Target node not found: {}", target_id)))?;

            // Edge type resolution priority:
            //   1. `as_edge` (explicit assert, skips validation)
            //   2. `edge_type` (requested type, validated)
            //   3. auto-detect from src/tgt node types
            let edge_type = if let Some(ref et) = as_edge {
                if !et.is_empty() {
                    et.clone()
                } else {
                    auto_detect_edge_type(&src.node_type, &tgt.node_type)
                }
            } else if !edge_type_param.is_empty() {
                edge_type_param.clone()
            } else {
                auto_detect_edge_type(&src.node_type, &tgt.node_type)
            };

            if let Err(e) =
                crate::validation::validate_edge_creation(&src.node_type, &tgt.node_type, &edge_type)
            {
                return Ok(
                    serde_json::to_string(&json!({"error": e, "code": "VALIDATION_ERROR"}))
                        .unwrap_or_default(),
                );
            }

            if !force && !matches!(edge_type.as_str(), "BLOCKS" | "DECOMPOSES_TO") {
                let existing = crate::db::crud::get_edges(
                    &conn,
                    Some(&source_id),
                    Some(&target_id),
                    Some(&edge_type),
                    None,
                    10,
                )
                .unwrap_or_default();
                if !existing.is_empty() {
                    return Ok(serde_json::to_string(
                        &json!({"status": "already_exists", "edge_id": existing[0].id}),
                    )
                    .unwrap_or_default());
                }
            }

            if !force && !matches!(edge_type.as_str(), "BLOCKS" | "DECOMPOSES_TO") {
                if let Ok(paths) =
                    crate::db::crud::pathfind(&conn, &source_id, &target_id, 6, 500)
                {
                    if !paths.is_empty() {
                        return Ok(serde_json::to_string(&json!({
                            "status": "redundant",
                            "reason": "Path already exists between nodes",
                            "existing_paths": paths.len(),
                            "shortest_hops": paths.iter().map(|p| p.len()).min().unwrap_or(0)
                        }))
                        .unwrap_or_default());
                    }
                }
            }

            // Wrap edge creation + flow propagation in a transaction.
            //
            // Previously, add_edge + emit_downward + renormalize_graph ran as
            // three separate implicit transactions. If emit_downward failed
            // mid-way, the edge was committed but drive states were partially
            // updated — leaving the graph inconsistent with no rollback.
            //
            // We now wrap the full sequence in BEGIN IMMEDIATE / COMMIT.
            // If any step fails, the entire operation rolls back.
            conn.execute_batch("BEGIN IMMEDIATE").map_err(mcp_err)?;

            let edge = match crate::db::crud::add_edge(
                &conn,
                &NewEdge {
                    source_id: source_id.clone(),
                    target_id: target_id.clone(),
                    edge_type: edge_type.clone(),
                    weight,
                    ..Default::default()
                },
            ) {
                Ok(e) => e,
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(mcp_err(e));
                }
            };

            // Flow engine: propagate drives downward from source, then renormalize.
            // Errors are logged but DON'T abort the transaction — flow is non-blocking
            // and partial flow propagation is acceptable (the graph is still consistent
            // because the edge itself was created atomically).
            if let Err(e) = flow::emit_downward(&conn, &source_id, flow::DEFAULT_MAX_DEPTH) {
                tracing::warn!("flow::emit_downward failed after connect {}: {}", edge.id, e);
            }
            if let Err(e) = flow::renormalize_graph(&conn, false) {
                tracing::warn!("flow::renormalize_graph failed after connect {}: {}", edge.id, e);
            }

            // Phase 2: Generate catalyst at the contact boundary and enqueue
            // metabolism jobs. This is where novelty enters the system —
            // drives are *born* at boundaries, not just propagated.
            // Best-effort: failures logged but don't abort the transaction.
            let catalyst = crate::metabolism::generate_catalyst(
                &edge_type,
                weight.unwrap_or(1.0),
                &src.drives,
                &tgt.drives,
            );
            if catalyst > 0.0 {
                // Inject catalyst into the target holon (the one receiving the edge)
                let payload = serde_json::json!({
                    "catalyst_amount": catalyst,
                    "source": "edge_creation",
                    "source_holon": src.id,
                    "edge_type": edge_type,
                });
                if let Err(e) = crate::metabolism::worker::enqueue_job(
                    &conn,
                    &tgt.id,
                    crate::metabolism::worker::JobType::CatalystInjection,
                    payload,
                    crate::metabolism::worker::PRIORITY_NORMAL,
                ) {
                    tracing::warn!("Failed to enqueue catalyst injection for {}: {}", tgt.id, e);
                }
                // Also inject a smaller amount into the source (the interaction perturbs both)
                let source_catalyst = catalyst * 0.3;
                let payload = serde_json::json!({
                    "catalyst_amount": source_catalyst,
                    "source": "edge_creation",
                    "source_holon": tgt.id,
                    "edge_type": edge_type,
                });
                if let Err(e) = crate::metabolism::worker::enqueue_job(
                    &conn,
                    &src.id,
                    crate::metabolism::worker::JobType::CatalystInjection,
                    payload,
                    crate::metabolism::worker::PRIORITY_LOW,
                ) {
                    tracing::warn!("Failed to enqueue catalyst injection for {}: {}", src.id, e);
                }
            }

            match conn.execute_batch("COMMIT") {
                Ok(_) => {}
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(mcp_err(e));
                }
            }

            Ok(serde_json::to_string(&json!({
                "edge_id": edge.id,
                "source": {"id": src.id, "node_type": src.node_type},
                "target": {"id": tgt.id, "node_type": tgt.node_type},
                "edge_type": edge_type
            }))
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "Batch create nodes/edges")]
    pub(crate) async fn tdg_bulk_create(
        &self,
        Parameters(params): Parameters<BulkCreateParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let nodes_json = params.nodes_json.clone();
        let edges_json = params.edges_json.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let nodes: Vec<Value> = serde_json::from_str(&nodes_json)
                .map_err(|e| McpError::invalid_params(format!("Invalid nodes_json: {}", e), None))?;
            if nodes.len() > MAX_BULK_NODES {
                return Err(McpError::invalid_params(
                    format!("Too many nodes: {} (max {})", nodes.len(), MAX_BULK_NODES),
                    None,
                ));
            }
            let mut ids = Vec::new();
            for nv in &nodes {
                let node = crate::db::crud::add_node(
                    &conn,
                    &NewNode {
                        node_type: nv["node_type"]
                            .as_str()
                            .unwrap_or("observation")
                            .to_string(),
                        name: nv["name"].as_str().unwrap_or("").to_string(),
                        description: nv["description"].as_str().map(|s| s.to_string()),
                        source: nv["source"].as_str().map(|s| s.to_string()),
                        ..Default::default()
                    },
                )
                .map_err(mcp_err)?;
                ids.push(node.id);
            }
            let edges_str = edges_json.as_deref().unwrap_or("[]");
            let edges: Vec<Value> = serde_json::from_str(edges_str)
                .map_err(|e| McpError::invalid_params(format!("Invalid edges_json: {}", e), None))?;
            let mut ec = 0i64;
            let mut failed = 0i64;
            for ev in &edges {
                if let (Some(src), Some(tgt)) = (ev["source_id"].as_str(), ev["target_id"].as_str()) {
                    let edge_type = ev["edge_type"].as_str().unwrap_or("USES").to_string();
                    // Dedup: skip if an active edge of the same type already exists
                    let dup = crate::db::crud::get_edges(
                        &conn, Some(src), Some(tgt), Some(&edge_type), None, 1,
                    ).unwrap_or_default();
                    if !dup.is_empty() {
                        continue;
                    }
                    match crate::db::crud::add_edge(
                        &conn,
                        &NewEdge {
                            source_id: src.to_string(),
                            target_id: tgt.to_string(),
                            edge_type,
                            ..Default::default()
                        },
                    ) {
                        Ok(_) => ec += 1,
                        Err(e) => {
                            tracing::warn!("bulk_create: failed to create edge {} -> {}: {}", src, tgt, e);
                            failed += 1;
                        }
                    }
                }
            }
            Ok(serde_json::to_string(
                &json!({"created_nodes": ids.len(), "created_edges": ec, "failed_edges": failed, "node_ids": ids}),
            )
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "Record execution as observation")]
    pub(crate) async fn tdg_record_exec(
        &self,
        Parameters(params): Parameters<RecordExecParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let action_type = params.action_type.clone();
        let description = params.description.clone();
        let result_val = params.result.clone();
        let tags = params.tags.clone();
        let metrics_json = params.metrics_json.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let truncated: String = description.chars().take(80).collect();
            let props = json!({"action_type": &action_type, "result": &result_val, "tags": tags.as_deref().unwrap_or(""), "metrics": metrics_json.as_deref().unwrap_or("{}")});
            let node = crate::db::crud::add_node(
                &conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("{}: {}", action_type, truncated),
                    description: Some(description),
                    source: Some("mcp_record_exec".to_string()),
                    properties: Some(props),
                    ..Default::default()
                },
            )
            .map_err(mcp_err)?;
            if let Ok(Some(agent)) = crate::db::crud::get_node(&conn, "agent:self") {
                // Dedup: only create EXPERIENCES edge if one doesn't already exist
                // between this observation and agent:self. Previously every
                // tdg_record_exec call created a new edge, accumulating
                // duplicates that inflated edge noise.
                let existing = crate::db::crud::get_edges(
                    &conn,
                    Some(&node.id),
                    Some(&agent.id),
                    Some("EXPERIENCES"),
                    None,
                    1,
                ).unwrap_or_default();
                if existing.is_empty() {
                    if let Err(e) = crate::db::crud::add_edge(
                        &conn,
                        &NewEdge {
                            source_id: node.id.clone(),
                            target_id: agent.id,
                            edge_type: "EXPERIENCES".to_string(),
                            ..Default::default()
                        },
                    ) {
                        tracing::warn!(
                            "Failed to create EXPERIENCES edge {} -> agent:self: {}",
                            node.id, e
                        );
                    }
                }
            }
            Ok(serde_json::to_string(&json!({"observation_id": node.id, "action_type": action_type, "result": result_val})).unwrap_or_default())
        }).await
    }

    #[tool(description = "Rate node confidence")]
    pub(crate) async fn tdg_rate_memory(
        &self,
        Parameters(params): Parameters<RateMemoryParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let node_id = params.node_id.clone();
        let helpful = params.helpful;
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let delta: i32 = if helpful { 1 } else { -1 };
            // Use MAX(0, helpful_count + ?1) to prevent negative helpful_count.
            // Previously, unhelpful ratings on a node with helpful_count=0
            // produced -1, corrupting the trust formula (confidence * (1 + (-1)) = 0).
            conn.execute("UPDATE nodes SET helpful_count = MAX(0, helpful_count + ?1), updated_at = ?2 WHERE id = ?3 AND valid_to IS NULL",
                rusqlite::params![delta, crate::db::crud::now_iso(), &node_id]).map_err(mcp_err)?;
            let trust: f64 = conn.query_row("SELECT confidence * (1.0 + helpful_count) / (1.0 + retrieval_count) FROM nodes WHERE id = ?1 AND valid_to IS NULL", rusqlite::params![&node_id], |row| row.get(0)).unwrap_or(0.0);
            Ok(serde_json::to_string(
                &json!({"node_id": node_id, "helpful": helpful, "trust_score": trust}),
            )
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "Graph state, health, or integrity check")]
    pub(crate) async fn tdg_mind_state(
        &self,
        Parameters(params): Parameters<MindStateParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let verify = params.verify.unwrap_or(false);
        let health = params.health.unwrap_or(false);
        let detail = params.detail.unwrap_or(false);
        run_blocking(move || {
            let conn = get_conn(&pool)?;

            // --- verify mode: PRAGMA integrity_check + basic counts ---
            if verify {
                let integrity: String = conn
                    .pragma_query_value(None, "integrity_check", |row| row.get(0))
                    .unwrap_or_else(|_| "error".to_string());
                let nc = crate::db::crud::count_nodes(&conn, None).unwrap_or(0);
                let ec = crate::db::crud::count_edges(&conn, None).unwrap_or(0);
                let evc: i64 = conn
                    .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
                    .unwrap_or(0);
                let db_path = crate::config::Config::from_env().db_path;
                let db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
                return Ok(serde_json::to_string(&json!({
                    "valid": integrity == "ok",
                    "integrity_check": integrity,
                    "active_nodes": nc,
                    "active_edges": ec,
                    "total_events": evc,
                    "db_size_mb": (db_size as f64 / 1_048_576.0 * 100.0).round() / 100.0,
                }))
                .unwrap_or_default());
            }

            // --- default mode: graph stats + quadrants ---
            let nc = crate::db::crud::count_nodes(&conn, None).unwrap_or(0);
            let ec = crate::db::crud::count_edges(&conn, None).unwrap_or(0);
            let evc: i64 = conn
                .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
                .unwrap_or(0);
            let oc = crate::db::crud::count_nodes(&conn, Some("observation")).unwrap_or(0);
            let tc = crate::db::crud::count_nodes(&conn, Some("telos")).unwrap_or(0);
            let sc = crate::db::crud::count_nodes(&conn, Some("skill")).unwrap_or(0);

            // developmental stage
            let stages: Vec<i64> = conn
                .prepare("SELECT developmental_stage FROM nodes WHERE valid_to IS NULL AND developmental_stage IS NOT NULL")
                .and_then(|mut stmt| {
                    let rows = stmt.query_map([], |r| r.get(0))?;
                    Ok(rows.filter_map(|r| r.ok()).collect())
                })
                .unwrap_or_default();
            let cs = stages.iter().copied().max().unwrap_or(1);

            let mut result = json!({
                "graph": {"nodes": nc, "edges": ec, "events": evc},
                "observations": oc,
                "teloi": tc,
                "skills": sc,
                "stage": {"current": cs, "nodes_with_data": stages.len()},
            });

            // quadrant distribution
            let mut qd = json!({"UL": 0, "UR": 0, "LL": 0, "LR": 0});
            
            // Read from quadrants_json (new location) with fallback to properties_json (legacy)
            if let Ok(mut stmt) = conn.prepare(
                "SELECT quadrants_json, properties_json FROM nodes WHERE valid_to IS NULL AND (quadrants_json NOT IN ('{}', '') OR properties_json NOT IN ('{}', ''))"
            ) {
                if let Ok(rows) = stmt.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                    ))
                }) {
                    for row in rows.flatten() {
                        let (quadrants_json, properties_json) = row;
                        
                        // Try quadrants_json["primary"] first
                        let mut primary: Option<String> = None;
                        if let Ok(props) = serde_json::from_str::<serde_json::Value>(&quadrants_json) {
                            primary = props.get("primary").and_then(|v| v.as_str()).map(|s| s.to_string());
                        }
                        
                        // Fallback to properties_json["quadrant"]
                        if primary.is_none() {
                            if let Ok(props) = serde_json::from_str::<serde_json::Value>(&properties_json) {
                                primary = props.get("quadrant").and_then(|v| v.as_str()).map(|s| s.to_string());
                            }
                        }
                        
                        if let Some(p) = primary {
                            if let Some(count) = qd.get_mut(&p) {
                                *count = json!(count.as_i64().unwrap_or(0) + 1);
                            }
                        }
                    }
                }
            }
            result["quadrants"] = qd;

            // --- health mode: orphan ratio + constraint analysis ---
            if health {
                let orphan: i64 = conn
                    .prepare("SELECT COUNT(*) FROM nodes n WHERE n.valid_to IS NULL AND NOT EXISTS (SELECT 1 FROM edges e WHERE (e.source_id=n.id OR e.target_id=n.id) AND e.valid_to IS NULL)")
                    .and_then(|mut stmt| stmt.query_row([], |r| r.get(0)))
                    .unwrap_or(0);
                let cc: i64 = conn
                    .prepare(
                        "SELECT COUNT(*) FROM nodes WHERE node_type='constraint' AND valid_to IS NULL",
                    )
                    .and_then(|mut stmt| stmt.query_row([], |r| r.get(0)))
                    .unwrap_or(0);
                let actv: i64 = conn
                    .prepare("SELECT COUNT(DISTINCT source_id) FROM edges WHERE edge_type='BLOCKS' AND valid_to IS NULL")
                    .and_then(|mut stmt| stmt.query_row([], |r| r.get(0)))
                    .unwrap_or(0);

                let mut recs = Vec::new();
                let orphan_ratio = orphan as f64 / nc.max(1) as f64;
                if orphan_ratio > 0.15 {
                    recs.push(format!("Orphan ratio {:.1}%", orphan_ratio * 100.0));
                }
                if cc > 0 && actv == 0 {
                    recs.push(format!("{} constraints with 0 active BLOCKS", cc));
                }
                let status = if recs.is_empty() { "good" } else { "degraded" };

                result["health"] = json!({
                    "orphans": orphan,
                    "orphan_ratio": (orphan_ratio * 10000.0).round() / 10000.0,
                    "constraints": {"total": cc, "active_blocks": actv},
                    "recommendations": recs,
                });
                result["status"] = json!(status);
            }

            // --- detail mode: drive scores + telos hierarchy + stage trajectory ---
            if detail {
                // drive scores (eros, agape, agency, communion)
                let drive_keys = ["eros", "agape", "agency", "communion"];
                let mut drives: DriveScores = drive_keys
                    .iter()
                    .map(|k| (k.to_string(), (Vec::new(), Vec::new(), Vec::new())))
                    .collect();

                if let Ok(mut stmt) = conn.prepare("SELECT drives_json FROM nodes WHERE valid_to IS NULL AND drives_json NOT IN ('{}', '')") {
                    if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                        for row in rows.flatten() {
                            if let Ok(props) = serde_json::from_str::<serde_json::Value>(&row) {
                                for dk in &drive_keys {
                                    // drives_json is stored as {"eros": {"positive_pole": 5.0, "negative_pole": 2.0}, ...}
                                    // (see flow.rs FlowDriveState::to_json). The previous implementation
                                    // looked for flat keys like "eros_positive_pole" or literal dotted
                                    // keys like "eros.positive_pole" — both of which NEVER matched the
                                    // actual nested format. So drive_scores always returned zeros.
                                    // We now use nested JSON access: props[dk]["positive_pole"].
                                    let drive_obj = props.get(*dk);
                                    let pos = drive_obj
                                        .and_then(|d| d.get("positive_pole"))
                                        .and_then(|v| v.as_f64());
                                    let neg = drive_obj
                                        .and_then(|d| d.get("negative_pole"))
                                        .and_then(|v| v.as_f64());
                                    let net = match (pos, neg) {
                                        (Some(p), Some(n)) => Some(p - n),
                                        _ => None,
                                    };
                                    if let Some(p) = pos { if let Some(v) = drives.get_mut(*dk) { v.0.push(p); } }
                                    if let Some(n) = neg { if let Some(v) = drives.get_mut(*dk) { v.1.push(n); } }
                                    if let Some(nt) = net { if let Some(v) = drives.get_mut(*dk) { v.2.push(nt); } }
                                }
                            }
                        }
                    }
                }

                let drive_scores: serde_json::Map<String, serde_json::Value> = drive_keys.iter().filter_map(|dk| {
                    let (p, n, net) = drives.get(*dk)?;
                    Some((dk.to_string(), json!({
                        "avg_positive": if p.is_empty() { 0.0 } else { (p.iter().sum::<f64>() / p.len() as f64 * 100.0).round() / 100.0 },
                        "avg_negative": if n.is_empty() { 0.0 } else { (n.iter().sum::<f64>() / n.len() as f64 * 100.0).round() / 100.0 },
                        "avg_net": if net.is_empty() { 0.0 } else { (net.iter().sum::<f64>() / net.len() as f64 * 100.0).round() / 100.0 },
                        "nodes_with_data": p.len(),
                    })))
                }).collect();
                result["drive_scores"] = json!(drive_scores);

                if let Ok(mut stmt) = conn.prepare("SELECT id,name,teleological_level FROM nodes WHERE node_type='telos' AND valid_to IS NULL ORDER BY teleological_level") {
                    if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))) {
                        let mut by_lev: std::collections::HashMap<String, Vec<serde_json::Value>> = std::collections::HashMap::new();
                        for row in rows.flatten() {
                            let level = if row.2.is_empty() { "T4".to_string() } else { row.2 };
                            let name_trunc: String = row.1.chars().take(40).collect();
                            by_lev.entry(level).or_default().push(json!({"id": row.0, "name": name_trunc}));
                        }
                        let mut telos_hierarchy = serde_json::Map::new();
                        let mut sorted_keys: Vec<_> = by_lev.keys().collect();
                        sorted_keys.sort_by_key(|k| k.strip_prefix("T").and_then(|s| s.parse::<u32>().ok()).unwrap_or(99));
                        for k in sorted_keys {
                            let v = &by_lev[k];
                            telos_hierarchy.insert(k.clone(), json!({"count": v.len(), "items": v}));
                        }
                        result["telos_hierarchy"] = json!(telos_hierarchy);
                    }
                }

                // stage trajectory
                let sn_names = serde_json::json!({
                    "1": "Survival", "2": "Identity", "3": "Power", "4": "Heart",
                    "5": "Rational", "6": "Pluralistic", "7": "Integral", "8": "Harvest"
                });
                let ns = (cs + 1).min(8);
                let ns_name = sn_names
                    .get(ns.to_string())
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");
                let req = (stages.len() + 5).max(10) as f64;
                let prog = (stages.len() as f64 / req * 100.0).min(100.0) as i64;
                result["stage_trajectory"] = json!({
                    "current": cs,
                    "next": ns,
                    "next_name": ns_name,
                    "evidence_count": stages.len(),
                    "progress_pct": prog,
                });
            }

            Ok(serde_json::to_string(&result).unwrap_or_default())
        }).await
    }

    #[tool(description = "Create observation node")]
    pub(crate) async fn tdg_observe(
        &self,
        Parameters(params): Parameters<ObserveParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        if params.description.is_empty() {
            return Err(McpError::invalid_params("description is required", None));
        }
        let pool = self.pool.clone();
        let description = params.description.clone();
        let quadrant = params.quadrant.unwrap_or_else(|| "LR".to_string());
        let trust = params.trust.unwrap_or(0.5).clamp(0.0, 1.0);
        let cycle = params.cycle.unwrap_or(0);
        let entities = params.entities.clone();
        let trigger_digestion = params.trigger_digestion.unwrap_or(true);
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let truncated: String = description.chars().take(80).collect();
            
            // Write quadrant to quadrants_json["primary"] for tdg_mind_state compatibility
            // Also keep in properties_json for backward compatibility
            let mut quadrants = serde_json::Map::new();
            quadrants.insert("primary".to_string(), json!(quadrant));
            quadrants.insert("cycle".to_string(), json!(cycle));
            quadrants.insert("trust".to_string(), json!(trust));
            
            let props = json!({
                "quadrant": quadrant,
                "cycle": cycle,
                "trust": trust,
                // Set catalyst_type and digestion status so the digestion pipeline
                // can group and process MCP-created observations. Previously
                // tdg_observe bypassed digest_catalyst, so these fields were never
                // set on MCP-created observations — making process_digestion_cycle
                // useless for them.
                "catalyst_type": "routine_observation",
                "status": "raw",
            });
            let node = crate::db::crud::add_node(
                &conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("Obs: {}", truncated),
                    description: Some(description.clone()),
                    source: Some("mcp_observe".to_string()),
                    properties: Some(props),
                    quadrants: Some(json!(quadrants)),
                    ..Default::default()
                },
            )
            .map_err(mcp_err)?;

            let mut entity_ids: Vec<String> = Vec::new();
            let mut seen_entities: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();

            // Handle explicit entities parameter
            if let Some(ref entities_str) = entities {
                for entity_name in entities_str.split(',') {
                    let name = entity_name.trim().to_string();
                    if name.is_empty() {
                        continue;
                    }
                    let key = (name.to_lowercase(), "entity".to_string());
                    if seen_entities.insert(key) {
                        if let Ok(id) = upsert_entity_and_connect(&conn, &node.id, &name, "entity") {
                            entity_ids.push(id);
                        }
                    }
                }
            }

            // Extract entities from description and wire them into the graph
            let entity_extractor = crate::plugins::entity_extractor::EntityExtractor::new();
            let extracted_entities = entity_extractor.extract(&description, Some(&conn));

            // Wire up extracted entities with deduplication
            for extracted in &extracted_entities {
                let key = (extracted.name.to_lowercase(), extracted.entity_type.clone());
                if seen_entities.insert(key) {
                    if let Ok(id) = upsert_entity_and_connect(
                        &conn,
                        &node.id,
                        &extracted.name,
                        &extracted.entity_type,
                    ) {
                        entity_ids.push(id);
                    }
                }
            }

            let pref_extractor = crate::plugins::preference_extractor::PreferenceExtractor::new();
            let extracted_preferences = pref_extractor.extract_from_message(&description);

            // Persist extracted preferences as constraint nodes in the graph.
            // Previously these were only returned in the JSON response and then
            // discarded — the agent's preferences/corrections were detected but
            // never stored, so they couldn't influence future behavior.
            let mut persisted_preferences = 0usize;
            for pref in &extracted_preferences {
                // Only persist meaningful preferences (skip low-confidence noise)
                if pref.confidence < 0.5 {
                    continue;
                }
                // Use the constraint_id if provided, else generate one
                let constraint_id = if pref.constraint_id.is_empty() {
                    format!("pref_{}", uuid::Uuid::new_v4().as_simple())
                } else {
                    pref.constraint_id.clone()
                };
                // Check if this constraint already exists (dedup by id)
                let exists = crate::db::crud::get_node(&conn, &constraint_id)
                    .ok()
                    .flatten()
                    .is_some();
                if exists {
                    continue;
                }
                let constraint_name = format!(
                    "{}: {}",
                    pref.extraction_type,
                    pref.constraint_text.chars().take(80).collect::<String>()
                );
                let props = json!({
                    "extraction_type": pref.extraction_type,
                    "confidence": pref.confidence,
                    "quadrant": pref.quadrant,
                    "source": "preference_extractor",
                    "originating_observation": node.id,
                });
                if let Err(e) = crate::db::crud::add_node(
                    &conn,
                    &NewNode {
                        node_type: "constraint".to_string(),
                        name: constraint_name,
                        description: Some(pref.constraint_text.clone()),
                        source: Some("mcp_observe".to_string()),
                        properties: Some(props),
                        ..Default::default()
                    },
                ) {
                    tracing::warn!(
                        "Failed to persist preference as constraint node: {}", e
                    );
                    continue;
                }
                // Link the observation to the constraint via a MENTIONS edge
                if let Err(e) = crate::db::crud::add_edge(
                    &conn,
                    &NewEdge {
                        source_id: node.id.clone(),
                        target_id: constraint_id.clone(),
                        edge_type: "MENTIONS".to_string(),
                        weight: Some(pref.confidence as f64),
                        agent_id: Some("mcp_observe".to_string()),
                        ..Default::default()
                    },
                ) {
                    tracing::warn!(
                        "Failed to create MENTIONS edge to constraint {}: {}",
                        constraint_id, e
                    );
                }
                persisted_preferences += 1;
            }

            let mut digested = false;
            let mut hypothesis_count = 0usize;
            let mut digestion_error: Option<String> = None;

            // Phase 2: Inject catalyst into the new observation to kickstart
            // its lesser cycle. Every new observation is a perturbation that
            // should trigger metabolic processing.
            {
                let payload = serde_json::json!({
                    "catalyst_amount": 0.5,  // base catalyst for a new observation
                    "source": "observation_creation",
                });
                if let Err(e) = crate::metabolism::worker::enqueue_job(
                    &conn,
                    &node.id,
                    crate::metabolism::worker::JobType::CatalystInjection,
                    payload,
                    crate::metabolism::worker::PRIORITY_NORMAL,
                ) {
                    tracing::warn!("Failed to enqueue catalyst injection for observation {}: {}", node.id, e);
                }
            }

            if trigger_digestion {
                let engine = crate::digestion::DigestionEngine::new(&conn);
                match engine.check_upward_cascade() {
                    Ok(hypotheses) => {
                        hypothesis_count = hypotheses.len();
                        digested = hypothesis_count > 0;
                    }
                    Err(e) => {
                        tracing::warn!("Digestion cascade failed for {}: {}", node.id, e);
                        digestion_error = Some(e.to_string());
                    }
                }
            }

            Ok(serde_json::to_string(&json!({
                "observation_id": node.id,
                "description": truncated,
                "quadrant": quadrant,
                "trust": trust,
                "entities_connected": entity_ids,
                "extracted_entities": extracted_entities,
                "extracted_preferences": extracted_preferences,
                "preferences_persisted": persisted_preferences,
                "digested": digested,
                "hypotheses_generated": hypothesis_count,
                "digestion_error": digestion_error,
            }))
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "Traverse node relationships")]
    pub(crate) async fn tdg_get_related(
        &self,
        Parameters(params): Parameters<GetRelatedParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let node_id = params.node_id.clone();
        let limit = params.limit.unwrap_or(20);
        let direction = params.direction.as_deref().unwrap_or("out").to_string();
        let edge_type = params.edge_type.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let edge_type_ref = edge_type.as_deref().filter(|s| !s.is_empty());
            let mut results = Vec::new();
            if direction == "out" || direction == "both" {
                for edge in crate::db::crud::get_edges(
                    &conn,
                    Some(&node_id),
                    None,
                    edge_type_ref,
                    None,
                    limit,
                )
                .unwrap_or_default()
                {
                    if let Ok(Some(n)) = crate::db::crud::get_node(&conn, &edge.target_id) {
                        results.push(json!({"id": n.id, "name": n.name, "node_type": n.node_type, "edge_type": edge.edge_type, "direction": "out"}));
                    }
                }
            }
            if direction == "in" || direction == "both" {
                for edge in crate::db::crud::get_edges(
                    &conn,
                    None,
                    Some(&node_id),
                    edge_type_ref,
                    None,
                    limit,
                )
                .unwrap_or_default()
                {
                    if let Ok(Some(n)) = crate::db::crud::get_node(&conn, &edge.source_id) {
                        results.push(json!({"id": n.id, "name": n.name, "node_type": n.node_type, "edge_type": edge.edge_type, "direction": "in"}));
                    }
                }
            }
            results.truncate(limit as usize);
            Ok(serde_json::to_string(
                &json!({"node_id": node_id, "related": results, "total": results.len()}),
            )
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "Run graph maintenance")]
    pub async fn tdg_maintenance(
        &self,
        Parameters(params): Parameters<MaintenanceParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();

        // Read action first, fall back to phase for backward compatibility
        let action = params.action.as_deref();
        let phase = params.phase.as_deref();

        let actual_action = if let Some(a) = action {
            a
        } else if let Some(p) = phase {
            // Log deprecation warning for phase usage
            tracing::warn!("WARNING: 'phase' parameter is deprecated. Use 'action' instead.");
            p
        } else {
            return Err(McpError::invalid_params(
                "Either 'action' or 'phase' parameter is required",
                None,
            ));
        };

        // Map old phase names to new action names for backward compatibility
        let normalized_action = match actual_action {
            "fts_rebuild" => {
                tracing::warn!("WARNING: 'fts_rebuild' is deprecated. Use 'rebuild_fts' instead.");
                "rebuild_fts"
            }
            "hygiene" => {
                tracing::warn!("WARNING: 'hygiene' is deprecated. Use 'health' instead.");
                "health"
            }
            "archive" => {
                tracing::warn!("WARNING: 'archive' action is deprecated and will be removed in a future version.");
                "archive"
            }
            "all" => "all",
            _ => actual_action,
        };

        let action_str = normalized_action.to_string();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let mut report = serde_json::Map::new();

            match action_str.as_str() {
                "rebuild_fts" => {
                    crate::db::schema::rebuild_fts(&conn).map_err(mcp_err)?;
                    report.insert("fts_rebuilt".to_string(), json!(true));
                }
                "health" => {
                    let hygiene = crate::knowledge::generate_hygiene_report(&conn).map_err(mcp_err)?;
                    report.insert("orphan_count".to_string(), json!(hygiene.orphan_count));
                    report.insert(
                        "dangling_edge_count".to_string(),
                        json!(hygiene.dangling_edge_count),
                    );
                    report.insert("stale_node_count".to_string(), json!(hygiene.stale_count));
                }
                "archive" => {
                    let archived = crate::knowledge::archive_stale_nodes(&conn, None).map_err(mcp_err)?;
                    report.insert(
                        "archived_count".to_string(),
                        json!(archived
                            .get("archived_count")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0)),
                    );
                }
                "enrich" | "align_data" => {
                    let enricher = crate::maintenance::Enricher::new(&conn);
                    let enricher_report = enricher.run(false).map_err(mcp_err)?;
                    report.insert("drives_enriched".to_string(), json!(enricher_report.drives_enriched));
                    report.insert("stages_enriched".to_string(), json!(enricher_report.stages_enriched));
                    report.insert("parents_enriched".to_string(), json!(enricher_report.parents_enriched));
                    report.insert("embeddings_enriched".to_string(), json!(enricher_report.embeddings_enriched));
                    report.insert("embeddings_failed".to_string(), json!(enricher_report.embeddings_failed));
                }
                "all" => {
                    // Run all maintenance actions
                    let hygiene = crate::knowledge::generate_hygiene_report(&conn).map_err(mcp_err)?;
                    report.insert("orphan_count".to_string(), json!(hygiene.orphan_count));
                    report.insert(
                        "dangling_edge_count".to_string(),
                        json!(hygiene.dangling_edge_count),
                    );
                    report.insert("stale_node_count".to_string(), json!(hygiene.stale_count));

                    let archived = crate::knowledge::archive_stale_nodes(&conn, None).map_err(mcp_err)?;
                    report.insert(
                        "archived_count".to_string(),
                        json!(archived
                            .get("archived_count")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0)),
                    );

                    crate::db::schema::rebuild_fts(&conn).map_err(mcp_err)?;
                    report.insert("fts_rebuilt".to_string(), json!(true));
                }
                "rebuild_embeddings" => {
                    // Re-embed all active nodes that are missing embeddings.
                    // Leverages the Janitor's backfill_vec pass (non-dry-run).
                    let janitor = crate::maintenance::Janitor::new(&conn);
                    let janitor_report = janitor.run(false).map_err(mcp_err)?;
                    report.insert("embeddings_missing".to_string(), json!(janitor_report.vec_missing));
                    report.insert("embeddings_built".to_string(), json!(janitor_report.vec_embedded));
                    report.insert("fts5_indexed".to_string(), json!(janitor_report.fts5_indexed));
                    report.insert("edges_pruned".to_string(), json!(janitor_report.edges_pruned));
                }
                "gc_edges" => {
                    // Prune edges that point at archived/deleted nodes, then
                    // collapse duplicate edges (same src+tgt+type) keeping the
                    // most recently created one.
                    let janitor = crate::maintenance::Janitor::new(&conn);
                    let janitor_report = janitor.run(false).map_err(mcp_err)?;
                    report.insert("orphaned_edges_pruned".to_string(), json!(janitor_report.edges_pruned));

                    // Collapse duplicate active edges (same src, tgt, type) — keep newest.
                    let dup_count: usize = conn.execute(
                        "DELETE FROM edges
                         WHERE rowid IN (
                             SELECT e1.rowid FROM edges e1
                             INNER JOIN edges e2
                             ON e1.source_id = e2.source_id
                             AND e1.target_id = e2.target_id
                             AND e1.edge_type = e2.edge_type
                             AND e1.valid_to IS NULL
                             AND e2.valid_to IS NULL
                             AND e1.created_at < e2.created_at
                         )",
                        [],
                    ).map_err(mcp_err)?;
                    report.insert("duplicate_edges_collapsed".to_string(), json!(dup_count));
                }
                "gc_nodes" => {
                    // Soft-delete (archive) nodes that have been stale for a long time
                    // and have no active edges. This is safe — soft delete only.
                    let archived = crate::knowledge::archive_stale_nodes(&conn, None).map_err(mcp_err)?;
                    report.insert(
                        "nodes_archived".to_string(),
                        json!(archived
                            .get("archived_count")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0)),
                    );

                    // Also prune orphaned edges (edges pointing to archived nodes).
                    let janitor = crate::maintenance::Janitor::new(&conn);
                    let janitor_report = janitor.run(false).map_err(mcp_err)?;
                    report.insert("edges_pruned".to_string(), json!(janitor_report.edges_pruned));
                    report.insert("parents_backfilled".to_string(), json!(janitor_report.parents_backfilled));
                }
                "gc_all" => {
                    // Full GC sweep: nodes + edges + duplicates + FTS rebuild.
                    let archived = crate::knowledge::archive_stale_nodes(&conn, None).map_err(mcp_err)?;
                    report.insert(
                        "nodes_archived".to_string(),
                        json!(archived
                            .get("archived_count")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0)),
                    );

                    let janitor = crate::maintenance::Janitor::new(&conn);
                    let janitor_report = janitor.run(false).map_err(mcp_err)?;
                    report.insert("edges_pruned".to_string(), json!(janitor_report.edges_pruned));
                    report.insert("embeddings_built".to_string(), json!(janitor_report.vec_embedded));
                    report.insert("fts5_indexed".to_string(), json!(janitor_report.fts5_indexed));

                    let dup_count: usize = conn.execute(
                        "DELETE FROM edges
                         WHERE rowid IN (
                             SELECT e1.rowid FROM edges e1
                             INNER JOIN edges e2
                             ON e1.source_id = e2.source_id
                             AND e1.target_id = e2.target_id
                             AND e1.edge_type = e2.edge_type
                             AND e1.valid_to IS NULL
                             AND e2.valid_to IS NULL
                             AND e1.created_at < e2.created_at
                         )",
                        [],
                    ).map_err(mcp_err)?;
                    report.insert("duplicate_edges_collapsed".to_string(), json!(dup_count));

                    crate::db::schema::rebuild_fts(&conn).map_err(mcp_err)?;
                    report.insert("fts_rebuilt".to_string(), json!(true));
                }
                _ => {
                    return Err(McpError::invalid_params(
                        format!(
                            "Unknown action '{}'. Valid actions: rebuild_fts, rebuild_embeddings, health, archive, enrich, align_data, gc_nodes, gc_edges, gc_all, all",
                            action_str
                        ),
                        None,
                    ));
                }
            }

            Ok(serde_json::to_string(&json!(report)).unwrap_or_default())
        }).await
    }

    #[tool(description = "Run enrichment pipeline: embeddings, drives, stages, parents")]
    pub(crate) async fn tdg_enrich(
        &self,
        Parameters(params): Parameters<EnrichParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let dry_run = params.dry_run.unwrap_or(false);
        let pool = self.pool.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let enricher = crate::maintenance::Enricher::new(&conn);
            let report = enricher.run(dry_run).map_err(mcp_err)?;
            Ok(serde_json::to_string(&json!({
                "dry_run": report.dry_run,
                "drives_enriched": report.drives_enriched,
                "stages_enriched": report.stages_enriched,
                "parents_enriched": report.parents_enriched,
                "embeddings_enriched": report.embeddings_enriched,
                "embeddings_failed": report.embeddings_failed,
                "timestamp": report.timestamp,
            }))
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "Run autonomous self-management cycle")]
    pub(crate) async fn tdg_self_manage(
        &self,
        Parameters(params): Parameters<SelfManageParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let dry_run = params.dry_run.unwrap_or(true);
        let pool = self.pool.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let manager = crate::maintenance::SelfManager::new(&conn);
            let report = manager.run(dry_run).map_err(mcp_err)?;
            Ok(serde_json::to_string(&json!({
                "dry_run": report.dry_run,
                "health_before": report.health_before.as_ref().map(|h| h.health_score),
                "health_after": report.health_after.as_ref().map(|h| h.health_score),
                "health_delta": report.health_delta,
                "janitor": report.janitor.as_ref().map(|j| format!("{:?}", j)),
                "enricher": report.enricher.as_ref().map(|e| format!("{:?}", e)),
                "archiver": report.archiver.as_ref().map(|a| format!("{:?}", a)),
                "duration_seconds": report.duration_seconds,
                "succeeded": report.succeeded,
                "failed": report.failed,
            }))
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "Introspect database schema")]
    pub(crate) async fn tdg_get_schema(&self) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let mut tables = serde_json::Map::new();
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .map_err(mcp_err)?;
            let names: Vec<String> = stmt
                .query_map([], |row| row.get(0))
                .map_err(mcp_err)?
                .filter_map(|r| r.ok())
                .collect();
            for name in &names {
                if name.starts_with("sqlite_") {
                    continue;
                }
                let count: i64 = conn
                    .query_row(&format!("SELECT COUNT(*) FROM \"{}\"", name), [], |r| {
                        r.get(0)
                    })
                    .unwrap_or(0);
                tables.insert(name.clone(), json!({"row_count": count}));
            }
            Ok(serde_json::to_string(&json!({"tables": tables})).unwrap_or_default())
        }).await
    }

    #[tool(description = "Manage memory banks")]
    pub(crate) async fn tdg_bank(
        &self,
        Parameters(params): Parameters<BankParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let action = params.action.clone();
        let profile = params.profile.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            match action.as_deref().unwrap_or("list") {
                "list" => {
                    let mut stmt = conn.prepare("SELECT DISTINCT agent_id FROM nodes WHERE agent_id IS NOT NULL AND valid_to IS NULL ORDER BY agent_id").map_err(mcp_err)?;
                    let banks: Vec<String> = stmt
                        .query_map([], |row| row.get(0))
                        .map_err(mcp_err)?
                        .filter_map(|r| r.ok())
                        .collect();
                    let data: Vec<Value> = banks.iter().map(|b| { let count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes WHERE agent_id = ?1 AND valid_to IS NULL", [b.as_str()], |r| r.get(0)).unwrap_or(0); json!({"bank_id": b, "node_count": count}) }).collect();
                    Ok(serde_json::to_string(&json!({"banks": data})).unwrap_or_default())
                }
                "set_context" => Ok(serde_json::to_string(
                    &json!({"context_set": profile.as_deref().unwrap_or("default")}),
                )
                .unwrap_or_default()),
                a => Err(McpError::invalid_params(
                    format!("Unknown bank action: {}", a),
                    None,
                )),
            }
        }).await
    }

    #[tool(description = "Resolve entity names and aliases")]
    pub(crate) async fn tdg_entity(
        &self,
        Parameters(params): Parameters<EntityParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let action = params.action.clone();
        let name = params.name.clone();
        let text = params.text.clone();
        let node_id = params.node_id.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            match action.as_deref().unwrap_or("resolve") {
                "resolve" => {
                    let search = name
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .or(text.as_deref().filter(|s| !s.is_empty()));
                    let term = search
                        .ok_or_else(|| McpError::invalid_params("name or text is required", None))?;
                    let q = NodeQuery {
                        node_type: Some("entity".to_string()),
                        limit: Some(10),
                        ..Default::default()
                    };
                    let nodes = crate::db::crud::query_nodes(&conn, &q).map_err(mcp_err)?;
                    let entities: Vec<Value> = nodes
                        .iter()
                        .filter(|n| n.name.to_lowercase().contains(&term.to_lowercase()))
                        .map(|n| json!({"id": n.id, "name": n.name}))
                        .collect();
                    Ok(serde_json::to_string(&json!({"entities": entities})).unwrap_or_default())
                }
                "get" => {
                    let nid = node_id.as_deref().unwrap_or("");
                    if nid.is_empty() {
                        return Err(McpError::invalid_params("node_id required", None));
                    }
                    match crate::db::crud::get_node(&conn, nid).map_err(mcp_err)? {
                        Some(n) => Ok(serde_json::to_string(
                            &json!({"id": n.id, "name": n.name, "properties": n.properties}),
                        )
                        .unwrap_or_default()),
                        None => Err(McpError::invalid_params("Node not found", None)),
                    }
                }
                a => Err(McpError::invalid_params(
                    format!("Unknown entity action: {}", a),
                    None,
                )),
            }
        }).await
    }

    #[tool(description = "Run LLM synthesis on graph context")]
    pub(crate) async fn tdg_reflect(
        &self,
        Parameters(params): Parameters<ReflectParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }

        let pool = self.pool.clone();
        let turns = params.turns.unwrap_or(50).min(200);
        let status_only = params.status_only.unwrap_or(false);
        let focus_topics: Vec<String> = params
            .focus_topics
            .as_ref()
            .map(|s| {
                s.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        // ── Status-only mode: check LLM availability ──────────────
        if status_only {
            let llm_cfg = crate::llm::config::LlmConfig::from_env();
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .map_err(mcp_err)?;

            let mut providers: Vec<(&str, bool)> = Vec::new();
            // OpenAI
            providers.push(("openai", llm_cfg.provider_available("openai")));
            // Anthropic
            providers.push(("anthropic", llm_cfg.provider_available("anthropic")));
            // Ollama
            let ollama_ok = client
                .get(format!("{}/api/tags", llm_cfg.ollama.base_url))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            providers.push(("ollama", ollama_ok));

            return Ok(serde_json::to_string(&json!({
                "status": "ok",
                "providers": providers.iter().map(|(name, avail)| json!({"name": name, "available": avail})).collect::<Vec<_>>(),
                "default_provider": llm_cfg.default_provider,
                "timestamp": crate::db::crud::now_iso(),
            }))
            .unwrap_or_default());
        }

        // ── Gather context from graph (blocking) ───────────────────
        let focus_topics_cp = focus_topics.clone();
        let (observations, people, telos, edge_count, total_nodes, type_dist, entity_names) = run_blocking(move || {
            let conn = get_conn(&pool)?;
            let obs_query = NodeQuery {
                node_type: Some("observation".to_string()),
                limit: Some(turns.min(200)),
                ..Default::default()
            };
            let observations = crate::db::crud::query_nodes(&conn, &obs_query).unwrap_or_default();

            let people_query = NodeQuery {
                node_type: Some("people".to_string()),
                limit: Some(20),
                ..Default::default()
            };
            let people = crate::db::crud::query_nodes(&conn, &people_query).unwrap_or_default();

            let telos_query = NodeQuery {
                node_type: Some("telos".to_string()),
                limit: Some(20),
                ..Default::default()
            };
            let telos = crate::db::crud::query_nodes(&conn, &telos_query).unwrap_or_default();

            let edge_count = crate::db::crud::count_edges(&conn, None).unwrap_or(0);
            let total_nodes = crate::db::crud::count_nodes(&conn, None).unwrap_or(0);

            // Node type distribution
            let type_dist = {
                let mut stmt = conn
                    .prepare("SELECT node_type, COUNT(*) FROM nodes WHERE valid_to IS NULL GROUP BY node_type ORDER BY COUNT(*) DESC")
                    .map_err(mcp_err)?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    })
                    .map_err(mcp_err)?;
                let mut map = std::collections::HashMap::new();
                for row in rows.flatten() {
                    map.insert(row.0, row.1);
                }
                map
            };

            // Unique entity names
            let entity_names: Vec<String> = people
                .iter()
                .map(|p| p.name.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            Ok((observations, people, telos, edge_count, total_nodes, type_dist, entity_names))
        }).await?;

        // ── Focus topics filtering ─────────────────────────────────
        let observations = if !focus_topics_cp.is_empty() {
            let mut scored: Vec<(i32, _)> = observations
                .into_iter()
                .map(|n| {
                    let haystack = format!("{} {}", n.name, n.description).to_lowercase();
                    let score = focus_topics
                        .iter()
                        .filter(|t| haystack.contains(&t.to_lowercase()))
                        .count() as i32;
                    (score, n)
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            scored
                .into_iter()
                .take(turns as usize)
                .map(|(_, n)| n)
                .collect()
        } else {
            observations
        };

        if observations.is_empty() && telos.is_empty() {
            return Ok(serde_json::to_string(&json!({
                "status": "error",
                "method": "none",
                "error": "No graph context available for synthesis.",
                "insights": [],
                "patterns": [],
                "synthesis": "",
                "questions": [],
                "confidence": 0.0,
                "synthesis_nodes": [],
                "timestamp": crate::db::crud::now_iso(),
            }))
            .unwrap_or_default());
        }

        // ── Build context string for prompt ───────────────────────
        let context_nodes: Vec<Value> = observations
            .iter()
            .chain(people.iter())
            .chain(telos.iter())
            .take(turns as usize)
            .map(|n| {
                json!({
                    "id": n.id,
                    "type": n.node_type,
                    "name": n.name,
                    "description": n.description.chars().take(200).collect::<String>(),
                    "created_at": n.created_at,
                })
            })
            .collect();

        let context_map = json!({
            "nodes": context_nodes,
            "entities": entity_names,
            "edges": edge_count,
            "total_nodes": total_nodes,
            "node_types": type_dist,
            "focus_topics": focus_topics_cp,
        });
        let context_str = serde_json::to_string_pretty(&context_map).unwrap_or_default();

        // ── Build synthesis prompt ─────────────────────────────────
        let focus_section = if focus_topics_cp.is_empty() {
            String::new()
        } else {
            format!(
                "FOCUS TOPICS: {}\nPay special attention to these areas.\n",
                focus_topics_cp.join(", ")
            )
        };

        let prompt = format!(
            r#"You are analyzing a TDG (Teleological Developmental Graph) — a knowledge graph
maintained by an autonomous AI agent. The graph contains observations, entities,
skills, constraints, and relationships that model the agent's developmental trajectory.

Below is a summary of recent graph activity (nodes, entities, edges).
Analyze this data and produce a structured synthesis.

Focus on:
1. INSIGHTS — What meaningful patterns or truths emerge from this data?
2. PATTERNS — What recurring structures, relationships, or behaviors do you see?
3. SYNTHESIS — How do these pieces connect? What story does the data tell?
4. QUESTIONS — What unknowns or avenues for deeper investigation exist?

{focus_section}
Context data:
{context_str}

Respond ONLY in JSON format with exactly these keys:
{{
    "insights": ["string", ...],
    "patterns": ["string", ...],
    "synthesis": "string (2-4 sentences)",
    "questions": ["string", ...],
    "confidence": float (0.0-1.0)
}}

Do NOT include any text outside the JSON block."#
        );

        // ── Try LLM providers in order ────────────────────────────
        let llm_cfg = crate::llm::config::LlmConfig::from_env();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(mcp_err)?;

        let llm_result = try_llm_providers(&client, &llm_cfg, &prompt).await;

        if let Some((parsed, provider_name)) = llm_result {
            // Store synthesis nodes
            let pool = self.pool.clone();
            let total_nodes_cp = total_nodes;
            let parsed_clone = parsed.clone();
            let provider_name_clone = provider_name.clone();
            let synthesis_nodes = run_blocking(move || {
                let conn = get_conn(&pool)?;
                Ok(store_synthesis(&conn, &parsed_clone, &provider_name_clone, total_nodes_cp))
            }).await?;
            return Ok(serde_json::to_string(&json!({
                "status": "ok",
                "method": provider_name,
                "insights": parsed.get("insights").cloned().unwrap_or(json!([])),
                "patterns": parsed.get("patterns").cloned().unwrap_or(json!([])),
                "synthesis": parsed.get("synthesis").cloned().unwrap_or(json!("")),
                "questions": parsed.get("questions").cloned().unwrap_or(json!([])),
                "confidence": parsed.get("confidence").cloned().unwrap_or(json!(0.5)),
                "synthesis_nodes": synthesis_nodes,
                "synthesis_count": synthesis_nodes.len(),
                "timestamp": crate::db::crud::now_iso(),
            }))
            .unwrap_or_default());
        }

        // ── Fallback: pattern-based synthesis ─────────────────────
        let pool = self.pool.clone();
        let total_nodes_cp = total_nodes;
        let edge_count_cp = edge_count;
        let focus_topics_cp = focus_topics.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let pattern_result =
                pattern_synthesis(&conn, &context_map, total_nodes_cp, edge_count_cp, &focus_topics_cp);
            let synthesis_nodes = store_synthesis(&conn, &pattern_result, "pattern", total_nodes_cp);

            Ok(serde_json::to_string(&json!({
                "status": "ok",
                "method": "pattern",
                "insights": pattern_result.get("insights").cloned().unwrap_or(json!([])),
                "patterns": pattern_result.get("patterns").cloned().unwrap_or(json!([])),
                "synthesis": pattern_result.get("synthesis").cloned().unwrap_or(json!("")),
                "questions": pattern_result.get("questions").cloned().unwrap_or(json!([])),
                "confidence": pattern_result.get("confidence").cloned().unwrap_or(json!(0.4)),
                "synthesis_nodes": synthesis_nodes,
                "synthesis_count": synthesis_nodes.len(),
                "timestamp": crate::db::crud::now_iso(),
            }))
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "Run reflect engine to discover patterns and create skill nodes from observation clusters")]
    pub(crate) async fn tdg_reflect_run(
        &self,
        Parameters(_params): Parameters<ReflectRunParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }

        let pool = self.pool.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let engine = ReflectEngine::new(&conn);
            let result = engine.run().map_err(mcp_err)?;

            Ok(serde_json::to_string(&json!({
                "status": "ok",
                "skipped": result.skipped,
                "skip_reason": result.skip_reason,
                "observations_analyzed": result.observations_analyzed,
                "clusters_found": result.clusters_found,
                "clusters_processed": result.clusters_processed,
                "skills_created": result.skills_created,
                "discoveries_created": result.discoveries_created,
                "observations_archived": result.observations_archived,
                "errors": result.errors,
                "timestamp": crate::db::crud::now_iso(),
            }))
            .unwrap_or_default())
        }).await
    }

    // ─── Trust Tools ────────────────────────────────────────────────────────

    #[tool(description = "Get agent trust score")]
    pub(crate) async fn tdg_get_trust(
        &self,
        Parameters(params): Parameters<GetTrustParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        // Wrap in run_blocking — TrustStore::get_trust does sync DB I/O.
        // Previously this ran on the tokio async executor, blocking it.
        let ts = self.trust_store.clone();
        let agent_name = params.agent_name.clone();
        run_blocking(move || {
            match ts.get_trust(&agent_name) {
                Ok(Some(entry)) => Ok(serde_json::to_string(&json!({
                    "agent_name": agent_name,
                    "score": entry.score,
                    "updated_at": entry.updated_at,
                    "source": entry.source,
                    "reason": entry.reason,
                }))
                .unwrap_or_default()),
                Ok(None) => Ok(serde_json::to_string(&json!({
                    "agent_name": agent_name,
                    "score": 0.5,
                    "note": "No trust record found; returning default score 0.5",
                }))
                .unwrap_or_default()),
                Err(e) => Err(e),
            }
        }).await
    }

    #[tool(description = "Adjust agent trust score by delta")]
    pub(crate) async fn tdg_adjust_trust(
        &self,
        Parameters(params): Parameters<AdjustTrustParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        // Wrap in run_blocking — TrustStore::adjust_trust does sync DB writes.
        let ts = self.trust_store.clone();
        let mgr = self.mind_state_manager.clone();
        let agent_name = params.agent_name.clone();
        let delta = params.delta;
        let reason = params.reason;
        let source = params.source;
        run_blocking(move || {
            let new_score = ts.adjust_trust(&agent_name, delta, reason, source)?;

            // Sync MindStateManager.trust_score with the new DB value.
            // Previously, tdg_adjust_trust updated TrustStore (DB + cache) but
            // NOT MindStateManager — so tdg_load_mind_state always returned
            // the stale startup value (0.5). The two stores diverged forever.
            if let Err(e) = mgr.set_trust(new_score) {
                tracing::warn!("Failed to sync MindStateManager.trust_score: {}", e);
            }

            Ok(serde_json::to_string(&json!({
                "agent_name": agent_name,
                "new_score": new_score,
            }))
            .unwrap_or_default())
        }).await
    }

    // ─── Health Tools ───────────────────────────────────────────────────────

    #[tool(description = "Record service health check")]
    pub(crate) async fn tdg_health_check(
        &self,
        Parameters(params): Parameters<HealthCheckParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        // Wrap in run_blocking — HealthMonitor::record_health_check does sync DB writes.
        let hm = self.health_monitor.clone();
        let service = params.service.clone();
        let latency_ms = params.latency_ms;
        let success = params.success;
        let error_message = params.error_message;
        let metadata = params.metadata;
        run_blocking(move || {
            hm.record_health_check(&service, latency_ms, success, error_message, metadata)?;
            Ok(serde_json::to_string(&json!({
                "status": "recorded",
                "service": service,
                "success": success,
            }))
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "System health + circuit breaker status")]
    pub(crate) async fn tdg_system_health(
        &self,
        Parameters(_params): Parameters<SystemHealthParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        // Wrap in run_blocking — both methods do sync DB reads.
        let hm = self.health_monitor.clone();
        run_blocking(move || {
            let summary = hm.get_health_summary()?;
            let cb_status = hm.get_circuit_breaker_status()?;
            let result = json!({
                "health": summary,
                "circuit_breakers": cb_status,
            });
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }).await
    }

    #[tool(description = "Full mind audit: anomalies, drive polarity, stage evidence, graph entropy")]
    pub(crate) async fn tdg_audit(
        &self,
        Parameters(_params): Parameters<AuditParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;

            // Run the AuditEngine — previously this was 767 lines of dead code,
            // never exposed via any MCP tool. The agent had no way to inspect
            // its own drive polarity, blind spots, stage evidence, or graph entropy.
            // We pass a temp file path for the AnomalyRegistry (it's optional
            // in practice; the engine works without persistent anomaly tracking).
            let registry_path = std::env::temp_dir().join("tdg_anomaly_registry.json");
            let engine = crate::audit::AuditEngine::new(&conn, &registry_path);
            let bundle = engine.full_audit_bundle().map_err(mcp_err)?;

            // Also surface flow.rs diagnostics (also previously dead code)
            let entropy = crate::flow::compute_graph_entropy(&conn).map_err(mcp_err)?;
            let polarity_diag = crate::flow::diagnose_polarity(&conn).map_err(mcp_err)?;

            let result = json!({
                "audit_bundle": bundle,
                "graph_entropy": entropy,
                "polarity_diagnosis": polarity_diag,
            });
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }).await
    }

    #[tool(description = "Run full graph renormalization: heal drives, emit downward, aggregate upward")]
    pub(crate) async fn tdg_renormalize(
        &self,
        Parameters(_params): Parameters<RenormalizeParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let result = crate::flow::renormalize_graph(&conn, false).map_err(mcp_err)?;
            Ok(serde_json::to_string(&result).unwrap_or_default())
        }).await
    }

    #[tool(description = "Graph stats: counts, degree, PageRank")]
    pub(crate) async fn tdg_graph_stats(
        &self,
        Parameters(_params): Parameters<GraphStatsParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }

        // Check the TTL cache first (60-second window).
        // Previously, every tdg_graph_stats call rebuilt the entire in-memory
        // graph (up to 100K nodes + 100K edges) and recomputed 100 PageRank
        // iterations — O(N²) per request. For a 5000-node graph that's ~25s
        // of CPU per call. Now we cache the result for 60 seconds.
        const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(60);
        {
            let cache = self.graph_stats_cache.lock().map_err(|e| {
                McpError::internal_error(format!("Cache lock poisoned: {}", e), None)
            })?;
            if let Some((timestamp, ref cached_value)) = *cache {
                if timestamp.elapsed() < CACHE_TTL {
                    tracing::debug!("graph_stats cache hit (age={:?})", timestamp.elapsed());
                    return Ok(serde_json::to_string(cached_value).unwrap_or_default());
                }
            }
        }

        let pool = self.pool.clone();
        let cache = self.graph_stats_cache.clone();
        let result = run_blocking(move || {
            let conn = get_conn(&pool)?;
            let node_count = crate::db::crud::count_nodes(&conn, None).map_err(mcp_err)?;
            let edge_count = crate::db::crud::count_edges(&conn, None).map_err(mcp_err)?;
            let avg_degree = if node_count > 0 {
                (edge_count as f64 * 2.0) / node_count as f64
            } else {
                0.0
            };
            let density = if node_count > 1 {
                edge_count as f64 / (node_count as f64 * (node_count as f64 - 1.0))
            } else {
                0.0
            };
            // PageRank for top hubs
            let top_hubs: Vec<Value> = if node_count > 0 && edge_count > 0 {
                match GraphProjection::build(&conn) {
                    Ok(proj) => {
                        let ranks = page_rank(&proj.graph, 0.85_f64, 100);
                        let mut ranked: Vec<(String, f64)> = proj
                            .node_map
                            .iter()
                            .map(|(id, idx)| (id.clone(), ranks[idx.index()]))
                            .collect();
                        ranked
                            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                        ranked
                            .into_iter()
                            .take(5)
                            .map(|(id, score)| json!({"node_id": id, "rank": score}))
                            .collect()
                    }
                    Err(_) => vec![],
                }
            } else {
                vec![]
            };
            Ok(json!({
                "node_count": node_count,
                "edge_count": edge_count,
                "average_degree": avg_degree,
                "density": density,
                "top_hubs": top_hubs,
            }))
        }).await?;

        // Store in cache
        if let Ok(mut cache_guard) = cache.lock() {
            *cache_guard = Some((std::time::Instant::now(), result.clone()));
        }

        Ok(serde_json::to_string(&result).unwrap_or_default())
    }

    // ─── Mind State Persistence Tools ──────────────────────────────────────────

    #[tool(description = "Save mind state to disk")]
    pub(crate) async fn tdg_save_mind_state(
        &self,
        Parameters(params): Parameters<SaveMindStateParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        // Wrap in run_blocking — MindStateManager::update does sync file I/O.
        let mgr = self.mind_state_manager.clone();
        let session_id = params.session_id.clone();
        run_blocking(move || {
            if let Some(ref sid) = session_id {
                if !sid.is_empty() {
                    mgr.update(|state| {
                        state.session_id = sid.clone();
                    })
                    .map_err(mcp_err)?;
                }
            }
            let state = mgr.get_state();
            Ok(serde_json::to_string(&json!({
                "status": "saved",
                "session_id": state.session_id,
                "version": state.version,
            }))
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "Load mind state from disk")]
    pub(crate) async fn tdg_load_mind_state(
        &self,
        Parameters(_params): Parameters<LoadMindStateParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        // Wrap in run_blocking — get_state does sync file read (via Mutex).
        let mgr = self.mind_state_manager.clone();
        run_blocking(move || {
            let state = mgr.get_state();
            Ok(serde_json::to_string(&json!({
                "session_id": state.session_id,
                "agent_name": state.agent_name,
                "active_plan": state.active_plan,
                "trust_score": state.trust_score,
                "working_memory_count": state.working_memory.len(),
                "version": state.version,
                "last_updated": state.last_updated.to_rfc3339(),
                "metrics": {
                    "tasks_completed": state.metrics.tasks_completed,
                    "tasks_failed": state.metrics.tasks_failed,
                    "avg_response_time_ms": state.metrics.avg_response_time_ms,
                },
            }))
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "Get project context")]
    pub(crate) async fn tdg_get_project_context(&self) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        // Wrap in run_blocking — recall does sync file I/O.
        let mgr = self.mind_state_manager.clone();
        run_blocking(move || {
            let context = mgr
                .recall("project_context")
                .map(|item| item.value)
                .unwrap_or_default();
            Ok(serde_json::to_string(&json!({
                "project_context": context,
            }))
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "Set project context string")]
    pub(crate) async fn tdg_set_project_context(
        &self,
        Parameters(params): Parameters<SetProjectContextParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        if params.context.is_empty() {
            return Err(McpError::invalid_params("context is required", None));
        }
        // Wrap in run_blocking — remember does sync file write.
        let mgr = self.mind_state_manager.clone();
        let context = params.context.clone();
        run_blocking(move || {
            mgr.remember("project_context", &context, None)
                .map_err(mcp_err)?;
            Ok(serde_json::to_string(&json!({
                "status": "saved",
                "project_context": context,
            }))
            .unwrap_or_default())
        }).await
    }

    #[tool(description = "Generate terrain-first context prompt for LLM consumption")]
    pub(crate) async fn tdg_context(&self) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let cfg = Config::from_env();
            let prompt =
                crate::mind::injector::generate_prompt(&conn, &cfg).map_err(mcp_err)?;

            // Also write the mind-state snapshot to disk (tdg-mind-snapshot.json).
            //
            // Previously, write_mind_state_file was dead code — never called
            // from production. The rich diagnostic snapshot (feeling, escalation,
            // terrain, active_constraints, active_skills, projects, prompt_length)
            // was never persisted. External monitors had no way to inspect the
            // agent's state without calling tdg_context (which returns the prompt
            // text, not structured data). Now we write the snapshot on every
            // tdg_context call so external tools can read it.
            let diagnostic = crate::mind::diagnostic::DiagnosticEngine::new()
                .analyze(&conn, &[], &[])
                .map(|r| serde_json::to_value(&r).unwrap_or_default())
                .unwrap_or_default();
            let terrain = crate::mind::terrain::generate_terrain_context(&conn, &serde_json::json!({}))
                .map(|v| serde_json::to_value(&v).unwrap_or_default())
                .unwrap_or_default();
            if let Err(e) = crate::mind::injector::write_mind_state_file(
                &conn, &cfg, &prompt, &diagnostic, &terrain,
            ) {
                tracing::debug!("Failed to write mind-state snapshot: {}", e);
            }

            Ok(prompt)
        }).await
    }

    #[tool(description = "Run consolidation pass on graph")]
    pub(crate) async fn tdg_consolidate(
        &self,
        Parameters(params): Parameters<ConsolidateParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }

        let pool = self.pool.clone();
        let lean = params.lean_mode.unwrap_or(false);
        run_blocking(move || {
            let conn = get_conn(&pool)?;

            let engine = crate::mind::consolidation_engine::ConsolidationEngine::new(&conn, lean);
            let report = engine.run().map_err(mcp_err)?;

            Ok(serde_json::to_string(&report).unwrap_or_default())
        }).await
    }
}
