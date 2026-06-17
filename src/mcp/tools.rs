//! MCP Tools — All 17 TDG tools using official rmcp SDK
//!
//! Uses `#[tool]` and `#[tool_router]` macros for automatic schema generation.
//! Uses `rmcp::schemars::JsonSchema` for parameter schemas.

use std::collections::HashMap;
use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::schemars::JsonSchema;
use rmcp::{tool, tool_router, ErrorData as McpError};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::ConnectionPool;
use crate::models::{NewEdge, NewNode, NodeQuery};

use super::MAX_BULK_NODES;

// ─── Parameter Structs ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    #[schemars(description = "Search query text")]
    pub query: String,
    #[schemars(description = "Optional filter by node type")]
    pub node_type: Option<String>,
    #[schemars(description = "Number of results (max 50)")]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetNodeParams {
    #[schemars(description = "Node ID")]
    pub node_id: String,
    #[schemars(description = "Include neighbors and paths")]
    pub include_context: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryEventsParams {
    #[schemars(description = "Filter by event action")]
    pub action: Option<String>,
    #[schemars(description = "Filter by node ID")]
    pub node_id: Option<String>,
    #[schemars(description = "Start timestamp (ISO 8601)")]
    pub after: Option<String>,
    #[schemars(description = "End timestamp (ISO 8601)")]
    pub before: Option<String>,
    #[schemars(description = "Max records (500)")]
    pub limit: Option<i64>,
    #[schemars(description = "Pagination offset")]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateParams {
    #[schemars(description = "Node type (observation, action, constraint, telos, etc.)")]
    pub node_type: String,
    #[schemars(description = "Node name")]
    pub name: String,
    #[schemars(description = "Node description")]
    pub description: Option<String>,
    #[schemars(description = "Quadrant (LR, UL, LL, UR)")]
    pub quadrant: Option<String>,
    #[schemars(description = "Comma-separated parent IDs")]
    pub parent_ids: Option<String>,
    #[schemars(description = "Teleological level")]
    pub t_level: Option<String>,
    #[schemars(description = "Developmental stage")]
    pub stage: Option<i32>,
    #[schemars(description = "Lifecycle state")]
    pub lifecycle_state: Option<String>,
    #[schemars(description = "Source identifier")]
    pub source: Option<String>,
    #[schemars(description = "Comma-separated target IDs for BLOCKS edges")]
    pub blocks_targets: Option<String>,
    #[schemars(description = "Comma-separated target IDs for EVIDENCE edges")]
    pub evidence_targets: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateParams {
    #[schemars(description = "Node ID to update")]
    pub node_id: String,
    #[schemars(description = "New name")]
    pub name: Option<String>,
    #[schemars(description = "New description")]
    pub description: Option<String>,
    #[schemars(description = "New lifecycle state")]
    pub lifecycle_state: Option<String>,
    #[schemars(description = "New node type")]
    pub new_type: Option<String>,
    #[schemars(description = "New teleological level")]
    pub t_level: Option<String>,
    #[schemars(description = "New developmental stage")]
    pub stage: Option<i32>,
    #[schemars(description = "Comma-separated parent IDs to add")]
    pub add_parent_ids: Option<String>,
    #[schemars(description = "Comma-separated parent IDs to remove")]
    pub remove_parent_ids: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConnectParams {
    #[schemars(description = "Source node ID")]
    pub source_id: String,
    #[schemars(description = "Target node ID")]
    pub target_id: String,
    #[schemars(description = "Edge type (auto-detected if empty)")]
    pub as_edge: Option<String>,
    #[schemars(description = "Skip existence check")]
    pub force: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BulkCreateParams {
    #[schemars(description = "JSON array of node objects")]
    pub nodes_json: String,
    #[schemars(description = "JSON array of edge objects")]
    pub edges_json: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecordExecParams {
    #[schemars(description = "Action type")]
    pub action_type: String,
    #[schemars(description = "What was done")]
    pub description: String,
    #[schemars(description = "Outcome (success, failure, partial)")]
    pub result: String,
    #[schemars(description = "Comma-separated tags")]
    pub tags: Option<String>,
    #[schemars(description = "JSON metrics object")]
    pub metrics_json: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RateMemoryParams {
    #[schemars(description = "Node ID to rate")]
    pub node_id: String,
    #[schemars(description = "Was this memory helpful?")]
    pub helpful: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MindStateParams {
    #[schemars(description = "Include detailed breakdown")]
    pub detail: Option<bool>,
    #[schemars(description = "Run health checks only")]
    pub health: Option<bool>,
    #[schemars(description = "Verify integrity")]
    pub verify: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ObserveParams {
    #[schemars(description = "What was observed")]
    pub description: String,
    #[schemars(description = "Quadrant")]
    pub quadrant: Option<String>,
    #[schemars(description = "Current cycle")]
    pub cycle: Option<i64>,
    #[schemars(description = "Initial trust score")]
    pub trust: Option<f64>,
    #[schemars(description = "Comma-separated entity names")]
    pub entities: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetRelatedParams {
    #[schemars(description = "Source node ID")]
    pub node_id: String,
    #[schemars(description = "Filter by edge type")]
    pub edge_type: Option<String>,
    #[schemars(description = "Direction: out, in, or both")]
    pub direction: Option<String>,
    #[schemars(description = "Max results")]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MaintenanceParams {
    #[schemars(description = "Maintenance phase: hygiene, archive, or all")]
    pub phase: Option<String>,
    #[schemars(description = "Run full maintenance")]
    pub full: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BankParams {
    #[schemars(description = "Action: list, set_context, or get_nodes")]
    pub action: Option<String>,
    #[schemars(description = "Profile name")]
    pub profile: Option<String>,
    #[schemars(description = "Bank ID")]
    pub bank_id: Option<String>,
    #[schemars(description = "Filter by node type")]
    pub node_type: Option<String>,
    #[schemars(description = "Max results")]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EntityParams {
    #[schemars(description = "Entity name to resolve")]
    pub name: Option<String>,
    #[schemars(description = "Text to extract entities from")]
    pub text: Option<String>,
    #[schemars(description = "Node ID for alias operations")]
    pub node_id: Option<String>,
    #[schemars(description = "Action: resolve, get, add, or update")]
    pub action: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReflectParams {
    #[schemars(description = "Number of recent turns to consider")]
    pub turns: Option<i64>,
    #[schemars(description = "Comma-separated focus topics")]
    pub focus_topics: Option<String>,
    #[schemars(description = "Check API/Ollama status only")]
    pub status_only: Option<bool>,
}

// ─── Helper to get a connection ──────────────────────────────────────────────

fn get_conn(pool: &ConnectionPool) -> Result<rusqlite::Connection, McpError> {
    pool.get_connection().map_err(|e| McpError::internal_error(e.to_string(), None))
}

fn mcp_err(e: impl std::fmt::Display) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

// ─── TdgServer — the MCP server handler ──────────────────────────────────────

#[derive(Clone)]
pub struct TdgServer {
    pub pool: Arc<ConnectionPool>,
}

#[tool_router(server_handler)]
impl TdgServer {
    pub fn new(pool: ConnectionPool) -> Self {
        Self { pool: Arc::new(pool) }
    }

    #[tool(description = "Search graph memory using hybrid FTS5 full-text search")]
    pub(crate) async fn tdg_search(&self, Parameters(params): Parameters<SearchParams>) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        let limit = params.limit.unwrap_or(10).min(50);
        let node_type = params.node_type.as_deref().filter(|s| !s.is_empty());
        let retriever = crate::plugins::HybridRetriever::new();
        let results = retriever.search(&conn, &params.query, limit, node_type).map_err(mcp_err)?;
        let items: Vec<Value> = results.iter().map(|r| json!({
            "id": r.node.id, "node_type": r.node.node_type, "name": r.node.name,
            "description": r.node.description, "confidence": r.node.confidence, "score": r.score,
        })).collect();
        Ok(serde_json::to_string(&json!({"nodes": items, "total": items.len()})).unwrap_or_default())
    }

    #[tool(description = "Retrieve details for a specific node with optional context")]
    pub(crate) async fn tdg_get_node(&self, Parameters(params): Parameters<GetNodeParams>) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        let node = crate::db::crud::get_node(&conn, &params.node_id).map_err(mcp_err)?
            .ok_or_else(|| McpError::invalid_params(format!("Node {} not found", params.node_id), None))?;
        let mut result = json!({
            "id": node.id, "node_type": node.node_type, "name": node.name,
            "description": node.description, "confidence": node.confidence,
            "lifecycle_state": node.lifecycle_state, "created_at": node.created_at,
        });
        if params.include_context.unwrap_or(false) {
            let out = crate::db::crud::get_edges(&conn, Some(&node.id), None, None, None, 100).unwrap_or_default();
            let inp = crate::db::crud::get_edges(&conn, None, Some(&node.id), None, None, 100).unwrap_or_default();
            result["neighbors"] = json!({"outgoing": out.len(), "incoming": inp.len()});
            result["parents"] = json!(node.parent_ids);
        }
        Ok(serde_json::to_string(&result).unwrap_or_default())
    }

    #[tool(description = "Query the event log with optional filters")]
    pub(crate) async fn tdg_query_events(&self, Parameters(params): Parameters<QueryEventsParams>) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        let limit = params.limit.unwrap_or(50).min(500);
        let offset = params.offset.unwrap_or(0);
        let mut sql = String::from("SELECT event_id, event_action, node_id, payload, timestamp FROM events WHERE 1=1");
        let mut pv: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(ref a) = params.action { if !a.is_empty() { sql.push_str(" AND event_action = ?"); pv.push(Box::new(a.clone())); } }
        if let Some(ref nid) = params.node_id { if !nid.is_empty() { sql.push_str(" AND node_id = ?"); pv.push(Box::new(nid.clone())); } }
        sql.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");
        pv.push(Box::new(limit)); pv.push(Box::new(offset));
        let mut stmt = conn.prepare(&sql).map_err(mcp_err)?;
        let refs: Vec<&dyn rusqlite::types::ToSql> = pv.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(&*refs, |row| Ok(json!({
            "event_id": row.get::<_, String>(0)?, "event_action": row.get::<_, String>(1)?,
            "node_id": row.get::<_, Option<String>>(2)?, "payload": row.get::<_, Option<String>>(3)?,
            "timestamp": row.get::<_, String>(4)?,
        }))).map_err(mcp_err)?;
        let events: Vec<Value> = rows.filter_map(|r| r.ok()).collect();
        Ok(serde_json::to_string(&json!({"events": events, "total": events.len()})).unwrap_or_default())
    }

    #[tool(description = "Create a new graph node with automatic edge wiring")]
    pub(crate) async fn tdg_create(&self, Parameters(params): Parameters<CreateParams>) -> Result<String, McpError> {
        if params.name.is_empty() { return Err(McpError::invalid_params("name is required", None)); }
        let conn = get_conn(&self.pool)?;
        let parent_ids = params.parent_ids.as_deref().filter(|s| !s.is_empty()).map(|s| s.split(',').map(|p| p.trim().to_string()).collect());
        let mut quadrants = serde_json::Map::new();
        if let Some(ref q) = params.quadrant { if !q.is_empty() { quadrants.insert("primary".to_string(), json!(q)); } }
        let mut drives = serde_json::Map::new();
        if let Some(ref tl) = params.t_level { if !tl.is_empty() { drives.insert("teleological_level".to_string(), json!(tl)); } }
        if let Some(stage) = params.stage { if stage > 0 { drives.insert("stage".to_string(), json!(stage)); } }
        let node = crate::db::crud::add_node(&conn, &NewNode {
            node_type: params.node_type, name: params.name, description: params.description,
            source: params.source, lifecycle_state: params.lifecycle_state, parent_ids,
            quadrants: if quadrants.is_empty() { None } else { Some(json!(quadrants)) },
            drives: if drives.is_empty() { None } else { Some(json!(drives)) },
            ..Default::default()
        }).map_err(mcp_err)?;
        if let Some(ref targets) = params.blocks_targets {
            for tid in targets.split(',') { let tid = tid.trim(); if !tid.is_empty() {
                let _ = crate::db::crud::add_edge(&conn, &NewEdge { source_id: node.id.clone(), target_id: tid.to_string(), edge_type: "BLOCKS".to_string(), ..Default::default() });
            } }
        }
        if let Some(ref targets) = params.evidence_targets {
            for tid in targets.split(',') { let tid = tid.trim(); if !tid.is_empty() {
                let _ = crate::db::crud::add_edge(&conn, &NewEdge { source_id: node.id.clone(), target_id: tid.to_string(), edge_type: "EVIDENCE".to_string(), ..Default::default() });
            } }
        }
        Ok(serde_json::to_string(&json!({"id": node.id, "name": node.name, "node_type": node.node_type})).unwrap_or_default())
    }

    #[tool(description = "Update a node's details or relationships")]
    pub(crate) async fn tdg_update(&self, Parameters(params): Parameters<UpdateParams>) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        let mut updates = HashMap::new();
        if let Some(ref n) = params.name { updates.insert("name".to_string(), json!(n)); }
        if let Some(ref d) = params.description { updates.insert("description".to_string(), json!(d)); }
        if let Some(ref ls) = params.lifecycle_state { updates.insert("lifecycle_state".to_string(), json!(ls)); }
        if let Some(ref tl) = params.t_level { updates.insert("teleological_level".to_string(), json!(tl)); }
        if let Some(stage) = params.stage { updates.insert("developmental_stage".to_string(), json!(stage)); }
        let existing = crate::db::crud::get_node(&conn, &params.node_id).map_err(mcp_err)?
            .ok_or_else(|| McpError::invalid_params(format!("Node {} not found", params.node_id), None))?;
        let mut parent_ids = existing.parent_ids.clone();
        if let Some(ref add) = params.add_parent_ids { for pid in add.split(',') { let p = pid.trim().to_string(); if !p.is_empty() && !parent_ids.contains(&p) { parent_ids.push(p); } } }
        if let Some(ref remove) = params.remove_parent_ids { let rm: std::collections::HashSet<&str> = remove.split(',').map(|s| s.trim()).collect(); parent_ids.retain(|p| !rm.contains(p.as_str())); }
        updates.insert("parent_ids".to_string(), json!(serde_json::to_string(&parent_ids).unwrap_or_default()));
        let updated = crate::db::crud::update_node(&conn, &params.node_id, &updates).map_err(mcp_err)?
            .ok_or_else(|| McpError::invalid_params(format!("Node {} not found", params.node_id), None))?;
        Ok(serde_json::to_string(&json!({"id": updated.id, "name": updated.name, "node_type": updated.node_type})).unwrap_or_default())
    }

    #[tool(description = "Connect two nodes with an edge")]
    pub(crate) async fn tdg_connect(&self, Parameters(params): Parameters<ConnectParams>) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        let force = params.force.unwrap_or(false);
        let edge_type = if let Some(ref et) = params.as_edge {
            if !et.is_empty() { et.clone() } else { "USES".to_string() }
        } else {
            let tt = crate::db::crud::get_node(&conn, &params.target_id).unwrap_or(None).map(|n| n.node_type).unwrap_or_default();
            if tt == "telos" { "EVOLVES_INTO".to_string() } else { "USES".to_string() }
        };
        if !force {
            let edges = crate::db::crud::get_edges(&conn, Some(&params.source_id), Some(&params.target_id), Some(&edge_type), None, 10).unwrap_or_default();
            if !edges.is_empty() { return Ok(serde_json::to_string(&json!({"status": "already_exists", "edge_id": edges[0].id})).unwrap_or_default()); }
        }
        let edge = crate::db::crud::add_edge(&conn, &NewEdge { source_id: params.source_id.clone(), target_id: params.target_id.clone(), edge_type: edge_type.clone(), ..Default::default() }).map_err(mcp_err)?;
        Ok(serde_json::to_string(&json!({"edge_id": edge.id, "source_id": params.source_id, "target_id": params.target_id, "edge_type": edge_type})).unwrap_or_default())
    }

    #[tool(description = "Batch-import nodes and edges from JSON arrays")]
    pub(crate) async fn tdg_bulk_create(&self, Parameters(params): Parameters<BulkCreateParams>) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        let nodes: Vec<Value> = serde_json::from_str(&params.nodes_json).map_err(|e| McpError::invalid_params(format!("Invalid nodes_json: {}", e), None))?;
        if nodes.len() > MAX_BULK_NODES { return Err(McpError::invalid_params(format!("Too many nodes: {} (max {})", nodes.len(), MAX_BULK_NODES), None)); }
        let mut ids = Vec::new();
        for nv in &nodes {
            let node = crate::db::crud::add_node(&conn, &NewNode {
                node_type: nv["node_type"].as_str().unwrap_or("observation").to_string(),
                name: nv["name"].as_str().unwrap_or("").to_string(),
                description: nv["description"].as_str().map(|s| s.to_string()),
                source: nv["source"].as_str().map(|s| s.to_string()),
                ..Default::default()
            }).map_err(mcp_err)?;
            ids.push(node.id);
        }
        let edges_json = params.edges_json.as_deref().unwrap_or("[]");
        let edges: Vec<Value> = serde_json::from_str(edges_json).unwrap_or_default();
        let mut ec = 0i64;
        for ev in &edges { if let (Some(src), Some(tgt)) = (ev["source_id"].as_str(), ev["target_id"].as_str()) {
            let _ = crate::db::crud::add_edge(&conn, &NewEdge { source_id: src.to_string(), target_id: tgt.to_string(), edge_type: ev["edge_type"].as_str().unwrap_or("USES").to_string(), ..Default::default() });
            ec += 1;
        } }
        Ok(serde_json::to_string(&json!({"created_nodes": ids.len(), "created_edges": ec, "node_ids": ids})).unwrap_or_default())
    }

    #[tool(description = "Record an execution outcome as an observation node")]
    pub(crate) async fn tdg_record_exec(&self, Parameters(params): Parameters<RecordExecParams>) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        let truncated: String = params.description.chars().take(80).collect();
        let props = json!({"action_type": &params.action_type, "result": &params.result, "tags": params.tags.as_deref().unwrap_or(""), "metrics": params.metrics_json.as_deref().unwrap_or("{}")});
        let node = crate::db::crud::add_node(&conn, &NewNode {
            node_type: "observation".to_string(), name: format!("{}: {}", params.action_type, truncated),
            description: Some(params.description), source: Some("mcp_record_exec".to_string()), properties: Some(props), ..Default::default()
        }).map_err(mcp_err)?;
        if let Ok(Some(agent)) = crate::db::crud::get_node(&conn, "agent:self") {
            let _ = crate::db::crud::add_edge(&conn, &NewEdge { source_id: node.id.clone(), target_id: agent.id, edge_type: "EXPERIENCES".to_string(), ..Default::default() });
        }
        Ok(serde_json::to_string(&json!({"observation_id": node.id, "action_type": params.action_type, "result": params.result})).unwrap_or_default())
    }

    #[tool(description = "Adjust a node's confidence rating based on feedback")]
    pub(crate) async fn tdg_rate_memory(&self, Parameters(params): Parameters<RateMemoryParams>) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        let delta: i32 = if params.helpful { 1 } else { -1 };
        conn.execute("UPDATE nodes SET helpful_count = helpful_count + ?1, updated_at = ?2 WHERE id = ?3 AND valid_to IS NULL",
            rusqlite::params![delta, crate::db::crud::now_iso(), &params.node_id]).map_err(mcp_err)?;
        let trust: f64 = conn.query_row("SELECT confidence * (1.0 + helpful_count) / (1.0 + retrieval_count) FROM nodes WHERE id = ?1 AND valid_to IS NULL", rusqlite::params![&params.node_id], |row| row.get(0)).unwrap_or(0.0);
        Ok(serde_json::to_string(&json!({"node_id": params.node_id, "helpful": params.helpful, "trust_score": trust})).unwrap_or_default())
    }

    #[tool(description = "Get graph health diagnostics, statistics, and mind state")]
    pub(crate) async fn tdg_mind_state(&self, Parameters(_params): Parameters<MindStateParams>) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        let node_count = crate::db::crud::count_nodes(&conn, None).map_err(mcp_err)?;
        let edge_count = crate::db::crud::count_edges(&conn, None).map_err(mcp_err)?;
        let mut by_type = serde_json::Map::new();
        for nt in &["observation", "action", "constraint", "telos", "skill", "hypothesis", "discovery"] {
            by_type.insert(nt.to_string(), json!(crate::db::crud::count_nodes(&conn, Some(nt)).unwrap_or(0)));
        }
        Ok(serde_json::to_string(&json!({"total_nodes": node_count, "total_edges": edge_count, "by_type": by_type})).unwrap_or_default())
    }

    #[tool(description = "Create an observation node from a description")]
    pub(crate) async fn tdg_observe(&self, Parameters(params): Parameters<ObserveParams>) -> Result<String, McpError> {
        if params.description.is_empty() { return Err(McpError::invalid_params("description is required", None)); }
        let conn = get_conn(&self.pool)?;
        let truncated: String = params.description.chars().take(80).collect();
        let props = json!({"quadrant": params.quadrant.unwrap_or_else(|| "LR".to_string()), "cycle": params.cycle.unwrap_or(0), "trust": params.trust.unwrap_or(0.5)});
        let node = crate::db::crud::add_node(&conn, &NewNode {
            node_type: "observation".to_string(), name: format!("Obs: {}", truncated),
            description: Some(params.description), source: Some("mcp_observe".to_string()), properties: Some(props), ..Default::default()
        }).map_err(mcp_err)?;
        Ok(serde_json::to_string(&json!({"observation_id": node.id})).unwrap_or_default())
    }

    #[tool(description = "Traverse relationships from a node by edge type and direction")]
    pub(crate) async fn tdg_get_related(&self, Parameters(params): Parameters<GetRelatedParams>) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        let limit = params.limit.unwrap_or(20);
        let direction = params.direction.as_deref().unwrap_or("out");
        let edge_type = params.edge_type.as_deref().filter(|s| !s.is_empty());
        let mut results = Vec::new();
        if direction == "out" || direction == "both" {
            for edge in crate::db::crud::get_edges(&conn, Some(&params.node_id), None, edge_type, None, limit).unwrap_or_default() {
                if let Ok(Some(n)) = crate::db::crud::get_node(&conn, &edge.target_id) { results.push(json!({"id": n.id, "name": n.name, "node_type": n.node_type, "edge_type": edge.edge_type, "direction": "out"})); }
            }
        }
        if direction == "in" || direction == "both" {
            for edge in crate::db::crud::get_edges(&conn, None, Some(&params.node_id), edge_type, None, limit).unwrap_or_default() {
                if let Ok(Some(n)) = crate::db::crud::get_node(&conn, &edge.source_id) { results.push(json!({"id": n.id, "name": n.name, "node_type": n.node_type, "edge_type": edge.edge_type, "direction": "in"})); }
            }
        }
        results.truncate(limit as usize);
        Ok(serde_json::to_string(&json!({"node_id": params.node_id, "related": results, "total": results.len()})).unwrap_or_default())
    }

    #[tool(description = "Run graph maintenance (hygiene, archive, or all)")]
    pub(crate) async fn tdg_maintenance(&self, Parameters(params): Parameters<MaintenanceParams>) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        let phase = params.phase.as_deref().unwrap_or("all");
        let mut report = serde_json::Map::new();
        if phase == "hygiene" || phase == "all" {
            let hygiene = crate::knowledge::generate_hygiene_report(&conn).map_err(mcp_err)?;
            report.insert("orphan_count".to_string(), json!(hygiene.orphan_count));
            report.insert("dangling_edge_count".to_string(), json!(hygiene.dangling_edge_count));
            report.insert("stale_node_count".to_string(), json!(hygiene.stale_count));
        }
        if phase == "archive" || phase == "all" {
            let archived = crate::knowledge::archive_stale_nodes(&conn, None).map_err(mcp_err)?;
            report.insert("archived_count".to_string(), json!(archived.get("archived_count").and_then(|v| v.as_i64()).unwrap_or(0)));
        }
        Ok(serde_json::to_string(&json!(report)).unwrap_or_default())
    }

    #[tool(description = "Introspect the database schema (tables, columns, row counts)")]
    pub(crate) async fn tdg_get_schema(&self) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        let mut tables = serde_json::Map::new();
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").map_err(mcp_err)?;
        let names: Vec<String> = stmt.query_map([], |row| row.get(0)).map_err(mcp_err)?.filter_map(|r| r.ok()).collect();
        for name in &names {
            if name.starts_with("sqlite_") { continue; }
            let count: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM \"{}\"", name), [], |r| r.get(0)).unwrap_or(0);
            tables.insert(name.clone(), json!({"row_count": count}));
        }
        Ok(serde_json::to_string(&json!({"tables": tables})).unwrap_or_default())
    }

    #[tool(description = "Manage multi-agent memory banks")]
    pub(crate) async fn tdg_bank(&self, Parameters(params): Parameters<BankParams>) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        match params.action.as_deref().unwrap_or("list") {
            "list" => {
                let mut stmt = conn.prepare("SELECT DISTINCT agent_id FROM nodes WHERE agent_id IS NOT NULL AND valid_to IS NULL ORDER BY agent_id").map_err(mcp_err)?;
                let banks: Vec<String> = stmt.query_map([], |row| row.get(0)).map_err(mcp_err)?.filter_map(|r| r.ok()).collect();
                let data: Vec<Value> = banks.iter().map(|b| { let count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes WHERE agent_id = ?1 AND valid_to IS NULL", [b.as_str()], |r| r.get(0)).unwrap_or(0); json!({"bank_id": b, "node_count": count}) }).collect();
                Ok(serde_json::to_string(&json!({"banks": data})).unwrap_or_default())
            }
            "set_context" => Ok(serde_json::to_string(&json!({"context_set": params.profile.as_deref().unwrap_or("default")})).unwrap_or_default()),
            a => Err(McpError::invalid_params(format!("Unknown bank action: {}", a), None)),
        }
    }

    #[tool(description = "Resolve entity names and manage aliases")]
    pub(crate) async fn tdg_entity(&self, Parameters(params): Parameters<EntityParams>) -> Result<String, McpError> {
        let conn = get_conn(&self.pool)?;
        match params.action.as_deref().unwrap_or("resolve") {
            "resolve" => {
                let search = params.name.as_deref().filter(|s| !s.is_empty()).or(params.text.as_deref().filter(|s| !s.is_empty()));
                let term = search.ok_or_else(|| McpError::invalid_params("name or text is required", None))?;
                let q = NodeQuery { node_type: Some("entity".to_string()), limit: Some(10), ..Default::default() };
                let nodes = crate::db::crud::query_nodes(&conn, &q).map_err(mcp_err)?;
                let entities: Vec<Value> = nodes.iter().filter(|n| n.name.to_lowercase().contains(&term.to_lowercase())).map(|n| json!({"id": n.id, "name": n.name})).collect();
                Ok(serde_json::to_string(&json!({"entities": entities})).unwrap_or_default())
            }
            "get" => {
                let nid = params.node_id.as_deref().unwrap_or("");
                if nid.is_empty() { return Err(McpError::invalid_params("node_id required", None)); }
                match crate::db::crud::get_node(&conn, nid).map_err(mcp_err)? {
                    Some(n) => Ok(serde_json::to_string(&json!({"id": n.id, "name": n.name, "properties": n.properties})).unwrap_or_default()),
                    None => Err(McpError::invalid_params("Node not found", None)),
                }
            }
            a => Err(McpError::invalid_params(format!("Unknown entity action: {}", a), None)),
        }
    }

    #[tool(description = "Check Ollama status or trigger LLM reflection (requires Ollama)")]
    pub(crate) async fn tdg_reflect(&self, Parameters(_params): Parameters<ReflectParams>) -> Result<String, McpError> {
        let ollama_url = std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());
        let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build().map_err(mcp_err)?;
        let status = client.get(format!("{}/api/tags", ollama_url)).send().await;
        match status {
            Ok(resp) if resp.status().is_success() => Ok(serde_json::to_string(&json!({"status": "ollama_available", "url": ollama_url})).unwrap_or_default()),
            _ => Ok(serde_json::to_string(&json!({"status": "ollama_not_available", "url": ollama_url})).unwrap_or_default()),
        }
    }
}
