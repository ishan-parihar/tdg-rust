//! TDG Data Loader — filesystem + DB state loading
//!
//! Port of `core/mind/data_loader.py` (193 lines).
//! Provides robust JSON state file loading with graceful failure fallback.

use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;

use crate::config::Config;
use crate::db::crud;
use crate::error::TdgResult;

/// Load a JSON file with graceful fallback to a default value.
///
/// Mirrors Python's `robust_json_load(path, default)`.
pub fn robust_json_load(path: &Path, default: Value) -> Value {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or(default),
        Err(_) => default,
    }
}

/// Load meta view from cache or generate from DB.
///
/// Python: `load_meta_view()` — reads `hermes-meta-view-cache.json`, falls back to empty.
pub fn load_meta_view(cfg: &Config) -> Value {
    robust_json_load(&cfg.meta_view_cache_path(), serde_json::json!({}))
}

/// Load constraints from state file.
///
/// Python: `load_constraints()` — reads `hermes-constraints.json`.
pub fn load_constraints(cfg: &Config) -> Value {
    robust_json_load(&cfg.constraints_path(), serde_json::json!({}))
}

/// Load working memory from state file.
///
/// Python: `load_working_memory()` — reads `hermes-working-memory.json`.
///
/// **Rust-specific bridge**: If `hermes-working-memory.json` doesn't exist or
/// doesn't contain `current_project` / `working_memory`, we fall back to reading
/// `tdg-mind-state.json` (written by `MindStateManager`). This bridges the gap
/// between `tdg_set_project_context` (which writes to MindStateManager) and
/// `tdg_context` / `generate_prompt` (which calls this function). Without this
/// bridge, project context set via MCP is invisible in the generated prompt.
pub fn load_working_memory(cfg: &Config) -> Value {
    let mut wm = robust_json_load(&cfg.working_memory_path(), serde_json::json!({}));

    // Bridge: if the legacy file doesn't have project context, check MindStateManager's file
    let has_project = wm.get("current_project").is_some()
        || wm.get("working_memory").and_then(|v| v.as_array()).map(|a| !a.is_empty()).unwrap_or(false);

    if !has_project {
        let mind_state_path = cfg.state_dir.join("tdg-mind-state.json");
        if let Ok(mind_state_content) = std::fs::read_to_string(&mind_state_path) {
            if let Ok(mind_state) = serde_json::from_str::<serde_json::Value>(&mind_state_content) {
                // MindStateManager stores working_memory as Vec<WorkingMemoryItem>
                // Each item has: { key, value, timestamp }
                if let Some(items) = mind_state.get("working_memory").and_then(|v| v.as_array()) {
                    let mut bridged_wm = serde_json::json!([]);
                    let bridged_arr = bridged_wm.as_array_mut().unwrap();
                    for item in items {
                        if let Some(key) = item.get("key").and_then(|v| v.as_str()) {
                            if let Some(value) = item.get("value") {
                                bridged_arr.push(serde_json::json!({
                                    "key": key,
                                    "value": value,
                                }));
                                // If this is project_context, also set current_project
                                if key == "project_context" {
                                    if let Some(s) = value.as_str() {
                                        wm["current_project"] = serde_json::json!(s);
                                    }
                                }
                            }
                        }
                    }
                    if !bridged_arr.is_empty() && !wm.get("working_memory").is_some() {
                        wm["working_memory"] = bridged_wm;
                    }
                }
            }
        }
    }

    wm
}

/// Load loop state from state file.
///
/// Python: `load_loop_state()` — reads `hermes-loop-state.json`.
pub fn load_loop_state(cfg: &Config) -> Value {
    robust_json_load(&cfg.loop_state_path(), serde_json::json!({}))
}

/// Load recent graph events since last cycle.
///
/// Python: `load_recent_graph_events(loop_state)` — queries nodes/edges created/updated since
/// `last_cycle_at` timestamp.
pub fn load_recent_graph_events(conn: &Connection, loop_state: &Value) -> TdgResult<Vec<Value>> {
    let last_cycle = loop_state
        .get("last_cycle_at")
        .and_then(|v| v.as_str())
        .unwrap_or("1970-01-01T00:00:00");

    let mut events = Vec::new();

    let nodes = crud::query_nodes(
        conn,
        &crate::models::NodeQuery {
            node_type: None,
            lifecycle_state: None,
            source: None,
            teleological_level: None,
            developmental_stage: None,
            quadrant: None,
            agent_id: None,
            include_deleted: false,
            limit: Some(20),
            offset: None,
        },
    )?;

    for node in nodes {
        if node.created_at.as_str() > last_cycle {
            events.push(serde_json::json!({
                "type": "node_created",
                "node_type": node.node_type,
                "name": node.name,
                "id": node.id,
                "timestamp": node.created_at,
            }));
        }
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn robust_json_load_valid() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, r#"{{"key": "value"}}"#).unwrap();
        let result = robust_json_load(f.path(), serde_json::json!({}));
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn robust_json_load_missing_file() {
        let path = Path::new("/nonexistent/path.json");
        let result = robust_json_load(path, serde_json::json!({"default": true}));
        assert_eq!(result["default"], true);
    }

    #[test]
    fn robust_json_load_invalid_json() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "not json").unwrap();
        let result = robust_json_load(f.path(), serde_json::json!({"fallback": true}));
        assert_eq!(result["fallback"], true);
    }

    #[test]
    fn load_meta_view_empty() {
        let cfg = Config::with_db_path(
            tempfile::NamedTempFile::new()
                .unwrap()
                .into_temp_path()
                .to_path_buf(),
        );
        let result = load_meta_view(&cfg);
        assert!(result.is_object());
    }
}
