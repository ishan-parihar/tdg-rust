//! TDG Operations & Facade
//!
//! Port of `core/tdg_ops.py` and `core/tdg_impl.py`.
//!
//! High-level operations: reconcile, micro/macro slice, record_action,
//! flow_up, polarity, hygiene, stage_status, drive_matrix_report.
//! Plus CLI command dispatchers for graph, dream, knowledge operations.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::db::crud::{
    count_nodes, get_node, now_iso, query_nodes, record_event,
};
use crate::error::TdgResult;
use crate::flow::{self, FlowDriveState};
use crate::knowledge;
use crate::mind::diagnostic::DiagnosticEngine;
use crate::mind::pulse::PulseEngine;
use crate::models::{NewNode, NodeQuery};

// ─── Reconcile ───────────────────────────────────────────────────────────────

/// Reconcile drive states across the graph.
/// Performs a full renormalization and returns summary.
pub fn reconcile(conn: &Connection) -> TdgResult<serde_json::Value> {
    let result = flow::renormalize_graph(conn, false)?;

    // Record the reconcile event
    record_event(
        conn,
        "reconcile",
        None,
        None,
        None,
        Some(&result),
    )?;

    Ok(serde_json::json!({
        "status": "completed",
        "renormalization": result,
    }))
}

// ─── Micro Slice ─────────────────────────────────────────────────────────────

/// Tactical view: current quadrant focus, available actions, next steps.
pub fn micro_slice(conn: &Connection) -> TdgResult<serde_json::Value> {
    // Get active actions
    let q = NodeQuery {
        node_type: Some("action".to_string()),
        lifecycle_state: Some("active".to_string()),
        limit: Some(50),
        ..Default::default()
    };
    let actions = query_nodes(conn, &q)?;

    // Get active teloi
    let telos_q = NodeQuery {
        node_type: Some("telos".to_string()),
        lifecycle_state: Some("active".to_string()),
        limit: Some(10),
        ..Default::default()
    };
    let teloi = query_nodes(conn, &telos_q)?;

    // Get constraints
    let constraint_q = NodeQuery {
        node_type: Some("constraint".to_string()),
        lifecycle_state: Some("active".to_string()),
        limit: Some(20),
        ..Default::default()
    };
    let constraints = query_nodes(conn, &constraint_q)?;

    // Categorize actions by quadrant
    let mut by_quadrant: HashMap<String, Vec<&str>> = HashMap::new();
    for action in &actions {
        let quadrant = action
            .quadrants
            .get("active_quadrant")
            .and_then(|v| v.as_str())
            .unwrap_or("unassigned");
        by_quadrant
            .entry(quadrant.to_string())
            .or_default()
            .push(&action.name);
    }

    // Check for blocked actions
    let blocked: Vec<&str> = actions
        .iter()
        .filter(|a| {
            a.properties
                .get("blocked")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .map(|a| a.name.as_str())
        .collect();

    // Find next recommended action (highest confidence, not blocked)
    let next_action = actions
        .iter()
        .filter(|a| {
            !a.properties
                .get("blocked")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .max_by(|a, b| {
            a.confidence
                .partial_cmp(&b.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

    Ok(serde_json::json!({
        "summary": {
            "total_actions": actions.len(),
            "total_teloi": teloi.len(),
            "total_constraints": constraints.len(),
        },
        "by_quadrant": by_quadrant,
        "blocked_actions": blocked,
        "next_action": next_action.map(|a| serde_json::json!({
            "id": a.id,
            "name": a.name,
            "node_type": a.node_type,
            "confidence": a.confidence,
        })),
        "teloi": teloi.iter().map(|t| serde_json::json!({
            "id": t.id,
            "name": t.name,
            "confidence": t.confidence,
        })).collect::<Vec<_>>(),
    }))
}

// ─── Record Action ───────────────────────────────────────────────────────────

/// Record an action execution with optional quadrant and entities.
pub fn record_action(
    conn: &Connection,
    action_name: &str,
    quadrant: Option<&str>,
    entities: Option<&[String]>,
    notes: Option<&str>,
) -> TdgResult<serde_json::Value> {
    let now = now_iso();

    // Create an observation node for this action
    let mut properties = serde_json::json!({
        "action_name": action_name,
        "recorded_at": now,
    });

    if let Some(q) = quadrant {
        properties
            .as_object_mut()
            .unwrap()
            .insert("quadrant".to_string(), serde_json::json!(q));
    }

    if let Some(n) = notes {
        properties
            .as_object_mut()
            .unwrap()
            .insert("notes".to_string(), serde_json::json!(n));
    }

    if let Some(ent) = entities {
        properties
            .as_object_mut()
            .unwrap()
            .insert("entities".to_string(), serde_json::json!(ent));
    }

    let node = crate::db::crud::add_node(
        conn,
        &NewNode {
            node_type: "observation".to_string(),
            name: format!("Action: {action_name}"),
            description: notes.map(|s| s.to_string()),
            source: Some("action_record".to_string()),
            properties: Some(properties),
            ..Default::default()
        },
    )?;

    // Record the event
    let event_id = record_event(
        conn,
        "action_recorded",
        Some(&node.id),
        None,
        None,
        Some(&serde_json::json!({
            "action_name": action_name,
            "quadrant": quadrant,
            "entities": entities,
        })),
    )?;

    Ok(serde_json::json!({
        "action_id": node.id,
        "event_id": event_id,
        "recorded_at": now,
    }))
}

// ─── Flow Up ─────────────────────────────────────────────────────────────────

/// Propagate drive energy upward from an action to its parent teloi.
pub fn flow_up(conn: &Connection, node_id: &str) -> TdgResult<serde_json::Value> {
    let updated = flow::aggregate_upward(conn, node_id)?;

    record_event(
        conn,
        "flow_up",
        Some(node_id),
        None,
        None,
        Some(&serde_json::json!({ "parents_updated": updated })),
    )?;

    Ok(serde_json::json!({
        "node_id": node_id,
        "parents_updated": updated,
    }))
}

// ─── Polarity ────────────────────────────────────────────────────────────────

/// Full polarity diagnosis across the graph.
pub fn polarity(conn: &Connection) -> TdgResult<serde_json::Value> {
    flow::diagnose_polarity(conn)
}

// ─── Hygiene ─────────────────────────────────────────────────────────────────

/// Run a full knowledge hygiene cycle.
pub fn hygiene(conn: &Connection) -> TdgResult<serde_json::Value> {
    let orphans = knowledge::detect_orphans(conn)?;
    let dangling = knowledge::prune_dangling_edges(conn)?;
    let archived = knowledge::archive_stale_nodes(conn, None)?;
    let report = knowledge::generate_hygiene_report(conn)?;

    Ok(serde_json::json!({
        "orphans": orphans,
        "dangling_pruned": dangling,
        "archived": archived,
        "report": serde_json::to_value(&report).unwrap_or(serde_json::json!({})),
    }))
}

// ─── Macro Slice ─────────────────────────────────────────────────────────────

/// Strategic-level health view of the graph.
pub fn macro_slice(conn: &Connection, depth: Option<i64>) -> TdgResult<serde_json::Value> {
    let _depth = depth.unwrap_or(3);

    // Health metrics
    let total_nodes = count_nodes(conn, None)?;
    let total_edges: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges WHERE valid_to IS NULL",
        [],
        |row| row.get(0),
    )?;

    // Drive distribution
    let entropy = flow::compute_graph_entropy(conn)?;

    // Pulse analysis
    let pulse_engine = PulseEngine::new();
    let pulse_results = pulse_engine.pulse(conn, &[])?;
    let pulse_summary = pulse_engine.summarize(&pulse_results);

    // Diagnostic analysis
    let diag_engine = DiagnosticEngine::new();
    let report = diag_engine.analyze(conn, &[], &[])?;

    Ok(serde_json::json!({
        "health": {
            "total_nodes": total_nodes,
            "total_edges": total_edges,
        },
        "entropy": entropy,
        "pulse": pulse_summary,
        "diagnostics": {
            "pattern_flags": report.pattern_flags.len(),
            "blind_spots": report.blind_spots,
            "ghost_nodes": report.ghost_nodes,
            "suggestion": report.suggestion,
        },
    }))
}

// ─── Stage Status ────────────────────────────────────────────────────────────

/// Stage progression summary for all active teloi.
pub fn stage_status(conn: &Connection) -> TdgResult<serde_json::Value> {
    let q = NodeQuery {
        node_type: Some("telos".to_string()),
        lifecycle_state: Some("active".to_string()),
        limit: Some(100),
        ..Default::default()
    };
    let teloi = query_nodes(conn, &q)?;

    let stages: Vec<serde_json::Value> = teloi
        .iter()
        .map(|t| {
            let stage = t
                .developmental_stage
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unset".to_string());
            let level = t
                .teleological_level
                .clone()
                .unwrap_or_else(|| "unknown".to_string());

            // Count children (decomposed actions)
            let child_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM edges WHERE source_id = ?1 AND edge_type = 'DECOMPOSES_TO' AND valid_to IS NULL",
                    rusqlite::params![t.id],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            serde_json::json!({
                "id": t.id,
                "name": t.name,
                "level": level,
                "stage": stage,
                "confidence": t.confidence,
                "children": child_count,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "total_teloi": stages.len(),
        "stages": stages,
    }))
}

// ─── Drive Matrix Report ─────────────────────────────────────────────────────

/// Generate a 16-cell drive matrix report.
pub fn drive_matrix_report(
    conn: &Connection,
    node_id: Option<&str>,
) -> TdgResult<serde_json::Value> {
    if let Some(nid) = node_id {
        // Single node detail report
        let node = get_node(conn, nid)?
            .ok_or_else(|| crate::error::TdgError::Custom(format!("Node {nid} not found")))?;

        let state = FlowDriveState::from_json(&node.drives);

        let cells = serde_json::json!({
            "eros": {
                "positive_pole": state.eros.positive_pole,
                "negative_pole": state.eros.negative_pole,
                "net": state.eros.net(),
                "diagnosis": format!("{:?}", state.eros.diagnose()),
            },
            "agape": {
                "positive_pole": state.agape.positive_pole,
                "negative_pole": state.agape.negative_pole,
                "net": state.agape.net(),
                "diagnosis": format!("{:?}", state.agape.diagnose()),
            },
            "agency": {
                "positive_pole": state.agency.positive_pole,
                "negative_pole": state.agency.negative_pole,
                "net": state.agency.net(),
                "diagnosis": format!("{:?}", state.agency.diagnose()),
            },
            "communion": {
                "positive_pole": state.communion.positive_pole,
                "negative_pole": state.communion.negative_pole,
                "net": state.communion.net(),
                "diagnosis": format!("{:?}", state.communion.diagnose()),
            },
        });

        // Find imbalances
        let mut imbalances = Vec::new();
        for (name, drive) in [
            ("eros", &state.eros),
            ("agape", &state.agape),
            ("agency", &state.agency),
            ("communion", &state.communion),
        ] {
            let diag = drive.diagnose();
            if diag != crate::flow::DriveDiagnosis::Integrated {
                imbalances.push(serde_json::json!({
                    "drive": name,
                    "diagnosis": format!("{:?}", diag),
                    "net": drive.net(),
                }));
            }
        }

        Ok(serde_json::json!({
            "node_id": nid,
            "name": node.name,
            "cells": cells,
            "imbalances": imbalances,
            "integrated_count": 4 - imbalances.len(),
        }))
    } else {
        // Graph-wide summary
        let entropy = flow::compute_graph_entropy(conn)?;
        let polarity = flow::diagnose_polarity(conn)?;

        Ok(serde_json::json!({
            "summary": true,
            "entropy": entropy,
            "polarity": polarity,
        }))
    }
}

// ─── CLI Command Dispatchers ─────────────────────────────────────────────────

/// Graph operations dispatcher.
pub fn cmd_graph(
    conn: &Connection,
    subcommand: &str,
    args: &HashMap<String, String>,
) -> TdgResult<serde_json::Value> {
    match subcommand {
        "stats" => crate::db::events::stats(conn),
        "create-node" => {
            let node_type = args.get("node_type").ok_or_else(|| {
                crate::error::TdgError::Custom("Missing --node-type".to_string())
            })?;
            let name = args.get("name").ok_or_else(|| {
                crate::error::TdgError::Custom("Missing --name".to_string())
            })?;
            let node = crate::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: node_type.clone(),
                    name: name.clone(),
                    description: args.get("description").cloned(),
                    source: Some("cli".to_string()),
                    ..Default::default()
                },
            )?;
            Ok(serde_json::to_value(&node)?)
        }
        "search" => {
            let query = args.get("query").ok_or_else(|| {
                crate::error::TdgError::Custom("Missing --query".to_string())
            })?;
            let limit = args
                .get("limit")
                .and_then(|l| l.parse().ok())
                .unwrap_or(10);
            let results = crate::db::crud::search(conn, query, limit)?;
            Ok(serde_json::json!({
                "results": results.iter().map(|(n, score)| serde_json::json!({
                    "id": n.id,
                    "name": n.name,
                    "node_type": n.node_type,
                    "score": score,
                })).collect::<Vec<_>>(),
                "count": results.len(),
            }))
        }
        "query" => {
            let node_type = args.get("node_type").map(|s| s.clone());
            let limit = args
                .get("limit")
                .and_then(|l| l.parse().ok())
                .unwrap_or(100);
            let q = NodeQuery {
                node_type,
                limit: Some(limit),
                ..Default::default()
            };
            let results = query_nodes(conn, &q)?;
            Ok(serde_json::json!({
                "results": results.iter().map(|n| serde_json::json!({
                    "id": n.id,
                    "name": n.name,
                    "node_type": n.node_type,
                })).collect::<Vec<_>>(),
                "count": results.len(),
            }))
        }
        "pathfind" => {
            let source = args.get("source").ok_or_else(|| {
                crate::error::TdgError::Custom("Missing --source".to_string())
            })?;
            let target = args.get("target").ok_or_else(|| {
                crate::error::TdgError::Custom("Missing --target".to_string())
            })?;
            let max_depth = args
                .get("max-depth")
                .and_then(|l| l.parse().ok())
                .unwrap_or(5);
            let paths = crate::db::crud::pathfind(conn, source, target, max_depth, 100)?;
            Ok(serde_json::json!({
                "paths": paths,
                "count": paths.len(),
            }))
        }
        _ => Err(crate::error::TdgError::Custom(format!(
            "Unknown graph subcommand: {subcommand}"
        ))),
    }
}

/// Knowledge operations dispatcher.
pub fn cmd_knowledge(
    conn: &Connection,
    subcommand: &str,
    args: &HashMap<String, String>,
) -> TdgResult<serde_json::Value> {
    match subcommand {
        "stats" => {
            let report = knowledge::generate_hygiene_report(conn)?;
            Ok(serde_json::to_value(&report)?)
        }
        "detect-orphans" => knowledge::detect_orphans(conn),
        "prune-dangling" => knowledge::prune_dangling_edges(conn),
        "archive-stale" => knowledge::archive_stale_nodes(conn, None),
        "hygiene" => hygiene(conn),
        "process-lifecycle" => {
            let node_id = args.get("node-id").ok_or_else(|| {
                crate::error::TdgError::Custom("Missing --node-id".to_string())
            })?;
            knowledge::process_catalyst_lifecycle(conn, node_id)
        }
        _ => Err(crate::error::TdgError::Custom(format!(
            "Unknown knowledge subcommand: {subcommand}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_fts, init_schema, run_migrations};
    use crate::models::NewEdge;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn micro_slice_basic() {
        let conn = setup_db();
        crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "Test Action".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let result = micro_slice(&conn).unwrap();
        assert_eq!(result["summary"]["total_actions"], 1);
    }

    #[test]
    fn record_action_basic() {
        let conn = setup_db();
        let result = record_action(&conn, "test_action", Some("UR"), None, Some("notes")).unwrap();
        assert!(result.get("action_id").is_some());
        assert!(result.get("event_id").is_some());
    }

    #[test]
    fn stage_status_basic() {
        let conn = setup_db();
        crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Main Telos".to_string(),
                developmental_stage: Some(3),
                ..Default::default()
            },
        )
        .unwrap();

        let result = stage_status(&conn).unwrap();
        assert_eq!(result["total_teloi"], 1);
    }

    #[test]
    fn drive_matrix_single_node() {
        let conn = setup_db();
        let node = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Matrix Telos".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Set a drive state
        let state = FlowDriveState::intrinsic("telos");
        let json = state.to_json();
        conn.execute(
            "UPDATE nodes SET drives_json = ?1 WHERE id = ?2",
            rusqlite::params![json.to_string(), node.id],
        )
        .unwrap();

        let result = drive_matrix_report(&conn, Some(&node.id)).unwrap();
        assert!(result.get("cells").is_some());
        assert_eq!(result["node_id"], node.id);
    }

    #[test]
    fn cmd_graph_search() {
        let conn = setup_db();
        crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Rust memory safety".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let mut args = HashMap::new();
        args.insert("query".to_string(), "Rust".to_string());
        args.insert("limit".to_string(), "10".to_string());

        let result = cmd_graph(&conn, "search", &args).unwrap();
        assert_eq!(result["count"], 1);
    }

    #[test]
    fn cmd_knowledge_stats() {
        let conn = setup_db();
        let result = cmd_knowledge(&conn, "stats", &HashMap::new()).unwrap();
        assert!(result.get("total_nodes").is_some());
    }

    #[test]
    fn reconcile_basic() {
        let conn = setup_db();
        let telos = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Reconcile Telos".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let action = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "Reconcile Action".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        crate::db::crud::add_edge(
            &conn,
            &NewEdge {
                source_id: telos.id.clone(),
                target_id: action.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let result = reconcile(&conn).unwrap();
        assert_eq!(result["status"], "completed");
    }
}
