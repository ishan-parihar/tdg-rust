//! MCP Tools — All 17 TDG tools using official rmcp SDK
//!
//! Uses `#[tool]` and `#[tool_router]` macros for automatic schema generation.

use std::collections::HashMap;
use std::future::Future;
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
    let type_score = if node_count > 0 {
        type_count as f64 / node_count as f64
    } else {
        1.0
    };
    let embedding_score = if node_count > 0 {
        embedding_count as f64 / node_count as f64
    } else {
        1.0
    };
    let fts_score = if node_count > 0 {
        fts_count as f64 / node_count as f64
    } else {
        1.0
    };

    node_score * 0.35
        + edge_score * 0.20
        + type_score * 0.15
        + embedding_score * 0.20
        + fts_score * 0.10
}// ─── Helper to get a connection ──────────────────────────────────────────────

fn get_conn(pool: &ConnectionPool) -> Result<rusqlite::Connection, McpError> {
    pool.get_connection()
        .map_err(|e| McpError::internal_error(e.to_string(), None))
}

fn mcp_err(e: impl std::fmt::Display) -> McpError {
    McpError::internal_error(e.to_string(), None)
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
    pub trust_store: Arc<TrustStore>,
    pub health_monitor: Arc<HealthMonitor>,
    pub mind_state_manager: Arc<MindStateManager>,
    pub lean: bool,
}

#[tool_router(server_handler)]
impl TdgServer {
    pub fn new(pool: ConnectionPool) -> Self {
        let config = Config::from_env();
        let lean = config.lean;
        let mind_state_manager = Arc::new(MindStateManager::new(config));
        let pool = Arc::new(pool);
        Self {
            pool: pool.clone(),
            trust_store: Arc::new(TrustStore::new(pool.clone())),
            health_monitor: Arc::new(HealthMonitor::new(pool)),
            mind_state_manager,
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

            std::fs::write(&output_path, serde_json::to_string_pretty(&export).unwrap_or_default())
                .map_err(|e| mcp_err(anyhow::anyhow!("Write error: {}", e)))?;

            Ok(format!("Exported {} nodes, {} edges to {}", nodes.len(), edges.len(), output_path))
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
        let skip_dupes = params.skip_duplicates.unwrap_or(true);
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let content = std::fs::read_to_string(&input_path)
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
                        "INSERT OR REPLACE INTO nodes (id, node_type, name, description, created_at) VALUES (?1, ?2, ?3, ?4, datetime('now'))",
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
                        "INSERT OR IGNORE INTO edges (source_id, target_id, edge_type, created_at) VALUES (?1, ?2, ?3, datetime('now'))",
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

            let node_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL", [], |r| r.get(0)).unwrap_or(0);
            let edge_count: i64 = conn.query_row("SELECT COUNT(*) FROM edges WHERE valid_to IS NULL", [], |r| r.get(0)).unwrap_or(0);
            let type_count: i64 = conn.query_row("SELECT COUNT(DISTINCT node_type) FROM nodes WHERE valid_to IS NULL", [], |r| r.get(0)).unwrap_or(0);
            let fts_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes_fts", [], |r| r.get(0)).unwrap_or(0);
            let emb_count: i64 = conn.query_row("SELECT COUNT(*) FROM embeddings", [], |r| r.get(0)).unwrap_or(0);
            let mentions: i64 = conn.query_row("SELECT COUNT(*) FROM edges WHERE edge_type = 'MENTIONS' AND valid_to IS NULL", [], |r| r.get(0)).unwrap_or(0);
            let orphans: i64 = conn.query_row(
                "SELECT COUNT(*) FROM edges e WHERE e.valid_to IS NULL AND (e.source_id NOT IN (SELECT id FROM nodes WHERE valid_to IS NULL) OR e.target_id NOT IN (SELECT id FROM nodes WHERE valid_to IS NULL))",
                [], |r| r.get(0),
            ).unwrap_or(0);
            let event_count: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0)).unwrap_or(0);

            let fts_coverage = if node_count > 0 { fts_count as f64 / node_count as f64 } else { 1.0 };
            let emb_coverage = if node_count > 0 { emb_count as f64 / node_count as f64 } else { 1.0 };
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
            if let Some(ref targets) = blocks_targets {
                for tid in targets.split(',') {
                    let tid = tid.trim();
                    if !tid.is_empty() {
                        let _ = crate::db::crud::add_edge(
                            &conn,
                            &NewEdge {
                                source_id: node.id.clone(),
                                target_id: tid.to_string(),
                                edge_type: "BLOCKS".to_string(),
                                ..Default::default()
                            },
                        );
                    }
                }
            }
            if let Some(ref targets) = evidence_targets {
                for tid in targets.split(',') {
                    let tid = tid.trim();
                    if !tid.is_empty() {
                        let _ = crate::db::crud::add_edge(
                            &conn,
                            &NewEdge {
                                source_id: node.id.clone(),
                                target_id: tid.to_string(),
                                edge_type: "EVIDENCE".to_string(),
                                ..Default::default()
                            },
                        );
                    }
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
        let as_edge = params.as_edge.clone();
        let force = params.force.unwrap_or(false);
        run_blocking(move || {
            let conn = get_conn(&pool)?;

            let src = crate::db::crud::get_node(&conn, &source_id)
                .map_err(mcp_err)?
                .ok_or_else(|| mcp_err(format!("Source node not found: {}", source_id)))?;
            let tgt = crate::db::crud::get_node(&conn, &target_id)
                .map_err(mcp_err)?
                .ok_or_else(|| mcp_err(format!("Target node not found: {}", target_id)))?;

            let edge_type = if let Some(ref et) = as_edge {
                if !et.is_empty() {
                    et.clone()
                } else {
                    auto_detect_edge_type(&src.node_type, &tgt.node_type)
                }
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

            let edge = crate::db::crud::add_edge(
                &conn,
                &NewEdge {
                    source_id: source_id.clone(),
                    target_id: target_id.clone(),
                    edge_type: edge_type.clone(),
                    ..Default::default()
                },
            )
            .map_err(mcp_err)?;

            // Flow engine: propagate drives downward from source, then renormalize.
            // Errors are logged, not propagated — flow is non-blocking.
            if let Err(e) = flow::emit_downward(&conn, &source_id, flow::DEFAULT_MAX_DEPTH) {
                tracing::warn!("flow::emit_downward failed after connect {}: {}", edge.id, e);
            }
            if let Err(e) = flow::renormalize_graph(&conn, false) {
                tracing::warn!("flow::renormalize_graph failed after connect {}: {}", edge.id, e);
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
            let edges: Vec<Value> = serde_json::from_str(edges_str).unwrap_or_default();
            let mut ec = 0i64;
            for ev in &edges {
                if let (Some(src), Some(tgt)) = (ev["source_id"].as_str(), ev["target_id"].as_str()) {
                    let _ = crate::db::crud::add_edge(
                        &conn,
                        &NewEdge {
                            source_id: src.to_string(),
                            target_id: tgt.to_string(),
                            edge_type: ev["edge_type"].as_str().unwrap_or("USES").to_string(),
                            ..Default::default()
                        },
                    );
                    ec += 1;
                }
            }
            Ok(serde_json::to_string(
                &json!({"created_nodes": ids.len(), "created_edges": ec, "node_ids": ids}),
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
                let _ = crate::db::crud::add_edge(
                    &conn,
                    &NewEdge {
                        source_id: node.id.clone(),
                        target_id: agent.id,
                        edge_type: "EXPERIENCES".to_string(),
                        ..Default::default()
                    },
                );
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
            conn.execute("UPDATE nodes SET helpful_count = helpful_count + ?1, updated_at = ?2 WHERE id = ?3 AND valid_to IS NULL",
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
            if let Ok(mut stmt) = conn.prepare("SELECT quadrants_json FROM nodes WHERE valid_to IS NULL AND quadrants_json NOT IN ('{}', '')") {
                if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                    for row in rows.flatten() {
                        if let Ok(props) = serde_json::from_str::<serde_json::Value>(&row) {
                            if let Some(primary) = props.get("primary").and_then(|v| v.as_str()) {
                                if let Some(count) = qd.get_mut(primary) {
                                    *count = json!(count.as_i64().unwrap_or(0) + 1);
                                }
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
                                    let pos = props.get(format!("{}_positive_pole", dk))
                                        .or_else(|| props.get(format!("{}.positive_pole", dk)))
                                        .and_then(|v| v.as_f64());
                                    let neg = props.get(format!("{}_negative_pole", dk))
                                        .or_else(|| props.get(format!("{}.negative_pole", dk)))
                                        .and_then(|v| v.as_f64());
                                    let net = props.get(format!("{}_net_score", dk))
                                        .or_else(|| props.get(format!("{}.net_score", dk)))
                                        .and_then(|v| v.as_f64());
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
            let props = json!({
                "quadrant": quadrant,
                "cycle": cycle,
                "trust": trust,
            });
            let node = crate::db::crud::add_node(
                &conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("Obs: {}", truncated),
                    description: Some(description.clone()),
                    source: Some("mcp_observe".to_string()),
                    properties: Some(props),
                    ..Default::default()
                },
            )
            .map_err(mcp_err)?;

            let mut entity_ids: Vec<String> = Vec::new();
            if let Some(ref entities_str) = entities {
                for entity_name in entities_str.split(',') {
                    let name = entity_name.trim().to_string();
                    if name.is_empty() {
                        continue;
                    }
                    let existing = crate::db::crud::search(&conn, &name, 1)
                        .unwrap_or_default()
                        .into_iter()
                        .find(|(n, _)| n.node_type == "entity" && n.name == name)
                        .map(|(n, _)| n);
                    let entity_node = if let Some(n) = existing {
                        n
                    } else {
                        crate::db::crud::add_node(
                            &conn,
                            &NewNode {
                                node_type: "entity".to_string(),
                                name: name.clone(),
                                source: Some("mcp_observe".to_string()),
                                ..Default::default()
                            },
                        )
                        .map_err(mcp_err)?
                    };
                    let _ = crate::db::crud::add_edge(
                        &conn,
                        &crate::models::NewEdge {
                            source_id: node.id.clone(),
                            target_id: entity_node.id.clone(),
                            edge_type: "MENTIONS".to_string(),
                            weight: Some(1.0),
                            properties: None,
                            agent_id: Some("mcp_observe".to_string()),
                        },
                    );
                    entity_ids.push(entity_node.id);
                }
            }

            let entity_extractor = crate::plugins::entity_extractor::EntityExtractor::new();
            let extracted_entities = entity_extractor.extract(&description, Some(&conn));

            let pref_extractor = crate::plugins::preference_extractor::PreferenceExtractor::new();
            let extracted_preferences = pref_extractor.extract_from_message(&description);

            let mut digested = false;
            let mut hypothesis_count = 0usize;
            if trigger_digestion {
                let engine = crate::digestion::DigestionEngine::new(&conn);
                if let Ok(hypotheses) = engine.check_upward_cascade() {
                    hypothesis_count = hypotheses.len();
                    digested = true;
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
                "digested": digested,
                "hypotheses_generated": hypothesis_count,
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
    pub(crate) async fn tdg_maintenance(
        &self,
        Parameters(params): Parameters<MaintenanceParams>,
    ) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let pool = self.pool.clone();
        let phase = params.phase.as_deref().unwrap_or("all").to_string();
        run_blocking(move || {
            let conn = get_conn(&pool)?;
            let mut report = serde_json::Map::new();
            if phase == "hygiene" || phase == "all" {
                let hygiene = crate::knowledge::generate_hygiene_report(&conn).map_err(mcp_err)?;
                report.insert("orphan_count".to_string(), json!(hygiene.orphan_count));
                report.insert(
                    "dangling_edge_count".to_string(),
                    json!(hygiene.dangling_edge_count),
                );
                report.insert("stale_node_count".to_string(), json!(hygiene.stale_count));
            }
            if phase == "archive" || phase == "all" {
                let archived = crate::knowledge::archive_stale_nodes(&conn, None).map_err(mcp_err)?;
                report.insert(
                    "archived_count".to_string(),
                    json!(archived
                        .get("archived_count")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)),
                );
            }
            if phase == "fts_rebuild" || phase == "all" {
                crate::db::schema::rebuild_fts(&conn).map_err(mcp_err)?;
                report.insert("fts_rebuilt".to_string(), json!(true));
            }
            Ok(serde_json::to_string(&json!(report)).unwrap_or_default())
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
        let ts: &TrustStore = &self.trust_store;
        match ts.get_trust(&params.agent_name) {
            Ok(Some(entry)) => Ok(serde_json::to_string(&json!({
                "agent_name": params.agent_name,
                "score": entry.score,
                "updated_at": entry.updated_at,
                "source": entry.source,
                "reason": entry.reason,
            }))
            .unwrap_or_default()),
            Ok(None) => Ok(serde_json::to_string(&json!({
                "agent_name": params.agent_name,
                "score": 0.5,
                "note": "No trust record found; returning default score 0.5",
            }))
            .unwrap_or_default()),
            Err(e) => Err(e),
        }
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
        let new_score = self.trust_store.adjust_trust(
            &params.agent_name,
            params.delta,
            params.reason,
            params.source,
        )?;
        Ok(serde_json::to_string(&json!({
            "agent_name": params.agent_name,
            "new_score": new_score,
        }))
        .unwrap_or_default())
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
        self.health_monitor.record_health_check(
            &params.service,
            params.latency_ms,
            params.success,
            params.error_message,
            params.metadata,
        )?;
        Ok(serde_json::to_string(&json!({
            "status": "recorded",
            "service": params.service,
            "success": params.success,
        }))
        .unwrap_or_default())
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
        let summary = self.health_monitor.get_health_summary()?;
        let cb_status = self.health_monitor.get_circuit_breaker_status()?;
        let result = json!({
            "health": summary,
            "circuit_breakers": cb_status,
        });
        Ok(serde_json::to_string(&result).unwrap_or_default())
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
        let pool = self.pool.clone();
        run_blocking(move || {
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
            Ok(serde_json::to_string(&json!({
                "node_count": node_count,
                "edge_count": edge_count,
                "average_degree": avg_degree,
                "density": density,
                "top_hubs": top_hubs,
            }))
            .unwrap_or_default())
        }).await
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
        if let Some(ref session_id) = params.session_id {
            if !session_id.is_empty() {
                self.mind_state_manager
                    .update(|state| {
                        state.session_id = session_id.clone();
                    })
                    .map_err(mcp_err)?;
            }
        }
        let state = self.mind_state_manager.get_state();
        Ok(serde_json::to_string(&json!({
            "status": "saved",
            "session_id": state.session_id,
            "version": state.version,
        }))
        .unwrap_or_default())
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
        let state = self.mind_state_manager.get_state();
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
    }

    #[tool(description = "Get project context")]
    pub(crate) async fn tdg_get_project_context(&self) -> Result<String, McpError> {
        if self.lean_guard()? {
            return Ok(
                json!({"skipped": true, "reason": "Lean mode active — skipped"}).to_string(),
            );
        }
        let context = self
            .mind_state_manager
            .recall("project_context")
            .map(|item| item.value)
            .unwrap_or_default();
        Ok(serde_json::to_string(&json!({
            "project_context": context,
        }))
        .unwrap_or_default())
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
        self.mind_state_manager
            .remember("project_context", &params.context, None)
            .map_err(mcp_err)?;
        Ok(serde_json::to_string(&json!({
            "status": "saved",
            "project_context": params.context,
        }))
        .unwrap_or_default())
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

async fn try_llm_providers(
    client: &reqwest::Client,
    cfg: &crate::llm::config::LlmConfig,
    prompt: &str,
) -> Option<(Value, String)> {
    if cfg.provider_available("openai") && cfg.openai.api_key.is_some() {
        if let Some(raw) = call_openai(client, cfg, prompt).await {
            if let Some(parsed) = parse_llm_output(&raw) {
                return Some((parsed, "openai".to_string()));
            }
        }
    }
    if cfg.provider_available("anthropic") && cfg.anthropic.api_key.is_some() {
        if let Some(raw) = call_anthropic(client, cfg, prompt).await {
            if let Some(parsed) = parse_llm_output(&raw) {
                return Some((parsed, "anthropic".to_string()));
            }
        }
    }
    if let Some(raw) = call_ollama(client, cfg, prompt).await {
        if let Some(parsed) = parse_llm_output(&raw) {
            return Some((parsed, "ollama".to_string()));
        }
    }
    None
}

async fn call_openai(
    client: &reqwest::Client,
    cfg: &crate::llm::config::LlmConfig,
    prompt: &str,
) -> Option<String> {
    let url = format!(
        "{}/chat/completions",
        cfg.openai.base_url.trim_end_matches('/')
    );
    let payload = json!({
        "model": cfg.openai.model,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": cfg.openai.temperature,
        "max_tokens": cfg.openai.max_tokens,
        "response_format": {"type": "json_object"},
    });
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!("Bearer {}", cfg.openai.api_key.as_deref()?),
        )
        .json(&payload)
        .send()
        .await
        .ok()?;
    let body: Value = resp.json().await.ok()?;
    let content = body
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()?;
    Some(content.to_string())
}

async fn call_anthropic(
    client: &reqwest::Client,
    cfg: &crate::llm::config::LlmConfig,
    prompt: &str,
) -> Option<String> {
    let url = format!("{}/messages", cfg.anthropic.base_url.trim_end_matches('/'));
    let payload = json!({
        "model": cfg.anthropic.model,
        "max_tokens": cfg.anthropic.max_tokens,
        "messages": [{"role": "user", "content": prompt}],
    });
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("x-api-key", cfg.anthropic.api_key.as_deref()?)
        .header("anthropic-version", "2023-06-01")
        .json(&payload)
        .send()
        .await
        .ok()?;
    let body: Value = resp.json().await.ok()?;
    let content_list = body.get("content")?.as_array()?;
    let text_block = content_list
        .iter()
        .find(|b| b.get("type").and_then(|v| v.as_str()) == Some("text"))?;
    Some(text_block.get("text")?.as_str()?.to_string())
}

async fn call_ollama(
    client: &reqwest::Client,
    cfg: &crate::llm::config::LlmConfig,
    prompt: &str,
) -> Option<String> {
    let url = format!(
        "{}/v1/chat/completions",
        cfg.ollama.base_url.trim_end_matches('/')
    );
    let payload = json!({
        "model": cfg.ollama.model,
        "messages": [{"role": "user", "content": prompt}],
        "stream": false,
    });
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .ok()?;
    let body: Value = resp.json().await.ok()?;
    let content = body
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()?;
    Some(content.to_string())
}

fn parse_llm_output(raw: &str) -> Option<Value> {
    let mut text = raw.trim();
    if text.starts_with("```") {
        if let Some(nl) = text.find('\n') {
            text = &text[nl + 1..];
        }
        if let Some(end) = text.rfind("```") {
            text = text[..end].trim();
        }
    }
    if let Ok(data) = serde_json::from_str::<Value>(text) {
        return normalize_synthesis_json(data);
    }
    if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) {
        if end > start {
            if let Ok(data) = serde_json::from_str::<Value>(&text[start..=end]) {
                return normalize_synthesis_json(data);
            }
        }
    }
    None
}

fn normalize_synthesis_json(data: Value) -> Option<Value> {
    let insights = data
        .get("insights")
        .or_else(|| data.get("Insights"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let patterns = data
        .get("patterns")
        .or_else(|| data.get("Patterns"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let questions = data
        .get("questions")
        .or_else(|| data.get("Questions"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let synthesis = data
        .get("synthesis")
        .or_else(|| data.get("Synthesis"))
        .or_else(|| data.get("summary"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let confidence = data
        .get("confidence")
        .or_else(|| data.get("Confidence"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    Some(json!({
        "insights": insights,
        "patterns": patterns,
        "synthesis": synthesis,
        "questions": questions,
        "confidence": confidence,
    }))
}

fn pattern_synthesis(
    conn: &rusqlite::Connection,
    context: &Value,
    total_nodes: i64,
    edge_count: i64,
    focus_topics: &[String],
) -> Value {
    let mut insights: Vec<String> = Vec::new();
    let mut patterns: Vec<String> = Vec::new();
    let mut questions: Vec<String> = Vec::new();

    let node_types = context.get("node_types").and_then(|v| v.as_object());
    let entities = context.get("entities").and_then(|v| v.as_array());
    let obs_count = node_types
        .and_then(|m| m.get("observation"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    // ── Basic node type analysis ────────────────────────────────
    if let Some(types) = node_types {
        let total: i64 = types.values().filter_map(|v| v.as_i64()).sum();
        let mut sorted: Vec<_> = types.iter().collect();
        sorted.sort_by(|a, b| b.1.as_i64().unwrap_or(0).cmp(&a.1.as_i64().unwrap_or(0)));
        let type_summary: Vec<String> = sorted
            .iter()
            .take(5)
            .map(|(t, c)| format!("{}: {}", t, c))
            .collect();
        insights.push(format!(
            "Node distribution — {} total nodes across {} types. Top types: {}.",
            total,
            types.len(),
            type_summary.join(", ")
        ));
        if let Some((dominant_type, dominant_count)) = sorted.first() {
            let count = dominant_count.as_i64().unwrap_or(0);
            if total > 0 {
                patterns.push(format!(
                    "Graph is dominated by '{}' nodes ({}/{} = {:.0}%).",
                    dominant_type,
                    count,
                    total,
                    (count as f64 / total as f64) * 100.0
                ));
            }
        }
    }

    if obs_count > 0 {
        insights.push(format!(
            "Recent context includes {} observation nodes.",
            obs_count
        ));
    } else {
        insights.push("No recent observation activity detected.".to_string());
    }

    // ── Entity analysis ─────────────────────────────────────────
    if let Some(ent_arr) = entities {
        let names: Vec<&str> = ent_arr.iter().filter_map(|v| v.as_str()).collect();
        if !names.is_empty() {
            let display: Vec<&str> = names.iter().take(5).copied().collect();
            insights.push(format!(
                "Graph references {} known entities/people: {}{}.",
                names.len(),
                display.join(", "),
                if names.len() > 5 { "..." } else { "" }
            ));
        }
    }

    // ── Edge density analysis ───────────────────────────────────
    if obs_count > 0 && edge_count > 0 {
        let density = (edge_count as f64) / (obs_count as f64);
        let density_rounded = (density * 100.0).round() / 100.0;
        patterns.push(format!(
            "Edge density: {} edges per observation ({} edges / {} observations).",
            density_rounded, edge_count, obs_count
        ));
        if density < 1.0 {
            insights.push(
                "Low edge density suggests observations are under-connected — cross-linking may improve graph coherence.".to_string()
            );
        }
    }

    if let Some(types) = node_types {
        if types.len() >= 5 {
            patterns.push(format!(
                "Graph has high type diversity ({} types) — indicating a rich, multi-dimensional knowledge structure.",
                types.len()
            ));
        }
    }

    // ── Entity relationship analysis ────────────────────────────
    if let Some(ent_arr) = entities {
        let names: Vec<&str> = ent_arr.iter().filter_map(|v| v.as_str()).collect();
        if names.len() >= 2 {
            // Query edges between entity-adjacent nodes to find co-occurrence
            let rel_query = r#"
                SELECT e.edge_type, COUNT(*) as cnt
                FROM edges e
                JOIN nodes ns ON e.source_id = ns.id
                JOIN nodes nt ON e.target_id = nt.id
                WHERE e.valid_to IS NULL
                  AND (ns.node_type = 'people' OR nt.node_type = 'people')
                GROUP BY e.edge_type
                ORDER BY cnt DESC
                LIMIT 5
            "#;
            if let Ok(mut stmt) = conn.prepare(rel_query) {
                if let Ok(rows) = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                }) {
                    let rel_types: Vec<String> = rows
                        .filter_map(|r| r.ok())
                        .map(|(et, cnt)| format!("{}({})", et, cnt))
                        .collect();
                    if !rel_types.is_empty() {
                        patterns.push(format!(
                            "Entity relationship patterns: {}.",
                            rel_types.join(", ")
                        ));
                    }
                }
            }
        }
    }

    // ── Temporal pattern detection ──────────────────────────────
    {
        let temporal_query = r#"
            SELECT
                created_at,
                node_type
            FROM nodes
            WHERE valid_to IS NULL
            ORDER BY created_at DESC
            LIMIT 100
        "#;
        if let Ok(mut stmt) = conn.prepare(temporal_query) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                let entries: Vec<(String, String)> = rows.filter_map(|r| r.ok()).collect();
                if entries.len() >= 10 {
                    // Check for burst activity (many nodes on same day)
                    let mut day_counts: std::collections::HashMap<String, i64> =
                        std::collections::HashMap::new();
                    for (ts, _) in &entries {
                        let day = ts.chars().take(10).collect::<String>();
                        *day_counts.entry(day).or_insert(0) += 1;
                    }
                    if let Some((peak_day, peak_count)) = day_counts.iter().max_by_key(|(_, c)| *c)
                    {
                        if *peak_count >= 5 {
                            patterns.push(format!(
                                "Temporal burst: {} nodes created on {} — indicates concentrated activity.",
                                peak_count, peak_day
                            ));
                        }
                    }
                    // Check for type clustering over time
                    let mut type_order: Vec<String> =
                        entries.iter().map(|(_, t)| t.clone()).collect();
                    type_order.dedup();
                    if type_order.len() <= 3 && entries.len() >= 10 {
                        patterns.push(format!(
                            "Recent activity concentrated in {} types: {} — suggests focused work phase.",
                            type_order.len(),
                            type_order.join(", ")
                        ));
                    }
                }
            }
        }
    }

    // ── Drive state analysis ────────────────────────────────────
    {
        let drive_query = r#"
            SELECT drives_json FROM nodes
            WHERE valid_to IS NULL
              AND drives_json IS NOT NULL
              AND drives_json != '{}'
            LIMIT 20
        "#;
        if let Ok(mut stmt) = conn.prepare(drive_query) {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                let drive_entries: Vec<String> = rows.filter_map(|r| r.ok()).collect();
                let mut drive_sum: std::collections::HashMap<String, f64> =
                    std::collections::HashMap::new();
                let mut drive_count = 0i64;
                for raw in &drive_entries {
                    if let Ok(val) = serde_json::from_str::<Value>(raw) {
                        if let Some(obj) = val.as_object() {
                            drive_count += 1;
                            for (k, v) in obj {
                                if let Some(f) = v.as_f64() {
                                    *drive_sum.entry(k.clone()).or_insert(0.0) += f;
                                }
                            }
                        }
                    }
                }
                if drive_count > 0 && !drive_sum.is_empty() {
                    let mut avg_drives: Vec<(String, f64)> = drive_sum
                        .iter()
                        .map(|(k, v)| (k.clone(), v / drive_count as f64))
                        .collect();
                    avg_drives
                        .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    let top_drives: Vec<String> = avg_drives
                        .iter()
                        .take(3)
                        .map(|(k, v)| format!("{}({:.2})", k, v))
                        .collect();
                    insights.push(format!(
                        "Active drive dimensions across {} nodes: {}.",
                        drive_count,
                        top_drives.join(", ")
                    ));
                    // Flag depleted drives
                    let depleted: Vec<&String> = avg_drives
                        .iter()
                        .filter(|(_, v)| *v < 0.2)
                        .map(|(k, _)| k)
                        .collect();
                    if !depleted.is_empty() {
                        let depleted_names: Vec<&str> =
                            depleted.iter().map(|s| s.as_str()).collect();
                        questions.push(format!(
                            "Depleted drive dimensions ({}) may need attention — what actions could restore them?",
                            depleted_names.join(", ")
                        ));
                    }
                }
            }
        }
    }

    // ── Focus topics ────────────────────────────────────────────
    if !focus_topics.is_empty() {
        let topic_list = focus_topics.join(", ");
        insights.push(format!("Synthesis focused on: {}.", topic_list));
        for topic in focus_topics.iter().take(3) {
            questions.push(format!(
                "What patterns emerge specifically around '{}'?",
                topic
            ));
        }
    }

    // ── Open questions ──────────────────────────────────────────
    if let Some(ent_arr) = entities {
        let count = ent_arr.len();
        if count > 0 {
            questions.push(format!(
                "How do the {} known entities relate to each other and to the agent's goals?",
                count
            ));
        }
    }
    if obs_count > 50 {
        questions
            .push("With many observations, are there identifiable thematic clusters?".to_string());
    }
    questions
        .push("What emergent developmental patterns exist across the graph structure?".to_string());

    let synthesis = format!(
        "Pattern-based analysis of {} nodes ({} types, {} edges). The graph shows {} observations and {} known entities.",
        total_nodes,
        node_types.map(|m| m.len()).unwrap_or(0),
        edge_count,
        obs_count,
        entities.map(|a| a.len()).unwrap_or(0),
    );

    json!({
        "status": "ok",
        "method": "pattern",
        "insights": insights,
        "patterns": patterns,
        "synthesis": synthesis,
        "questions": questions,
        "confidence": 0.4,
    })
}

fn store_synthesis(
    conn: &rusqlite::Connection,
    result: &Value,
    method: &str,
    synthesis_count_hint: i64,
) -> Vec<String> {
    let mut created = Vec::new();
    let summary = result
        .get("synthesis")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let short_summary: String = summary.chars().take(300).collect();
    let name_preview: String = short_summary.chars().take(80).collect();
    let confidence = result
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);

    let main_node = crate::db::crud::add_node(
        conn,
        &NewNode {
            node_type: "synthesis".to_string(),
            name: format!("Synthesis: {}", name_preview),
            description: Some(short_summary.clone()),
            properties: Some(json!({
                "insights": result.get("insights").cloned().unwrap_or(json!([])),
                "patterns": result.get("patterns").cloned().unwrap_or(json!([])),
                "questions": result.get("questions").cloned().unwrap_or(json!([])),
                "method": method,
                "confidence": confidence,
                "synthesis_count": synthesis_count_hint + 1,
            })),
            quadrants: Some(json!({"primary": "LR", "inferred": true})),
            lifecycle_state: Some("active".to_string()),
            source: Some(format!("reflect_tool/{}", method)),
            confidence: Some(confidence),
            ..Default::default()
        },
    );
    match main_node {
        Ok(node) => {
            let main_id = node.id.clone();
            created.push(main_id.clone());

            let insights = result
                .get("insights")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for (i, insight) in insights.iter().take(5).enumerate() {
                let insight_text = insight.as_str().unwrap_or("");
                let insight_preview: String = insight_text.chars().take(80).collect();
                let insight_full: String = insight_text.chars().take(500).collect();
                if let Ok(sub_node) = crate::db::crud::add_node(
                    conn,
                    &NewNode {
                        node_type: "synthesis".to_string(),
                        name: format!("Insight: {}", insight_preview),
                        description: Some(insight_full),
                        properties: Some(json!({
                            "source_node": main_id,
                            "index": i,
                            "kind": "insight",
                            "method": method,
                        })),
                        quadrants: Some(json!({"primary": "LR", "inferred": true})),
                        lifecycle_state: Some("active".to_string()),
                        source: Some(format!("reflect_tool/{}", method)),
                        confidence: Some(confidence),
                        ..Default::default()
                    },
                ) {
                    created.push(sub_node.id.clone());
                    let _ = crate::db::crud::add_edge(
                        conn,
                        &NewEdge {
                            source_id: sub_node.id,
                            target_id: main_id.clone(),
                            edge_type: "SYNTHESIZES".to_string(),
                            weight: Some(0.9),
                            properties: Some(json!({"kind": "insight_contribution"})),
                            ..Default::default()
                        },
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to store synthesis node: {}", e);
        }
    }
    created
}

fn auto_detect_edge_type(source_type: &str, target_type: &str) -> String {
    match (source_type, target_type) {
        ("action", "telos") => "ENABLES",
        ("observation", "telos") => "EVIDENCES",
        ("artifact", "telos") => "CONTEXT",
        ("people", "telos") => "PURSUES",
        ("hypothesis", "telos") => "EVIDENCES",
        ("constraint", "action") => "BLOCKS",
        ("telos", "telos") => "DECOMPOSES_TO",
        ("observation", "hypothesis") => "EVIDENCES",
        _ => "USES",
    }
    .to_string()
}
