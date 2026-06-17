//! Scripts & Utilities — Phase 12
//!
//! Port of `scripts/*.py` CLI tools.

use rusqlite::Connection;
use serde_json::{json, Value};
use crate::error::TdgResult;
use crate::models::NewNode;

/// Audit graph integration: orphan detection, health scores, archival.
pub fn audit(conn: &Connection) -> TdgResult<Value> {
    let orphans = crate::knowledge::detect_orphans(conn)?;
    let hygiene = crate::knowledge::generate_hygiene_report(conn)?;
    let stale = crate::knowledge::archive_stale_nodes(conn, None)?;

    let orphan_count = orphans.get("disconnected")
        .and_then(|v| v.as_array())
        .map_or(0, |a| a.len());
    let unlinked = orphans.get("unlinked_observations")
        .and_then(|v| v.as_array())
        .map_or(0, |a| a.len());

    Ok(json!({
        "audit": "completed",
        "orphans": orphan_count,
        "unlinked_observations": unlinked,
        "stale_archived": stale.get("archived_count").and_then(|v| v.as_i64()).unwrap_or(0),
        "health": {
            "total_nodes": hygiene.total_nodes,
            "total_edges": hygiene.total_edges,
            "orphan_count": hygiene.orphan_count,
            "dangling_edge_count": hygiene.dangling_edge_count,
        },
        "recommendations": hygiene.recommendations,
    }))
}

/// Check constraint vitality: count constraints, BLOCKS edges, ghost nodes.
pub fn check(conn: &Connection) -> TdgResult<Value> {
    let constraints: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL AND node_type = 'constraint'",
        [], |r| r.get(0),
    ).unwrap_or(0);

    let blocks_edges: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges WHERE valid_to IS NULL AND edge_type = 'BLOCKS'",
        [], |r| r.get(0),
    ).unwrap_or(0);

    let active_nodes: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL AND lifecycle_state = 'active'",
        [], |r| r.get(0),
    ).unwrap_or(0);

    // Ghost nodes: active nodes with no quadrant data
    let ghost_nodes: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL AND lifecycle_state = 'active' AND (quadrants_json = '{}' OR quadrants_json IS NULL)",
        [], |r| r.get(0),
    ).unwrap_or(0);

    let warnings = if blocks_edges == 0 && active_nodes > 0 {
        vec!["No BLOCKS edges found — constraints may not be enforced".to_string()]
    } else if ghost_nodes > 0 {
        vec![format!("{} ghost nodes detected (no quadrant data)", ghost_nodes)]
    } else {
        vec![]
    };

    Ok(json!({
        "constraints": constraints,
        "blocks_edges": blocks_edges,
        "active_nodes": active_nodes,
        "ghost_nodes": ghost_nodes,
        "warnings": warnings,
    }))
}

/// Auto-capture observation from description.
pub fn auto_capture(
    conn: &Connection,
    description: &str,
    quadrant: &str,
    trust: f64,
    entities: Option<&str>,
) -> TdgResult<Value> {
    let props = json!({
        "quadrant": quadrant,
        "trust": trust,
        "entities": entities.unwrap_or(""),
        "source": "auto_capture",
    });

    let node = crate::db::crud::add_node(conn, &NewNode {
        node_type: "observation".to_string(),
        name: format!("Obs: {}", &description[..description.len().min(80)]),
        description: Some(description.to_string()),
        source: Some("auto_capture".to_string()),
        properties: Some(props),
        ..Default::default()
    })?;

    // Record the event
    crate::db::crud::record_event(
        conn,
        "auto_capture",
        Some(&node.id),
        None,
        None,
        Some(&json!({
            "quadrant": quadrant,
            "trust": trust,
            "description": description,
        })),
    )?;

    Ok(json!({
        "observation_id": node.id,
        "quadrant": quadrant,
        "trust": trust,
        "recorded": true,
    }))
}

/// Create a node from CLI arguments.
pub fn create_node(
    conn: &Connection,
    node_type: &str,
    name: &str,
    description: Option<&str>,
) -> TdgResult<Value> {
    let node = crate::db::crud::add_node(conn, &NewNode {
        node_type: node_type.to_string(),
        name: name.to_string(),
        description: description.map(|s| s.to_string()),
        source: Some("cli".to_string()),
        ..Default::default()
    })?;

    Ok(json!({
        "id": node.id,
        "node_type": node.node_type,
        "name": node.name,
        "created_at": node.created_at,
    }))
}

/// Maintenance check: detect and report orphan and stale nodes.
pub fn maintenance_check(conn: &Connection) -> TdgResult<Value> {
    let _orphans = crate::knowledge::detect_orphans(conn)?;
    let dangling = crate::knowledge::prune_dangling_edges(conn)?;
    let report = crate::knowledge::generate_hygiene_report(conn)?;

    Ok(json!({
        "orphan_count": report.orphan_count,
        "dangling_pruned": dangling.get("pruned_count").and_then(|v| v.as_i64()).unwrap_or(0),
        "stale_count": report.stale_count,
        "recommendations": report.recommendations,
    }))
}

/// Unify persistence: reconcile events table with node/edge state.
pub fn unify(conn: &Connection) -> TdgResult<Value> {
    let total_events: i64 = conn.query_row(
        "SELECT COUNT(*) FROM events", [], |r| r.get(0),
    ).unwrap_or(0);

    let orphan_events: i64 = conn.query_row(
        "SELECT COUNT(*) FROM events e
         LEFT JOIN nodes n ON e.node_id = n.id
         WHERE e.node_id IS NOT NULL AND n.id IS NULL",
        [], |r| r.get(0),
    ).unwrap_or(0);

    let duplicate_edges: i64 = conn.query_row(
        "SELECT COUNT(*) FROM (
            SELECT source_id, target_id, edge_type, COUNT(*) as cnt
            FROM edges WHERE valid_to IS NULL
            GROUP BY source_id, target_id, edge_type
            HAVING cnt > 1
         )",
        [], |r| r.get(0),
    ).unwrap_or(0);

    // Fix lifecycle_state inconsistencies
    let fixed_lifecycle: i64 = conn.execute(
        "UPDATE nodes SET lifecycle_state = 'active'
         WHERE valid_to IS NULL AND lifecycle_state NOT IN
         ('active', 'archived', 'discarded', 'draft', 'completed')",
        [],
    ).unwrap_or(0) as i64;

    Ok(json!({
        "total_events": total_events,
        "orphan_events": orphan_events,
        "duplicate_edge_groups": duplicate_edges,
        "fixed_lifecycle_states": fixed_lifecycle,
        "unified": true,
    }))
}

/// Reconcile constraints: dedup constraints and repair broken BLOCKS edges.
pub fn reconcile_constraints(conn: &Connection) -> TdgResult<Value> {
    // Fetch all constraint names and IDs, then dedup in memory
    let all_constraints: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, name, created_at FROM nodes
             WHERE valid_to IS NULL AND node_type = 'constraint'
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // Group by name to find duplicates
    use std::collections::HashMap;
    let mut by_name: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (id, name, created_at) in all_constraints {
        by_name.entry(name).or_default().push((id, created_at));
    }

    let mut deduped = 0i64;
    let mut dup_count = 0i64;
    let now = crate::db::crud::now_iso();
    for (_name, mut entries) in by_name {
        if entries.len() <= 1 {
            continue;
        }
        dup_count += 1;
        // Keep the oldest (first, since we ordered by created_at ASC), archive the rest
        for (id, _created_at) in entries.drain(1..) {
            conn.execute(
                "UPDATE nodes SET lifecycle_state = 'archived', updated_at = ?1 WHERE id = ?2",
                rusqlite::params![now, id],
            )?;
            deduped += 1;
        }
    }

    // Repair BLOCKS edges with dangling targets
    let dangling_blocks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges e
         LEFT JOIN nodes n ON e.target_id = n.id
         WHERE e.edge_type = 'BLOCKS' AND e.valid_to IS NULL AND n.id IS NULL",
        [], |r| r.get(0),
    ).unwrap_or(0);

    if dangling_blocks > 0 {
        let now = crate::db::crud::now_iso();
        conn.execute(
            "UPDATE edges SET valid_to = ?1
             WHERE edge_type = 'BLOCKS' AND valid_to IS NULL
             AND target_id NOT IN (SELECT id FROM nodes WHERE valid_to IS NULL)",
            rusqlite::params![now],
        )?;
    }

    Ok(json!({
        "duplicate_groups_found": dup_count,
        "constraints_deduped": deduped,
        "dangling_blocks_repaired": dangling_blocks,
    }))
}

/// Sync skills directory to graph: parse skill YAML/JSON and create/update nodes.
pub fn sync_skills(conn: &Connection, dir: &str) -> TdgResult<Value> {
    use std::path::Path;

    let skills_dir = Path::new(dir);
    if !skills_dir.exists() {
        return Ok(json!({"error": format!("Directory not found: {dir}"), "synced": 0}));
    }

    let mut synced = 0i64;
    let mut skipped = 0i64;
    let mut errors = Vec::new();

    if let Ok(entries) = std::fs::read_dir(skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "json" && ext != "yaml" && ext != "yml" {
                    skipped += 1;
                    continue;
                }

                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        let parsed = if ext == "json" {
                            serde_json::from_str::<Value>(&content)
                        } else {
                            // Simple YAML-like parsing: extract name and description
                            let name = content.lines()
                                .find(|l| l.starts_with("name:") || l.starts_with("title:"))
                                .map(|l| l.splitn(2, ':').nth(1).unwrap_or("").trim().trim_matches('"').trim_matches('\'').to_string());
                            let description = content.lines()
                                .find(|l| l.starts_with("description:") || l.starts_with("desc:"))
                                .map(|l| l.splitn(2, ':').nth(1).unwrap_or("").trim().trim_matches('"').trim_matches('\'').to_string());

                            match name {
                                Some(n) => Ok(json!({"name": n, "description": description.unwrap_or_default(), "source_file": path.display().to_string()})),
                                None => Err(serde_json::Error::io(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData, "No name field found",
                                ))),
                            }
                        };

                        match parsed {
                            Ok(skill) => {
                                let skill_name = skill.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                                let skill_desc = skill.get("description").and_then(|v| v.as_str()).unwrap_or("");
                                let source_file = skill.get("source_file").and_then(|v| v.as_str()).unwrap_or("");

                                // Check if skill node already exists
                                let existing: Option<String> = conn.query_row(
                                    "SELECT id FROM nodes WHERE valid_to IS NULL
                                     AND node_type = 'skill' AND name = ?1",
                                    rusqlite::params![skill_name], |r| r.get(0),
                                ).ok();

                                if let Some(_existing_id) = existing {
                                    skipped += 1;
                                } else {
                                    crate::db::crud::add_node(
                                        conn,
                                        &NewNode {
                                            node_type: "skill".to_string(),
                                            name: skill_name.to_string(),
                                            description: Some(skill_desc.to_string()),
                                            source: Some(format!("sync_skills:{source_file}")),
                                            properties: Some(skill.clone()),
                                            ..Default::default()
                                        },
                                    )?;
                                    synced += 1;
                                }
                            }
                            Err(e) => {
                                errors.push(format!("{}: {e}", path.display()));
                            }
                        }
                    }
                    Err(e) => {
                        errors.push(format!("{}: {e}", path.display()));
                    }
                }
            }
        }
    }

    Ok(json!({
        "synced": synced,
        "skipped": skipped,
        "errors": errors,
        "directory": dir,
    }))
}

/// Repair orphan nodes: attempt to link to structural nodes or archive critical ones.
pub fn repair_orphans(conn: &Connection) -> TdgResult<Value> {
    let orphans = crate::knowledge::detect_orphans(conn)?;
    let disconnected = orphans.get("disconnected")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut archived = 0;

    for orphan in &disconnected {
        let severity = orphan.get("severity").and_then(|v| v.as_str()).unwrap_or("");
        let node_id = orphan.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
        let age_days = orphan.get("age_days").and_then(|v| v.as_i64()).unwrap_or(0);

        if severity == "critical" || age_days > 60 {
            // Archive critical orphan
            let now = crate::db::crud::now_iso();
            conn.execute(
                "UPDATE nodes SET lifecycle_state = 'archived', updated_at = ?1 WHERE id = ?2",
                rusqlite::params![now, node_id],
            )?;
            archived += 1;
        }
    }

    // Enforce lifecycle (archive critical orphan observations)
    let enforced = crate::knowledge::enforce_observation_lifecycle(conn)?;

    Ok(json!({
        "total_orphans": disconnected.len(),
        "archived": archived,
        "lifecycle_enforced": enforced.get("enforced_count").and_then(|v| v.as_i64()).unwrap_or(0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_fts, init_schema, run_migrations};
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn audit_empty_graph() {
        let conn = setup_db();
        let result = audit(&conn).unwrap();
        assert_eq!(result["audit"], "completed");
        assert_eq!(result["health"]["total_nodes"], 0);
    }

    #[test]
    fn check_empty_graph() {
        let conn = setup_db();
        let result = check(&conn).unwrap();
        assert_eq!(result["constraints"], 0);
        assert_eq!(result["active_nodes"], 0);
    }

    #[test]
    fn check_with_constraints() {
        let conn = setup_db();
        crate::db::crud::add_node(&conn, &NewNode {
            node_type: "constraint".to_string(), name: "C1".to_string(), ..Default::default()
        }).unwrap();
        let result = check(&conn).unwrap();
        assert_eq!(result["constraints"], 1);
    }

    #[test]
    fn auto_capture_basic() {
        let conn = setup_db();
        let result = auto_capture(&conn, "Test observation", "LR", 0.8, None).unwrap();
        assert!(result["observation_id"].as_str().unwrap().starts_with('n'));
        assert_eq!(result["quadrant"], "LR");
    }

    #[test]
    fn create_node_basic() {
        let conn = setup_db();
        let result = create_node(&conn, "action", "Test Action", Some("desc")).unwrap();
        assert!(result["id"].as_str().unwrap().starts_with('n'));
        assert_eq!(result["node_type"], "action");
    }

    #[test]
    fn maintenance_check_basic() {
        let conn = setup_db();
        let result = maintenance_check(&conn).unwrap();
        assert_eq!(result["orphan_count"], 0);
    }

    #[test]
    fn repair_orphans_empty() {
        let conn = setup_db();
        let result = repair_orphans(&conn).unwrap();
        assert_eq!(result["total_orphans"], 0);
    }
}
