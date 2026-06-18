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

/// Load drive matrix from meta view.
///
/// Python: `load_drive_matrix()` — extracts drive_landscape from meta_view.
pub fn load_drive_matrix(cfg: &Config) -> Value {
    let meta = load_meta_view(cfg);
    meta.get("drive_landscape")
        .or_else(|| meta.get("drive_matrix"))
        .cloned()
        .unwrap_or(serde_json::json!({}))
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
pub fn load_working_memory(cfg: &Config) -> Value {
    robust_json_load(&cfg.working_memory_path(), serde_json::json!({}))
}

/// Load loop state from state file.
///
/// Python: `load_loop_state()` — reads `hermes-loop-state.json`.
pub fn load_loop_state(cfg: &Config) -> Value {
    robust_json_load(&cfg.loop_state_path(), serde_json::json!({}))
}

/// Load polarity state (currently returns empty — placeholder).
///
/// Python: `load_polarity()` — reads polarity state file.
pub fn load_polarity(cfg: &Config) -> Value {
    let path = cfg.state_dir.join("hermes-polarity.json");
    robust_json_load(&path, serde_json::json!({}))
}

/// Load hygiene state (currently returns empty — placeholder).
///
/// Python: `load_hygiene()` — reads hygiene state file.
pub fn load_hygiene(cfg: &Config) -> Value {
    let path = cfg.state_dir.join("hermes-hygiene.json");
    robust_json_load(&path, serde_json::json!({}))
}

/// Load micro slice from meta view.
///
/// Python: `load_micro_slice()` — extracts tactical view from meta_view.
pub fn load_micro_slice(cfg: &Config) -> Value {
    let meta = load_meta_view(cfg);
    meta.get("micro_slice")
        .or_else(|| meta.get("tactical_view"))
        .cloned()
        .unwrap_or(serde_json::json!({}))
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
