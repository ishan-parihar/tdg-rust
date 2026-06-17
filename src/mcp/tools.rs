//! MCP Tool definitions and dispatch
//!
//! Port of all 17 MCP tools from `mcp/tools/*.py`.

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::error::TdgResult;
use crate::mcp::protocol::{ToolDefinition, ToolResult};

/// All 17 MCP tool definitions (JSON Schema format)
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // ── Core tools ──────────────────────────────────────
        ToolDefinition {
            name: "tdg_search".to_string(),
            description: "Search graph memory using hybrid FTS5 full-text search.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query text"},
                    "node_type": {"type": "string", "description": "Optional filter by node type", "default": ""},
                    "limit": {"type": "integer", "description": "Number of results (max 50)", "default": 10, "maximum": 50}
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "tdg_get_node".to_string(),
            description: "Retrieve details for a specific node with optional context.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {"type": "string", "description": "Node ID"},
                    "include_context": {"type": "boolean", "description": "Include neighbors and paths", "default": false}
                },
                "required": ["node_id"]
            }),
        },
        ToolDefinition {
            name: "tdg_query_events".to_string(),
            description: "Query the event log with optional filters.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "description": "Filter by event action", "default": ""},
                    "node_id": {"type": "string", "description": "Filter by node ID", "default": ""},
                    "after": {"type": "string", "description": "Start timestamp (ISO 8601)", "default": ""},
                    "before": {"type": "string", "description": "End timestamp (ISO 8601)", "default": ""},
                    "limit": {"type": "integer", "description": "Max records (500)", "default": 50, "maximum": 500},
                    "offset": {"type": "integer", "description": "Pagination offset", "default": 0}
                }
            }),
        },
        // ── Write tools ─────────────────────────────────────
        ToolDefinition {
            name: "tdg_create".to_string(),
            description: "Create a new graph node with automatic edge wiring.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_type": {"type": "string", "description": "Node type (observation, action, constraint, telos, etc.)"},
                    "name": {"type": "string", "description": "Node name"},
                    "description": {"type": "string", "description": "Node description", "default": ""},
                    "quadrant": {"type": "string", "description": "Quadrant (LR, UL, LL, UR)", "default": ""},
                    "parent_ids": {"type": "string", "description": "Comma-separated parent IDs", "default": ""},
                    "t_level": {"type": "string", "description": "Teleological level", "default": ""},
                    "stage": {"type": "integer", "description": "Developmental stage", "default": 0},
                    "lifecycle_state": {"type": "string", "description": "Lifecycle state", "default": "active"},
                    "source": {"type": "string", "description": "Source identifier", "default": "mcp_tool"},
                    "blocks_targets": {"type": "string", "description": "Comma-separated target IDs for BLOCKS edges", "default": ""},
                    "evidence_targets": {"type": "string", "description": "Comma-separated target IDs for EVIDENCE edges", "default": ""}
                },
                "required": ["node_type", "name"]
            }),
        },
        ToolDefinition {
            name: "tdg_update".to_string(),
            description: "Update a node's details or relationships.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {"type": "string", "description": "Node ID to update"},
                    "name": {"type": "string", "description": "New name", "default": ""},
                    "description": {"type": "string", "description": "New description", "default": ""},
                    "quadrant": {"type": "string", "description": "New quadrant", "default": ""},
                    "lifecycle_state": {"type": "string", "description": "New lifecycle state", "default": ""},
                    "new_type": {"type": "string", "description": "New node type", "default": ""},
                    "t_level": {"type": "string", "description": "New teleological level", "default": ""},
                    "stage": {"type": "integer", "description": "New developmental stage", "default": 0},
                    "add_parent_ids": {"type": "string", "description": "Comma-separated parent IDs to add", "default": ""},
                    "remove_parent_ids": {"type": "string", "description": "Comma-separated parent IDs to remove", "default": ""}
                },
                "required": ["node_id"]
            }),
        },
        ToolDefinition {
            name: "tdg_connect".to_string(),
            description: "Connect two nodes with an edge.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source_id": {"type": "string", "description": "Source node ID"},
                    "target_id": {"type": "string", "description": "Target node ID"},
                    "as_edge": {"type": "string", "description": "Edge type (auto-detected if empty)", "default": ""},
                    "force": {"type": "boolean", "description": "Skip existence check", "default": false}
                },
                "required": ["source_id", "target_id"]
            }),
        },
        ToolDefinition {
            name: "tdg_bulk_create".to_string(),
            description: "Batch-import nodes and edges from JSON arrays.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "nodes_json": {"type": "string", "description": "JSON array of node objects"},
                    "edges_json": {"type": "string", "description": "JSON array of edge objects", "default": ""}
                },
                "required": ["nodes_json"]
            }),
        },
        ToolDefinition {
            name: "tdg_record_exec".to_string(),
            description: "Record an execution outcome as an observation node.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action_type": {"type": "string", "description": "Action type"},
                    "description": {"type": "string", "description": "What was done"},
                    "result": {"type": "string", "description": "Outcome (success, failure, partial)"},
                    "tags": {"type": "string", "description": "Comma-separated tags", "default": ""},
                    "metrics_json": {"type": "string", "description": "JSON metrics object", "default": ""}
                },
                "required": ["action_type", "description", "result"]
            }),
        },
        ToolDefinition {
            name: "tdg_rate_memory".to_string(),
            description: "Adjust a node's confidence rating based on feedback.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {"type": "string", "description": "Node ID to rate"},
                    "helpful": {"type": "boolean", "description": "Was this memory helpful?"}
                },
                "required": ["node_id", "helpful"]
            }),
        },
        // ── Mind tools ──────────────────────────────────────
        ToolDefinition {
            name: "tdg_mind_state".to_string(),
            description: "Get graph health diagnostics, statistics, and mind state.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "detail": {"type": "boolean", "description": "Include detailed breakdown", "default": false},
                    "health": {"type": "boolean", "description": "Run health checks only", "default": false},
                    "verify": {"type": "boolean", "description": "Verify integrity", "default": false}
                }
            }),
        },
        ToolDefinition {
            name: "tdg_observe".to_string(),
            description: "Create an observation node from a description.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "description": {"type": "string", "description": "What was observed"},
                    "quadrant": {"type": "string", "description": "Quadrant", "default": "LR"},
                    "cycle": {"type": "integer", "description": "Current cycle", "default": 0},
                    "trust": {"type": "number", "description": "Initial trust score", "default": 0.5},
                    "entities": {"type": "string", "description": "Comma-separated entity names", "default": ""}
                },
                "required": ["description"]
            }),
        },
        ToolDefinition {
            name: "tdg_get_related".to_string(),
            description: "Traverse relationships from a node by edge type and direction.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {"type": "string", "description": "Source node ID"},
                    "edge_type": {"type": "string", "description": "Filter by edge type", "default": ""},
                    "direction": {"type": "string", "description": "Direction: out, in, or both", "default": "out"},
                    "limit": {"type": "integer", "description": "Max results", "default": 20}
                },
                "required": ["node_id"]
            }),
        },
        // ── Utility tools ───────────────────────────────────
        ToolDefinition {
            name: "tdg_maintenance".to_string(),
            description: "Run graph maintenance (hygiene, archive, or all).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "phase": {"type": "string", "description": "Maintenance phase: hygiene, archive, or all", "default": "all"},
                    "full": {"type": "boolean", "description": "Run full maintenance including contradiction detection", "default": false}
                }
            }),
        },
        ToolDefinition {
            name: "tdg_get_schema".to_string(),
            description: "Introspect the database schema (tables, columns, indexes, row counts).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        // ── Banks tools ─────────────────────────────────────
        ToolDefinition {
            name: "tdg_bank".to_string(),
            description: "Manage multi-agent memory banks.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "description": "Action: list, set_context, or get_nodes", "default": "list"},
                    "profile": {"type": "string", "description": "Profile name", "default": "default"},
                    "bank_id": {"type": "string", "description": "Bank ID (for get_nodes)", "default": ""},
                    "node_type": {"type": "string", "description": "Filter by node type", "default": ""},
                    "limit": {"type": "integer", "description": "Max results", "default": 50}
                }
            }),
        },
        // ── Entity tools ────────────────────────────────────
        ToolDefinition {
            name: "tdg_entity".to_string(),
            description: "Resolve entity names and manage aliases.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Entity name to resolve", "default": ""},
                    "text": {"type": "string", "description": "Text to extract entities from", "default": ""},
                    "node_id": {"type": "string", "description": "Node ID for alias operations", "default": ""},
                    "aliases": {"type": "array", "items": {"type": "string"}, "description": "Aliases to add/update"},
                    "action": {"type": "string", "description": "Action: resolve, get, add, or update", "default": "resolve"}
                }
            }),
        },
        // ── Reflect tools ───────────────────────────────────
        ToolDefinition {
            name: "tdg_reflect".to_string(),
            description: "Trigger LLM-powered synthesis across memory (requires Ollama).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "turns": {"type": "integer", "description": "Number of recent turns to consider", "default": 50},
                    "focus_topics": {"type": "string", "description": "Comma-separated focus topics", "default": ""},
                    "status_only": {"type": "boolean", "description": "Check API/Ollama status only", "default": false}
                }
            }),
        },
    ]
}

/// Lean guard — returns skip response if TDG_LEAN is set
pub fn lean_guard() -> Option<ToolResult> {
    if std::env::var("TDG_LEAN").unwrap_or_default() == "true" {
        Some(ToolResult::err("Operation skipped: TDG_LEAN mode active"))
    } else {
        None
    }
}

/// Dispatch a tool call to the appropriate handler
pub fn dispatch_tool(conn: &Connection, tool_name: &str, args: &Value) -> TdgResult<ToolResult> {
    // Lean guard for heavy operations
    let heavy_ops = [
        "tdg_mind_state",
        "tdg_maintenance",
        "tdg_reflect",
        "tdg_get_schema",
        "tdg_bank",
    ];
    if heavy_ops.contains(&tool_name) {
        if let Some(skip) = lean_guard() {
            return Ok(skip);
        }
    }

    match tool_name {
        "tdg_search" => core_tool::tdg_search(conn, args),
        "tdg_get_node" => core_tool::tdg_get_node(conn, args),
        "tdg_query_events" => core_tool::tdg_query_events(conn, args),
        "tdg_create" => write_tool::tdg_create(conn, args),
        "tdg_update" => write_tool::tdg_update(conn, args),
        "tdg_connect" => write_tool::tdg_connect_tool(conn, args),
        "tdg_bulk_create" => write_tool::tdg_bulk_create(conn, args),
        "tdg_record_exec" => write_tool::tdg_record_exec(conn, args),
        "tdg_rate_memory" => write_tool::tdg_rate_memory(conn, args),
        "tdg_mind_state" => mind_tool::tdg_mind_state(conn, args),
        "tdg_observe" => mind_tool::tdg_observe(conn, args),
        "tdg_get_related" => mind_tool::tdg_get_related(conn, args),
        "tdg_maintenance" => utility_tool::tdg_maintenance(conn, args),
        "tdg_get_schema" => utility_tool::tdg_get_schema(conn),
        "tdg_bank" => banks_tool::tdg_bank(conn, args),
        "tdg_entity" => entity_tool::tdg_entity(conn, args),
        "tdg_reflect" => Ok(ToolResult::err(
            "Reflect tool requires Ollama — not yet ported to Rust",
        )),
        _ => Ok(ToolResult::err(format!("Unknown tool: {}", tool_name))),
    }
}

// ─── Core tools ──────────────────────────────────────────────────────────────

fn core_tool_tdg_search(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let query = args["query"].as_str().unwrap_or("");
    let node_type = args["node_type"].as_str().filter(|s| !s.is_empty());
    let limit = args["limit"].as_i64().unwrap_or(10).min(50);

    let retriever = crate::plugins::HybridRetriever::new();
    let results = retriever.search(conn, query, limit, node_type)?;
    let items: Vec<Value> = results
        .iter()
        .map(|r| {
            json!({
                "id": r.node.id,
                "node_type": r.node.node_type,
                "name": r.node.name,
                "description": r.node.description,
                "confidence": r.node.confidence,
                "score": r.score,
                "source": r.node.source,
                "lifecycle_state": r.node.lifecycle_state,
            })
        })
        .collect();

    Ok(ToolResult::ok(json!({"nodes": items, "total": items.len()})))
}

fn core_tool_tdg_get_node(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let node_id = args["node_id"].as_str().unwrap_or("");
    let include_context = args["include_context"].as_bool().unwrap_or(false);

    if node_id.is_empty() {
        return Ok(ToolResult::err("node_id is required"));
    }

    let node = crate::db::crud::get_node(conn, node_id)?
        .ok_or_else(|| crate::error::TdgError::NotFound(node_id.to_string()))?;

    let mut result = json!({
        "id": node.id,
        "node_type": node.node_type,
        "name": node.name,
        "description": node.description,
        "confidence": node.confidence,
        "lifecycle_state": node.lifecycle_state,
        "source": node.source,
        "created_at": node.created_at,
        "updated_at": node.updated_at,
    });

    if include_context {
        let edges_out = crate::db::crud::get_edges(conn, Some(&node.id), None, None, None, 100)?;
        let edges_in = crate::db::crud::get_edges(conn, None, Some(&node.id), None, None, 100)?;

        result["neighbors"] = json!({
            "outgoing": edges_out.len(),
            "incoming": edges_in.len(),
        });
        result["parents"] = json!(node.parent_ids);
    }

    Ok(ToolResult::ok(result))
}

fn core_tool_tdg_query_events(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let action = args["action"].as_str().filter(|s| !s.is_empty());
    let node_id = args["node_id"].as_str().filter(|s| !s.is_empty());
    let limit = args["limit"].as_i64().unwrap_or(50).min(500);
    let offset = args["offset"].as_i64().unwrap_or(0);

    let mut sql = String::from(
        "SELECT event_id, event_action, node_id, payload_json, timestamp FROM events WHERE 1=1",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(a) = action {
        sql.push_str(" AND event_action = ?");
        param_values.push(Box::new(a.to_string()));
    }
    if let Some(nid) = node_id {
        sql.push_str(" AND node_id = ?");
        param_values.push(Box::new(nid.to_string()));
    }

    sql.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");
    param_values.push(Box::new(limit));
    param_values.push(Box::new(offset));

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(&*param_refs, |row| {
        Ok(json!({
            "event_id": row.get::<_, String>(0)?,
            "event_action": row.get::<_, String>(1)?,
            "node_id": row.get::<_, Option<String>>(2)?,
            "payload_json": row.get::<_, Option<String>>(3)?,
            "timestamp": row.get::<_, String>(4)?,
        }))
    })?;

    let events: Vec<Value> = rows.filter_map(|r| r.ok()).collect();
    Ok(ToolResult::ok(
        json!({"events": events, "total": events.len()}),
    ))
}

pub mod core_tool {
    use super::*;

    pub fn tdg_search(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        core_tool_tdg_search(conn, args)
    }
    pub fn tdg_get_node(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        core_tool_tdg_get_node(conn, args)
    }
    pub fn tdg_query_events(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        core_tool_tdg_query_events(conn, args)
    }
}

// ─── Write tools ─────────────────────────────────────────────────────────────

fn write_tool_tdg_create(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let node_type = args["node_type"].as_str().unwrap_or("observation");
    let name = args["name"].as_str().unwrap_or("");
    if name.is_empty() {
        return Ok(ToolResult::err("name is required"));
    }

    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let source = Some(
        args["source"]
            .as_str()
            .unwrap_or("mcp_tool")
            .to_string(),
    );
    let lifecycle_state = Some(
        args["lifecycle_state"]
            .as_str()
            .unwrap_or("active")
            .to_string(),
    );
    let parent_ids: Option<Vec<String>> = args
        .get("parent_ids")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(|p| p.trim().to_string()).collect());

    let mut quadrants = serde_json::Map::new();
    if let Some(q) = args.get("quadrant").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        quadrants.insert("primary".to_string(), json!(q));
    }

    let mut drives = serde_json::Map::new();
    if let Some(tl) = args.get("t_level").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        drives.insert("teleological_level".to_string(), json!(tl));
    }
    let stage = args["stage"].as_i64().unwrap_or(0);
    if stage > 0 {
        drives.insert("stage".to_string(), json!(stage));
    }

    let node = crate::db::crud::add_node(
        conn,
        &crate::models::NewNode {
            node_type: node_type.to_string(),
            name: name.to_string(),
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
    )?;

    // Auto-wire BLOCKS edges
    if let Some(targets) = args
        .get("blocks_targets")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        for target_id in targets.split(',') {
            let tid = target_id.trim();
            if !tid.is_empty() {
                let _ = crate::db::crud::add_edge(
                    conn,
                    &crate::models::NewEdge {
                        source_id: node.id.clone(),
                        target_id: tid.to_string(),
                        edge_type: "BLOCKS".to_string(),
                        ..Default::default()
                    },
                );
            }
        }
    }

    // Auto-wire EVIDENCE edges
    if let Some(targets) = args
        .get("evidence_targets")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        for target_id in targets.split(',') {
            let tid = target_id.trim();
            if !tid.is_empty() {
                let _ = crate::db::crud::add_edge(
                    conn,
                    &crate::models::NewEdge {
                        source_id: node.id.clone(),
                        target_id: tid.to_string(),
                        edge_type: "EVIDENCE".to_string(),
                        ..Default::default()
                    },
                );
            }
        }
    }

    Ok(ToolResult::ok(json!({
        "id": node.id,
        "name": node.name,
        "node_type": node.node_type,
    })))
}

fn write_tool_tdg_update(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let node_id = args["node_id"].as_str().unwrap_or("");
    if node_id.is_empty() {
        return Ok(ToolResult::err("node_id is required"));
    }

    let node = crate::db::crud::get_node(conn, node_id)?
        .ok_or_else(|| crate::error::TdgError::NotFound(node_id.to_string()))?;

    // Build updates HashMap
    let mut updates = HashMap::new();

    if let Some(name) = args.get("name").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        updates.insert("name".to_string(), json!(name));
    }
    if let Some(desc) = args
        .get("description")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        updates.insert("description".to_string(), json!(desc));
    }
    if let Some(ls) = args
        .get("lifecycle_state")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        updates.insert("lifecycle_state".to_string(), json!(ls));
    }
    if let Some(nt) = args
        .get("new_type")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        updates.insert("node_type_hint".to_string(), json!(nt));
    }
    if let Some(tl) = args
        .get("t_level")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        updates.insert("teleological_level".to_string(), json!(tl));
    }
    let stage = args["stage"].as_i64().unwrap_or(0);
    if stage > 0 {
        updates.insert("developmental_stage".to_string(), json!(stage));
    }

    // Handle parent ID changes
    let mut parent_ids = node.parent_ids.clone();
    if let Some(add) = args
        .get("add_parent_ids")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        for pid in add.split(',') {
            let p = pid.trim().to_string();
            if !p.is_empty() && !parent_ids.contains(&p) {
                parent_ids.push(p);
            }
        }
    }
    if let Some(remove) = args
        .get("remove_parent_ids")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        let remove_set: std::collections::HashSet<&str> =
            remove.split(',').map(|s| s.trim()).collect();
        parent_ids.retain(|p| !remove_set.contains(p.as_str()));
    }

    // The crud update_node only handles specific keys; handle parent_ids separately
    let parent_ids_json = serde_json::to_string(&parent_ids)?;
    updates.insert("parent_ids".to_string(), json!(parent_ids_json));

    let updated = crate::db::crud::update_node(conn, node_id, &updates)?
        .ok_or_else(|| crate::error::TdgError::NotFound(node_id.to_string()))?;

    Ok(ToolResult::ok(json!({
        "id": updated.id,
        "name": updated.name,
        "node_type": updated.node_type,
        "lifecycle_state": updated.lifecycle_state,
    })))
}

fn write_tool_tdg_connect_tool(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let source_id = args["source_id"].as_str().unwrap_or("");
    let target_id = args["target_id"].as_str().unwrap_or("");
    let force = args["force"].as_bool().unwrap_or(false);

    if source_id.is_empty() || target_id.is_empty() {
        return Ok(ToolResult::err("source_id and target_id are required"));
    }

    let edge_type = if let Some(et) = args
        .get("as_edge")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        et.to_string()
    } else {
        let target_type = crate::db::crud::get_node(conn, target_id)?
            .map(|n| n.node_type)
            .unwrap_or_default();
        if target_type == "telos" {
            "EVOLVES_INTO".to_string()
        } else {
            "USES".to_string()
        }
    };

    if !force {
        let edges =
            crate::db::crud::get_edges(conn, Some(source_id), Some(target_id), Some(&edge_type), None, 10)?;
        if !edges.is_empty() {
            return Ok(ToolResult::ok(json!({
                "status": "already_exists",
                "edge_id": edges[0].id,
            })));
        }
    }

    let edge = crate::db::crud::add_edge(
        conn,
        &crate::models::NewEdge {
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            edge_type: edge_type.clone(),
            ..Default::default()
        },
    )?;

    Ok(ToolResult::ok(json!({
        "edge_id": edge.id,
        "source_id": source_id,
        "target_id": target_id,
        "edge_type": edge_type,
    })))
}

fn write_tool_tdg_bulk_create(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let nodes_json = args["nodes_json"].as_str().unwrap_or("[]");
    let nodes: Vec<Value> =
        serde_json::from_str(nodes_json).map_err(crate::error::TdgError::Json)?;

    if nodes.len() > crate::mcp::MAX_BULK_NODES {
        return Ok(ToolResult::err(format!(
            "Too many nodes: {} (max {})",
            nodes.len(),
            crate::mcp::MAX_BULK_NODES
        )));
    }

    let mut created_ids = Vec::new();
    for node_val in &nodes {
        let node_type = node_val["node_type"]
            .as_str()
            .unwrap_or("observation")
            .to_string();
        let name = node_val["name"].as_str().unwrap_or("").to_string();
        let description = node_val["description"]
            .as_str()
            .map(|s| s.to_string());
        let source = node_val["source"].as_str().map(|s| s.to_string());

        let node = crate::db::crud::add_node(
            conn,
            &crate::models::NewNode {
                node_type,
                name,
                description,
                source,
                ..Default::default()
            },
        )?;
        created_ids.push(node.id);
    }

    // Optionally create edges
    let edges_json = args["edges_json"].as_str().unwrap_or("[]");
    let edges: Vec<Value> =
        serde_json::from_str(edges_json).map_err(crate::error::TdgError::Json)?;

    let mut created_edges = 0;
    for edge_val in &edges {
        if let (Some(src), Some(tgt)) = (
            edge_val["source_id"].as_str(),
            edge_val["target_id"].as_str(),
        ) {
            let edge_type = edge_val["edge_type"]
                .as_str()
                .unwrap_or("USES")
                .to_string();
            let _ = crate::db::crud::add_edge(
                conn,
                &crate::models::NewEdge {
                    source_id: src.to_string(),
                    target_id: tgt.to_string(),
                    edge_type,
                    ..Default::default()
                },
            );
            created_edges += 1;
        }
    }

    Ok(ToolResult::ok(json!({
        "created_nodes": created_ids.len(),
        "created_edges": created_edges,
        "node_ids": created_ids,
    })))
}

fn write_tool_tdg_record_exec(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let action_type = args["action_type"].as_str().unwrap_or("exec");
    let description = args["description"].as_str().unwrap_or("");
    let result_str = args["result"].as_str().unwrap_or("unknown");

    let properties = json!({
        "action_type": action_type,
        "result": result_str,
        "tags": args["tags"].as_str().unwrap_or(""),
        "metrics": args["metrics_json"].as_str().unwrap_or("{}"),
    });

    let truncated: String = description.chars().take(80).collect();

    let node = crate::db::crud::add_node(
        conn,
        &crate::models::NewNode {
            node_type: "observation".to_string(),
            name: format!("{}: {}", action_type, truncated),
            description: Some(description.to_string()),
            source: Some("mcp_record_exec".to_string()),
            properties: Some(properties),
            ..Default::default()
        },
    )?;

    // Link to agent:self if it exists
    if let Ok(Some(agent)) = crate::db::crud::get_node(conn, "agent:self") {
        let _ = crate::db::crud::add_edge(
            conn,
            &crate::models::NewEdge {
                source_id: node.id.clone(),
                target_id: agent.id,
                edge_type: "EXPERIENCES".to_string(),
                ..Default::default()
            },
        );
    }

    Ok(ToolResult::ok(json!({
        "observation_id": node.id,
        "action_type": action_type,
        "result": result_str,
    })))
}

fn write_tool_tdg_rate_memory(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let node_id = args["node_id"].as_str().unwrap_or("");
    let helpful = args["helpful"].as_bool().unwrap_or(true);

    if node_id.is_empty() {
        return Ok(ToolResult::err("node_id is required"));
    }

    // rate_node: increment helpful_count if helpful
    let delta = if helpful { 1 } else { -1 };
    conn.execute(
        "UPDATE nodes SET helpful_count = helpful_count + ?1, updated_at = ?2
         WHERE id = ?3 AND valid_to IS NULL",
        rusqlite::params![delta, crate::db::crud::now_iso(), node_id],
    )?;

    // Get trust score: confidence * (1 + helpful_count) / (1 + retrieval_count)
    let trust: f64 = conn
        .query_row(
            "SELECT confidence * (1.0 + helpful_count) / (1.0 + retrieval_count)
             FROM nodes WHERE id = ?1 AND valid_to IS NULL",
            rusqlite::params![node_id],
            |row| row.get(0),
        )
        .unwrap_or(0.0);

    Ok(ToolResult::ok(json!({
        "node_id": node_id,
        "helpful": helpful,
        "trust_score": trust,
    })))
}

pub mod write_tool {
    use super::*;

    pub fn tdg_create(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        write_tool_tdg_create(conn, args)
    }
    pub fn tdg_update(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        write_tool_tdg_update(conn, args)
    }
    pub fn tdg_connect_tool(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        write_tool_tdg_connect_tool(conn, args)
    }
    pub fn tdg_bulk_create(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        write_tool_tdg_bulk_create(conn, args)
    }
    pub fn tdg_record_exec(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        write_tool_tdg_record_exec(conn, args)
    }
    pub fn tdg_rate_memory(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        write_tool_tdg_rate_memory(conn, args)
    }
}

// ─── Mind tools ──────────────────────────────────────────────────────────────

fn mind_tool_tdg_mind_state(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let detail = args["detail"].as_bool().unwrap_or(false);
    let health = args["health"].as_bool().unwrap_or(false);

    let node_count = crate::db::crud::count_nodes(conn, None)?;
    let edge_count = crate::db::crud::count_edges(conn, None)?;

    let mut by_type = serde_json::Map::new();
    for nt in &[
        "observation",
        "action",
        "constraint",
        "telos",
        "skill",
        "hypothesis",
        "discovery",
    ] {
        let count = crate::db::crud::count_nodes(conn, Some(nt))?;
        by_type.insert(nt.to_string(), json!(count));
    }

    let mut result = json!({
        "total_nodes": node_count,
        "total_edges": edge_count,
        "by_type": by_type,
    });

    if detail || health {
        let _micro = crate::ops::micro_slice(conn)?;
        // micro_slice returns a Value directly
        result["micro_slice"] = _micro;
    }

    Ok(ToolResult::ok(result))
}

fn mind_tool_tdg_observe(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let description = args["description"].as_str().unwrap_or("");
    if description.is_empty() {
        return Ok(ToolResult::err("description is required"));
    }

    let quadrant = args["quadrant"].as_str().unwrap_or("LR").to_string();

    let properties = json!({
        "quadrant": quadrant,
        "cycle": args["cycle"].as_i64().unwrap_or(0),
        "trust": args["trust"].as_f64().unwrap_or(0.5),
    });

    let truncated: String = description.chars().take(80).collect();

    let node = crate::db::crud::add_node(
        conn,
        &crate::models::NewNode {
            node_type: "observation".to_string(),
            name: format!("Obs: {}", truncated),
            description: Some(description.to_string()),
            source: Some("mcp_observe".to_string()),
            properties: Some(properties),
            ..Default::default()
        },
    )?;

    // Link to agent:self
    if let Ok(Some(agent)) = crate::db::crud::get_node(conn, "agent:self") {
        let _ = crate::db::crud::add_edge(
            conn,
            &crate::models::NewEdge {
                source_id: node.id.clone(),
                target_id: agent.id,
                edge_type: "EXPERIENCES".to_string(),
                ..Default::default()
            },
        );
    }

    Ok(ToolResult::ok(json!({
        "observation_id": node.id,
        "description": description,
    })))
}

fn mind_tool_tdg_get_related(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let node_id = args["node_id"].as_str().unwrap_or("");
    let edge_type = args["edge_type"].as_str().filter(|s| !s.is_empty());
    let direction = args["direction"].as_str().unwrap_or("out");
    let limit = args["limit"].as_i64().unwrap_or(20);

    if node_id.is_empty() {
        return Ok(ToolResult::err("node_id is required"));
    }

    let mut results = Vec::new();

    // Outgoing edges
    if direction == "out" || direction == "both" {
        let edges =
            crate::db::crud::get_edges(conn, Some(node_id), None, edge_type, None, limit)?;
        for edge in &edges {
            if let Ok(Some(node)) = crate::db::crud::get_node(conn, &edge.target_id) {
                results.push(json!({
                    "id": node.id,
                    "name": node.name,
                    "node_type": node.node_type,
                    "edge_type": edge.edge_type,
                    "direction": "out",
                }));
            }
        }
    }

    // Incoming edges
    if direction == "in" || direction == "both" {
        let edges =
            crate::db::crud::get_edges(conn, None, Some(node_id), edge_type, None, limit)?;
        for edge in &edges {
            if let Ok(Some(node)) = crate::db::crud::get_node(conn, &edge.source_id) {
                results.push(json!({
                    "id": node.id,
                    "name": node.name,
                    "node_type": node.node_type,
                    "edge_type": edge.edge_type,
                    "direction": "in",
                }));
            }
        }
    }

    results.truncate(limit as usize);

    Ok(ToolResult::ok(json!({
        "node_id": node_id,
        "related": results,
        "total": results.len(),
    })))
}

pub mod mind_tool {
    use super::*;

    pub fn tdg_mind_state(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        mind_tool_tdg_mind_state(conn, args)
    }
    pub fn tdg_observe(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        mind_tool_tdg_observe(conn, args)
    }
    pub fn tdg_get_related(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        mind_tool_tdg_get_related(conn, args)
    }
}

// ─── Utility tools ───────────────────────────────────────────────────────────

fn utility_tool_tdg_maintenance(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let phase = args["phase"].as_str().unwrap_or("all");
    let mut report = serde_json::Map::new();

    if phase == "hygiene" || phase == "all" {
        let orphans = crate::knowledge::detect_orphans(conn)?;
        let hygiene = crate::knowledge::generate_hygiene_report(conn)?;
        let orphan_array = orphans
            .get("disconnected")
            .and_then(|v| v.as_array())
            .map_or(0, |a| a.len());
        report.insert("orphans_found".to_string(), json!(orphan_array));
        report.insert(
            "hygiene".to_string(),
            json!({
                "orphan_count": hygiene.orphan_count,
                "dangling_edge_count": hygiene.dangling_edge_count,
                "stale_node_count": hygiene.stale_count,
            }),
        );
    }

    if phase == "archive" || phase == "all" {
        let archived = crate::knowledge::archive_stale_nodes(conn, None)?;
        let count = archived
            .get("archived_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        report.insert("archived_count".to_string(), json!(count));
    }

    Ok(ToolResult::ok(json!(report)))
}

fn utility_tool_tdg_get_schema(conn: &Connection) -> TdgResult<ToolResult> {
    let mut tables = serde_json::Map::new();

    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
    )?;
    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for table_name in &table_names {
        if table_name.starts_with("sqlite_") {
            continue;
        }

        let mut col_stmt =
            conn.prepare(&format!("PRAGMA table_info({})", table_name))?;
        let columns: Vec<Value> = col_stmt
            .query_map([], |row| {
                Ok(json!({
                    "name": row.get::<_, String>(1)?,
                    "type": row.get::<_, String>(2)?,
                    "notnull": row.get::<_, bool>(3)?,
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let count_sql = format!("SELECT COUNT(*) FROM {}", table_name);
        let count: i64 = conn
            .query_row(&count_sql, [], |row| row.get(0))
            .unwrap_or(0);

        tables.insert(
            table_name.clone(),
            json!({
                "columns": columns,
                "row_count": count,
            }),
        );
    }

    Ok(ToolResult::ok(json!({"tables": tables})))
}

pub mod utility_tool {
    use super::*;

    pub fn tdg_maintenance(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        utility_tool_tdg_maintenance(conn, args)
    }
    pub fn tdg_get_schema(conn: &Connection) -> TdgResult<ToolResult> {
        utility_tool_tdg_get_schema(conn)
    }
}

// ─── Banks tools ─────────────────────────────────────────────────────────────

fn banks_tool_tdg_bank(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let action = args["action"].as_str().unwrap_or("list");

    match action {
        "list" => {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT agent_id FROM nodes WHERE agent_id IS NOT NULL AND valid_to IS NULL ORDER BY agent_id",
            )?;
            let banks: Vec<String> = stmt
                .query_map([], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect();

            let bank_data: Vec<Value> = banks
                .iter()
                .map(|bank| {
                    let count: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM nodes WHERE agent_id = ?1 AND valid_to IS NULL",
                            [bank.as_str()],
                            |row| row.get(0),
                        )
                        .unwrap_or(0);
                    json!({"bank_id": bank, "node_count": count})
                })
                .collect();

            Ok(ToolResult::ok(json!({"banks": bank_data})))
        }
        "set_context" => {
            let profile = args["profile"].as_str().unwrap_or("default");
            Ok(ToolResult::ok(json!({"context_set": profile})))
        }
        _ => Ok(ToolResult::err(format!("Unknown bank action: {}", action))),
    }
}

pub mod banks_tool {
    use super::*;

    pub fn tdg_bank(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        banks_tool_tdg_bank(conn, args)
    }
}

// ─── Entity tools ────────────────────────────────────────────────────────────

fn entity_tool_tdg_entity(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
    let action = args["action"].as_str().unwrap_or("resolve");
    let name = args["name"].as_str().unwrap_or("");
    let text = args["text"].as_str().unwrap_or("");

    match action {
        "resolve" => {
            let search_term = if !name.is_empty() { name } else { text };
            if search_term.is_empty() {
                return Ok(ToolResult::err("name or text is required"));
            }

            let q = crate::models::NodeQuery {
                node_type: Some("entity".to_string()),
                limit: Some(10),
                ..Default::default()
            };
            let nodes = crate::db::crud::query_nodes(conn, &q)?;

            let entities: Vec<Value> = nodes
                .iter()
                .filter(|n| {
                    n.name
                        .to_lowercase()
                        .contains(&search_term.to_lowercase())
                })
                .map(|n| {
                    json!({
                        "id": n.id,
                        "name": n.name,
                        "node_type": n.node_type,
                        "confidence": n.confidence,
                    })
                })
                .collect();

            Ok(ToolResult::ok(json!({"entities": entities})))
        }
        "get" => {
            let node_id = args["node_id"].as_str().unwrap_or("");
            if node_id.is_empty() {
                return Ok(ToolResult::err("node_id is required for get"));
            }
            match crate::db::crud::get_node(conn, node_id)? {
                Some(n) => Ok(ToolResult::ok(json!({
                    "id": n.id,
                    "name": n.name,
                    "properties": n.properties,
                }))),
                None => Ok(ToolResult::err("Node not found")),
            }
        }
        _ => Ok(ToolResult::err(format!("Unknown entity action: {}", action))),
    }
}

pub mod entity_tool {
    use super::*;

    pub fn tdg_entity(conn: &Connection, args: &Value) -> TdgResult<ToolResult> {
        entity_tool_tdg_entity(conn, args)
    }
}
