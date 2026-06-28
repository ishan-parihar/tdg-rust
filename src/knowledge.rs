//! TDG Knowledge Hygiene Engine
//!
//! Graph hygiene: orphan detection, dangling edge pruning,
//! stale node archival, and hygiene reporting.

use std::collections::HashMap;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db::crud::{now_iso, record_event};
use crate::error::TdgResult;

// ─── Constants ───────────────────────────────────────────────────────────────

pub const DEFAULT_ORPHAN_THRESHOLD_DAYS: i64 = 30;
pub const DEFAULT_STALE_THRESHOLD_DAYS: i64 = 60;
pub const DEFAULT_INTEGRATION_DECAY_DAYS: i64 = 14;

/// Structural node types that catalysts can link to.
pub const STRUCTURAL_TARGET_TYPES: &[&str] = &["hypothesis", "constraint", "telos"];

/// Edge types used for catalyst linkage validation.
pub const CATALYST_LINK_EDGES: &[&str] = &[
    "EVIDENCES",
    "SUPPORTS",
    "CONTEXT",
    "DIGESTS_TO",
    "RELATES_TO",
];

// ─── Data Models ─────────────────────────────────────────────────────────────

/// Hygiene report for the entire graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HygieneReport {
    pub total_nodes: i64,
    pub total_edges: i64,
    pub total_observations: i64,
    pub orphan_count: i64,
    pub orphan_ids: Vec<String>,
    pub stale_count: i64,
    pub stale_ids: Vec<String>,
    pub dangling_edge_count: i64,
    pub dangling_edge_ids: Vec<String>,
    pub recently_archived: i64,
    pub lifecycle_distribution: HashMap<String, i64>,
    pub recommendations: Vec<String>,
}

// ─── Graph Hygiene ───────────────────────────────────────────────────────────

/// Detect orphan nodes: nodes with no active edges.
pub fn detect_orphans(conn: &Connection) -> TdgResult<serde_json::Value> {
    let mut stmt =
        conn.prepare("SELECT id, node_type, name, created_at FROM nodes WHERE valid_to IS NULL")?;

    let rows: Vec<(String, String, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut disconnected = Vec::new();
    let mut unlinked_observations = Vec::new();

    for (id, node_type, name, created_at) in &rows {
        // Count edges (both directions)
        let edge_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE (source_id = ?1 OR target_id = ?1) AND valid_to IS NULL",
            params![id],
            |row| row.get(0),
        )?;

        if edge_count == 0 {
            let age_days = chrono::NaiveDateTime::parse_from_str(
                created_at.replace('Z', "").as_str(),
                "%Y-%m-%dT%H:%M:%S%.f",
            )
            .ok()
            .map(|created| {
                let now = chrono::Utc::now().naive_utc();
                (now - created).num_days()
            })
            .unwrap_or(0);

            let severity = if age_days > DEFAULT_ORPHAN_THRESHOLD_DAYS {
                "critical"
            } else {
                "warning"
            };

            disconnected.push(serde_json::json!({
                "node_id": id,
                "node_type": node_type,
                "name": name,
                "edge_count": edge_count,
                "age_days": age_days,
                "severity": severity,
            }));
        } else if node_type == "observation" {
            // Check for structural links
            let structural_links: i64 = conn.query_row(
                "SELECT COUNT(*) FROM edges e JOIN nodes n ON e.target_id = n.id
                 WHERE e.source_id = ?1 AND e.valid_to IS NULL
                 AND n.node_type IN ('hypothesis', 'constraint', 'telos')",
                params![id],
                |row| row.get(0),
            )?;

            if structural_links == 0 {
                unlinked_observations.push(serde_json::json!({
                    "node_id": id,
                    "name": name,
                    "total_edges": edge_count,
                    "structural_links": 0,
                }));
            }
        }
    }

    Ok(serde_json::json!({
        "disconnected": disconnected,
        "unlinked_observations": unlinked_observations,
        "total_disconnected": disconnected.len(),
        "total_unlinked_observations": unlinked_observations.len(),
    }))
}

/// Prune edges that point to non-existent (hard-deleted) nodes.
pub fn prune_dangling_edges(conn: &Connection) -> TdgResult<serde_json::Value> {
    let mut pruned = 0i64;
    let mut pruned_ids = Vec::new();

    // Find edges with dangling source_id
    let mut stmt = conn.prepare(
        "SELECT e.id FROM edges e LEFT JOIN nodes n ON e.source_id = n.id
         WHERE n.id IS NULL AND e.valid_to IS NULL",
    )?;
    let dangling_source: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for eid in dangling_source {
        let now = now_iso();
        conn.execute(
            "UPDATE edges SET valid_to = ?1 WHERE id = ?2",
            params![now, eid],
        )?;
        pruned += 1;
        pruned_ids.push(eid);
    }

    // Find edges with dangling target_id
    let mut stmt = conn.prepare(
        "SELECT e.id FROM edges e LEFT JOIN nodes n ON e.target_id = n.id
         WHERE n.id IS NULL AND e.valid_to IS NULL",
    )?;
    let dangling_target: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for eid in dangling_target {
        let now = now_iso();
        conn.execute(
            "UPDATE edges SET valid_to = ?1 WHERE id = ?2",
            params![now, eid],
        )?;
        pruned += 1;
        pruned_ids.push(eid);
    }

    if pruned > 0 {
        record_event(
            conn,
            "dangling_edges_pruned",
            None,
            None,
            None,
            Some(&serde_json::json!({
                "pruned_count": pruned,
                "edge_ids": pruned_ids,
            })),
        )?;
    }

    Ok(serde_json::json!({
        "pruned_count": pruned,
        "edge_ids": pruned_ids,
    }))
}

/// Archive stale nodes that have passed their archive_after deadline.
pub fn archive_stale_nodes(
    conn: &Connection,
    days_threshold: Option<i64>,
) -> TdgResult<serde_json::Value> {
    let threshold = days_threshold.unwrap_or(DEFAULT_STALE_THRESHOLD_DAYS);
    let mut archived = 0i64;
    let mut archived_ids = Vec::new();

    let mut stmt = conn.prepare(
        "SELECT id, name, node_type, properties_json, parent_ids, created_at
         FROM nodes WHERE valid_to IS NULL AND node_type = 'observation'",
    )?;

    let rows: Vec<(String, String, String, String, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let now = chrono::Utc::now().naive_utc();

    for (id, name, node_type, properties_json, _parent_ids_json, created_at) in &rows {
        let properties: serde_json::Value =
            serde_json::from_str(properties_json).unwrap_or(serde_json::json!({}));

        let should_archive =
            if let Some(archive_after) = properties.get("archive_after").and_then(|v| v.as_str()) {
                if let Ok(aa) = chrono::NaiveDateTime::parse_from_str(
                    archive_after.replace('Z', "").as_str(),
                    "%Y-%m-%dT%H:%M:%S%.f",
                ) {
                    now > aa
                } else {
                    false
                }
            } else {
                // Check age-based staleness
                if let Ok(created) = chrono::NaiveDateTime::parse_from_str(
                    created_at.replace('Z', "").as_str(),
                    "%Y-%m-%dT%H:%M:%S%.f",
                ) {
                    let age_days = (now - created).num_days();
                    age_days > threshold
                } else {
                    false
                }
            };

        if should_archive {
            // Soft-archived: update lifecycle_state
            let now_str = now_iso();
            conn.execute(
                "UPDATE nodes SET lifecycle_state = 'archived', updated_at = ?1 WHERE id = ?2",
                params![now_str, id],
            )?;

            record_event(
                conn,
                "node_archived",
                Some(id),
                None,
                None,
                Some(&serde_json::json!({
                    "name": name,
                    "node_type": node_type,
                    "reason": "stale_or_expired",
                })),
            )?;

            archived += 1;
            archived_ids.push(id.clone());
        }
    }

    Ok(serde_json::json!({
        "archived_count": archived,
        "archived_ids": archived_ids,
    }))
}

/// Enforce observation lifecycle: archive critical orphan observations.
pub fn enforce_observation_lifecycle(conn: &Connection) -> TdgResult<serde_json::Value> {
    let orphans = detect_orphans(conn)?;
    let disconnected = orphans
        .get("disconnected")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut enforced = 0i64;
    let mut enforced_ids = Vec::new();

    for orphan in &disconnected {
        let severity = orphan
            .get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let node_id = orphan.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
        let node_type = orphan
            .get("node_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if severity == "critical" && node_type == "observation" {
            // Archive this critical orphan
            let now = now_iso();
            conn.execute(
                "UPDATE nodes SET lifecycle_state = 'archived', updated_at = ?1 WHERE id = ?2",
                params![now, node_id],
            )?;

            record_event(
                conn,
                "observation_archived_lifecycle",
                Some(node_id),
                None,
                None,
                Some(&serde_json::json!({
                    "reason": "critical_orphan",
                })),
            )?;

            enforced += 1;
            enforced_ids.push(node_id.to_string());
        }
    }

    Ok(serde_json::json!({
        "enforced_count": enforced,
        "enforced_ids": enforced_ids,
    }))
}

/// Generate a comprehensive hygiene report.
pub fn generate_hygiene_report(conn: &Connection) -> TdgResult<HygieneReport> {
    // Basic counts
    let total_nodes: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL",
        [],
        |row| row.get(0),
    )?;
    let total_edges: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges WHERE valid_to IS NULL",
        [],
        |row| row.get(0),
    )?;
    let total_observations: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL AND node_type = 'observation'",
        [],
        |row| row.get(0),
    )?;

    // Orphans
    let orphans = detect_orphans(conn)?;
    let orphan_ids: Vec<String> = orphans
        .get("disconnected")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("node_id")?.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Dangling edges
    let mut stmt = conn.prepare(
        "SELECT e.id FROM edges e
         LEFT JOIN nodes ns ON e.source_id = ns.id
         LEFT JOIN nodes nt ON e.target_id = nt.id
         WHERE (ns.id IS NULL OR nt.id IS NULL) AND e.valid_to IS NULL",
    )?;
    let dangling_edge_ids: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    // Stale nodes
    let stale_result = archive_stale_nodes(conn, Some(DEFAULT_STALE_THRESHOLD_DAYS))?;
    let stale_ids: Vec<String> = stale_result
        .get("archived_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Recently archived
    let recently_archived: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL AND lifecycle_state = 'archived'",
        [],
        |row| row.get(0),
    )?;

    // Lifecycle distribution
    let mut stmt = conn.prepare(
        "SELECT lifecycle_state, COUNT(*) FROM nodes WHERE valid_to IS NULL GROUP BY lifecycle_state",
    )?;
    let lifecycle_distribution: HashMap<String, i64> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Recommendations
    let mut recommendations = Vec::new();
    if orphan_ids.len() > 5 {
        recommendations.push(format!(
            "Consider removing {} disconnected nodes or linking them to the graph",
            orphan_ids.len()
        ));
    }
    if !dangling_edge_ids.is_empty() {
        recommendations.push(format!(
            "Prune {} dangling edges pointing to deleted nodes",
            dangling_edge_ids.len()
        ));
    }
    if total_observations > 0 && total_observations as f64 / total_nodes as f64 > 0.7 {
        recommendations.push(
            "High ratio of observations — consider converting some to insights or hypotheses"
                .to_string(),
        );
    }

    Ok(HygieneReport {
        total_nodes,
        total_edges,
        total_observations,
        orphan_count: orphan_ids.len() as i64,
        orphan_ids,
        stale_count: stale_ids.len() as i64,
        stale_ids,
        dangling_edge_count: dangling_edge_ids.len() as i64,
        dangling_edge_ids,
        recently_archived,
        lifecycle_distribution,
        recommendations,
    })
}

/// Combined hygiene pipeline: prune dangling → archive stale → enforce lifecycle → report.
pub fn run_full_hygiene_cycle(conn: &Connection, lean: bool) -> TdgResult<HygieneReport> {
    let pruned = prune_dangling_edges(conn)?;
    let pruned_count = pruned
        .get("pruned_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let archived = if lean {
        serde_json::json!({"archived_count": 0, "archived_ids": []})
    } else {
        archive_stale_nodes(conn, None)?
    };
    let archived_count = archived
        .get("archived_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let enforced = if lean {
        serde_json::json!({"enforced_count": 0, "enforced_ids": []})
    } else {
        enforce_observation_lifecycle(conn)?
    };
    let enforced_count = enforced
        .get("enforced_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let mut report = generate_hygiene_report(conn)?;

    report.recommendations.insert(0, format!(
        "Hygiene cycle complete: {pruned_count} dangling edges pruned, {archived_count} stale nodes archived, {enforced_count} critical orphans enforced"
    ));

    record_event(
        conn,
        "hygiene_cycle_complete",
        None,
        None,
        None,
        Some(&serde_json::json!({
            "pruned": pruned_count,
            "archived": archived_count,
            "enforced": enforced_count,
            "lean": lean,
        })),
    )?;

    Ok(report)
}

// ─── Catalyst Lifecycle ──────────────────────────────────────────────────────

/// Classify an observation as a catalyst (incoming signal that may drive change).
pub fn classify_catalyst(conn: &Connection, node_id: &str) -> TdgResult<serde_json::Value> {
    let node_type: String = conn.query_row(
        "SELECT node_type FROM nodes WHERE id = ?1 AND valid_to IS NULL",
        params![node_id],
        |row| row.get(0),
    )?;

    if node_type != "observation" {
        return Ok(serde_json::json!({
            "status": "skipped",
            "reason": "not_observation",
            "node_type": node_type,
        }));
    }

    let now = now_iso();
    conn.execute(
        "UPDATE nodes SET lifecycle_state = 'classified', updated_at = ?1 WHERE id = ?2",
        params![now, node_id],
    )?;

    record_event(
        conn,
        "catalyst_classified",
        Some(node_id),
        None,
        None,
        Some(&serde_json::json!({"classification": "catalyst"})),
    )?;

    Ok(serde_json::json!({
        "status": "classified",
        "node_id": node_id,
    }))
}

/// Link a catalyst observation to structural nodes (hypotheses, constraints, teloi).
pub fn link_catalyst_to_structure(conn: &Connection, node_id: &str) -> TdgResult<serde_json::Value> {
    let mut stmt = conn.prepare(
        "SELECT n.id, n.node_type, n.name FROM edges e
         JOIN nodes n ON e.target_id = n.id
         WHERE e.source_id = ?1 AND e.valid_to IS NULL AND n.valid_to IS NULL
         AND n.node_type IN ('hypothesis', 'constraint', 'telos')",
    )?;

    let linked: Vec<serde_json::Value> = stmt
        .query_map(params![node_id], |row| {
            Ok(serde_json::json!({
                "node_id": row.get::<_, String>(0)?,
                "node_type": row.get::<_, String>(1)?,
                "name": row.get::<_, String>(2)?,
            }))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let hypotheses: Vec<String> = linked
        .iter()
        .filter(|l| l["node_type"] == "hypothesis")
        .filter_map(|l| l["node_id"].as_str().map(|s| s.to_string()))
        .collect();

    Ok(serde_json::json!({
        "status": "linked",
        "node_id": node_id,
        "linked_count": linked.len(),
        "hypotheses": hypotheses,
        "links": linked,
    }))
}

/// Evaluate integration quality: how well a catalyst is connected to the graph.
pub fn evaluate_integration_quality(conn: &Connection, node_id: &str) -> TdgResult<serde_json::Value> {
    let edge_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges WHERE (source_id = ?1 OR target_id = ?1) AND valid_to IS NULL",
        params![node_id],
        |row| row.get(0),
    )?;

    let structural_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges e JOIN nodes n ON e.target_id = n.id
         WHERE e.source_id = ?1 AND e.valid_to IS NULL
         AND n.node_type IN ('hypothesis', 'constraint', 'telos')",
        params![node_id],
        |row| row.get(0),
    )?;

    let quality = if edge_count == 0 {
        0.0
    } else {
        (structural_count as f64 / edge_count as f64).min(1.0)
    };

    Ok(serde_json::json!({
        "integration_quality": quality,
        "total_edges": edge_count,
        "structural_edges": structural_count,
    }))
}

/// Process full catalyst lifecycle: classify → link → evaluate → mark complete.
pub fn process_catalyst_lifecycle(conn: &Connection, node_id: &str) -> TdgResult<serde_json::Value> {
    let classified = classify_catalyst(conn, node_id)?;
    if classified["status"] != "classified" {
        return Ok(serde_json::json!({
            "status": "skipped",
            "reason": classified.get("reason"),
        }));
    }

    let linked = link_catalyst_to_structure(conn, node_id)?;
    let evaluated = evaluate_integration_quality(conn, node_id)?;

    let now = now_iso();
    conn.execute(
        "UPDATE nodes SET lifecycle_state = 'lifecycle_complete', updated_at = ?1 WHERE id = ?2",
        params![now, node_id],
    )?;

    record_event(
        conn,
        "catalyst_lifecycle_complete",
        Some(node_id),
        None,
        None,
        Some(&serde_json::json!({
            "linked_count": linked["linked_count"],
            "integration_quality": evaluated["integration_quality"],
        })),
    )?;

    Ok(serde_json::json!({
        "status": "lifecycle_complete",
        "node_id": node_id,
        "linked_count": linked["linked_count"],
        "integration_quality": evaluated["integration_quality"],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_fts, init_schema, run_migrations};
    use crate::models::NewNode;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    fn add_observation(conn: &Connection, name: &str) -> crate::models::Node {
        crate::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: name.to_string(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn detect_orphans_basic() {
        let conn = setup_db();
        add_observation(&conn, "Orphan Node");
        add_observation(&conn, "Connected Node");

        let result = detect_orphans(&conn).unwrap();
        let disconnected = result["disconnected"].as_array().unwrap();
        assert!(!disconnected.is_empty());
    }

    #[test]
    fn prune_dangling_edges_basic() {
        let conn = setup_db();
        let obs = add_observation(&conn, "Source");
        let target = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "hypothesis".to_string(),
                name: "Target".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let _edge = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: obs.id.clone(),
                target_id: target.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Temporarily disable FK constraints to delete node while leaving edge dangling
        conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
        conn.execute("DELETE FROM nodes WHERE id = ?1", params![target.id])
            .unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();

        let result = prune_dangling_edges(&conn).unwrap();
        assert_eq!(result["pruned_count"], 1);
    }

    #[test]
    fn hygiene_report_basic() {
        let conn = setup_db();
        add_observation(&conn, "Node 1");
        add_observation(&conn, "Node 2");
        crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Telos".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let report = generate_hygiene_report(&conn).unwrap();
        assert_eq!(report.total_nodes, 3);
        assert!(!report.lifecycle_distribution.is_empty());
    }
}
