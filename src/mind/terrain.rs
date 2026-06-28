//! TDG Terrain — skill discovery and terrain context generation
//!
//! Port of `core/mind/terrain.py` (279 lines).
//! Provides domain-agnostic graph snapshot for mind injection.

use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::db::crud;
use crate::error::TdgResult;
use crate::models::NodeQuery;

/// Discover skills connected to the most densely populated node types.
///
/// Python: `discover_skills_for_terrain()` — finds top 3 dense node types,
/// then SkillNodes connected via ENABLES edges (fallback PACKAGES/REQUIRES/INCLUDES).
pub fn discover_skills_for_terrain(conn: &Connection) -> TdgResult<Vec<String>> {
    let type_counts = count_active_nodes_by_type(conn)?;

    let mut sorted: Vec<_> = type_counts.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));

    let top_types: Vec<&String> = sorted.iter().take(3).map(|(k, _)| *k).collect();

    let mut skills = Vec::new();
    for ntype in &top_types {
        // Find skills connected via ENABLES edges to nodes of this type
        let skill_edges: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT e.source_id
                 FROM edges e
                 JOIN nodes n ON e.target_id = n.id
                 WHERE e.edge_type = 'ENABLES'
                   AND e.valid_to IS NULL
                   AND n.node_type = ?1
                   AND n.valid_to IS NULL",
            )?;
            let rows = stmt.query_map([ntype.as_str()], |row| row.get::<_, String>(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };

        for source_id in &skill_edges {
            if let Ok(Some(node)) = crud::get_node(conn, source_id) {
                if node.node_type == "skill" && !skills.contains(&node.name) {
                    skills.push(node.name);
                }
            }
        }

        if !skills.is_empty() {
            break;
        }

        // Fallback: PACKAGES edges to nodes of this type
        let fallback_edges: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT e.source_id
                 FROM edges e
                 JOIN nodes n ON e.target_id = n.id
                 WHERE e.edge_type = 'PACKAGES'
                   AND e.valid_to IS NULL
                   AND n.node_type = ?1
                   AND n.valid_to IS NULL",
            )?;
            let rows = stmt.query_map([ntype.as_str()], |row| row.get::<_, String>(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };

        for source_id in &fallback_edges {
            if let Ok(Some(node)) = crud::get_node(conn, source_id) {
                if node.node_type == "skill" && !skills.contains(&node.name) {
                    skills.push(node.name);
                }
            }
        }

        if !skills.is_empty() {
            break;
        }
    }

    skills.truncate(10);
    Ok(skills)
}

pub fn count_active_nodes_by_type(conn: &Connection) -> TdgResult<HashMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT node_type, COUNT(*) as cnt FROM nodes
         WHERE lifecycle_state != 'archived'
         GROUP BY node_type ORDER BY cnt DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    let mut counts = HashMap::new();
    for row in rows {
        let (ntype, cnt) = row?;
        counts.insert(ntype, cnt);
    }
    Ok(counts)
}

/// Generate terrain context — domain-agnostic graph snapshot.
///
/// Python: `generate_terrain_context(loop_state, metrics)` — returns active_nodes_by_type,
/// highest_relevance_items, stale_items, open_edges, cycle_context.
pub fn generate_terrain_context(conn: &Connection, loop_state: &Value) -> TdgResult<Value> {
    let active_nodes_by_type = count_active_nodes_by_type(conn)?;

    let recent_nodes = crud::query_nodes(
        conn,
        &NodeQuery {
            node_type: None,
            lifecycle_state: None,
            source: None,
            teleological_level: None,
            developmental_stage: None,
            quadrant: None,
            agent_id: None,
            include_deleted: false,
            limit: Some(10),
            offset: None,
        },
    )?;

    let highest_relevance_items: Vec<String> = recent_nodes
        .iter()
        .map(|n| {
            format!(
                "{} [{}] (confidence: {:.0}%)",
                n.name,
                n.node_type,
                n.confidence * 100.0
            )
        })
        .collect();

    let stale_nodes = crud::query_nodes(
        conn,
        &NodeQuery {
            node_type: None,
            lifecycle_state: Some("stale".to_string()),
            source: None,
            teleological_level: None,
            developmental_stage: None,
            quadrant: None,
            agent_id: None,
            include_deleted: false,
            limit: Some(5),
            offset: None,
        },
    )?;

    let stale_items: Vec<String> = stale_nodes
        .iter()
        .map(|n| format!("{} [{}] — stale, needs attention", n.name, n.node_type))
        .collect();

    let open_edges = find_open_edges(conn)?;

    let cycle_num = loop_state
        .get("cycle_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let last_action = loop_state
        .get("last_action")
        .and_then(|v| v.as_str())
        .unwrap_or("none");

    let cycle_context = format!(
        "Cycle {} | Last action: {}",
        cycle_num,
        truncate_str(last_action, 80)
    );

    Ok(json!({
        "active_nodes_by_type": active_nodes_by_type,
        "highest_relevance_items": highest_relevance_items,
        "stale_items": stale_items,
        "open_edges": open_edges,
        "cycle_context": cycle_context,
    }))
}

fn find_open_edges(conn: &Connection) -> TdgResult<Vec<String>> {
    let edges = crud::get_edges(conn, None, None, Some("BLOCKS"), None, 5)?;
    let mut result = Vec::new();
    for edge in &edges {
        if let Ok(Some(target)) = crud::get_node(conn, &edge.target_id) {
            result.push(format!(
                "BLOCKS: {} → {}",
                edge.source_id,
                truncate_str(&target.name, 40)
            ));
        }
    }
    result.truncate(5);
    Ok(result)
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_schema, run_migrations};
    use crate::models::NewNode;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn discover_skills_empty_graph() {
        let conn = setup();
        let skills = discover_skills_for_terrain(&conn).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn count_active_nodes_by_type_empty() {
        let conn = setup();
        let counts = count_active_nodes_by_type(&conn).unwrap();
        assert!(counts.is_empty());
    }

    #[test]
    fn generate_terrain_context_empty() {
        let conn = setup();
        let loop_state = json!({"cycle_count": 1, "last_action": "test"});
        let ctx = generate_terrain_context(&conn, &loop_state).unwrap();
        assert!(ctx.get("active_nodes_by_type").is_some());
        assert!(ctx.get("highest_relevance_items").is_some());
        assert!(ctx.get("cycle_context").is_some());
    }

    #[test]
    fn discover_skills_with_skill_node() {
        let conn = setup();
        let obs = crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "obs-1".to_string(),
                description: None,
                properties: None,
                quadrants: None,
                drives: None,
                lifecycle_state: None,
                teleological_level: None,
                developmental_stage: None,
                confidence: None,
                source: None,
                parent_ids: None,
                agent_id: None,
            },
        )
        .unwrap();
        let skill = crud::add_node(
            &conn,
            &NewNode {
                node_type: "skill".to_string(),
                name: "test-skill".to_string(),
                description: None,
                properties: None,
                quadrants: None,
                drives: None,
                lifecycle_state: None,
                teleological_level: None,
                developmental_stage: None,
                confidence: None,
                source: None,
                parent_ids: None,
                agent_id: None,
            },
        )
        .unwrap();
        crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: skill.id.clone(),
                target_id: obs.id.clone(),
                edge_type: "ENABLES".to_string(),
                weight: None,
                properties: None,
                agent_id: None,
            },
        )
        .unwrap();
        let skills = discover_skills_for_terrain(&conn).unwrap();
        assert!(skills.contains(&"test-skill".to_string()));
    }
}
