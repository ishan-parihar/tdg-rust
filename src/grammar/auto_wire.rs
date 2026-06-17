//! Auto-wire edges based on NODE_CONTRACT `auto_wire_on_parent` rules.
//!
//! Ported from Python `auto_wire.py` (170 lines).

use rusqlite::Connection;

use crate::db::crud;
use crate::error::TdgResult;
use crate::validation::node_contracts;

/// Direction mapping for auto-wired edge types.
///
/// Each entry maps an edge type to (source, target) role hints:
/// - `true` = edge goes FROM parent TO child (parent is source)
/// - `false` = edge goes FROM child TO parent (child is source)
///
/// Python `AUTO_WIRE_DIRECTION` equivalent.
pub fn auto_wire_direction() -> &'static [(&'static str, bool)] {
    &[
        ("DECOMPOSES_TO", true),  // parent DECOMPOSES_TO child
        ("ENABLES", false),       // child ENABLES parent (skill enables telos)
        ("PURSUES", false),       // child PURSUES parent
        ("CONTEXT", false),       // child provides CONTEXT for parent
        ("EVIDENCES", false),     // child EVIDENCES parent
        ("BLOCKS", false),        // child BLOCKS parent
        ("SENT", false),          // child SENT to parent
        ("RECEIVED", false),      // child RECEIVED from parent
        ("TRIGGERED", false),     // child TRIGGERED by parent
        ("DETECTED", false),      // child DETECTED by parent
        ("ILLUMINATES", false),   // child ILLUMINATES parent
        ("OPENS", false),         // child OPENS parent
        ("CREATES", false),       // child CREATES parent
        ("ADVANCES", false),      // child ADVANCES parent
        ("APPEALS_TO", false),    // child APPEALS_TO parent
        ("REPLIES", false),       // child REPLIES to parent
        ("CONTINUES", false),     // child CONTINUES parent
        ("HAS_CAPABILITY", false), // child HAS_CAPABILITY (people→skill)
        ("SYNTHESIZES", true),    // synthesis SYNTHESIZES observation
        ("SEEKS", false),         // child SEEKS parent
    ]
}

/// Get the direction for a given edge type.
/// Returns `true` if parent is source, `false` if child is source.
fn edge_direction(edge_type: &str) -> Option<bool> {
    auto_wire_direction()
        .iter()
        .find(|(et, _)| *et == edge_type)
        .map(|(_, dir)| *dir)
}

/// Auto-wire edges for a newly created node based on its type's contract rules.
///
/// For each edge type in the node's `auto_wire_on_parent` list, creates an edge
/// between the node and each parent. Direction is determined by `AUTO_WIRE_DIRECTION`.
///
/// # Arguments
/// * `conn` - SQLite connection
/// * `node_id` - ID of the newly created node
/// * `node_type` - Type of the newly created node
/// * `parent_ids` - IDs of parent nodes (from the node's parent_ids field)
///
/// # Returns
/// Number of edges created.
pub fn auto_wire_edges(
    conn: &Connection,
    node_id: &str,
    node_type: &str,
    parent_ids: &[String],
) -> TdgResult<usize> {
    let contracts = node_contracts();
    let contract = match contracts.get(node_type) {
        Some(c) => c,
        None => return Ok(0),
    };

    if contract.auto_wire_on_parent.is_empty() || parent_ids.is_empty() {
        return Ok(0);
    }

    let mut edges_created = 0;

    for edge_type in &contract.auto_wire_on_parent {
        let direction = edge_direction(edge_type).unwrap_or(true);

        for parent_id in parent_ids {
            // Skip if parent doesn't exist
            if crud::get_node(conn, parent_id)?.is_none() {
                continue;
            }

            // Skip if edge already exists
            let exists = edge_exists(conn, node_id, parent_id, edge_type)?
                || edge_exists(conn, parent_id, node_id, edge_type)?;

            if exists {
                continue;
            }

            let (source_id, target_id) = if direction {
                (parent_id.to_string(), node_id.to_string())
            } else {
                (node_id.to_string(), parent_id.to_string())
            };

            let new_edge = crate::models::NewEdge {
                source_id,
                target_id,
                edge_type: edge_type.to_string(),
                weight: None,
                properties: None,
                agent_id: Some("auto_wire".to_string()),
            };

            crud::add_edge(conn, &new_edge)?;
            edges_created += 1;
        }
    }

    Ok(edges_created)
}

/// Check if an edge already exists between two nodes with a given type.
fn edge_exists(conn: &Connection, source_id: &str, target_id: &str, edge_type: &str) -> TdgResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges WHERE source_id = ?1 AND target_id = ?2 AND edge_type = ?3",
        rusqlite::params![source_id, target_id, edge_type],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::db::init_schema;
    use crate::models::NewNode;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    fn create_test_node(conn: &Connection, node_type: &str, name: &str) -> crate::models::Node {
        let new = NewNode {
            node_type: node_type.to_string(),
            name: name.to_string(),
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
        };
        crud::add_node(conn, &new).unwrap()
    }

    #[test]
    fn test_auto_wire_observation_evidences_parent() {
        let conn = setup_db();
        let telos = create_test_node(&conn, "telos", "Test Telos");
        let obs = create_test_node(&conn, "observation", "Test Observation");

        // Auto-wire observation to telos via EVIDENCES
        let created = auto_wire_edges(
            &conn,
            &obs.id,
            "observation",
            &[telos.id.clone()],
        )
        .unwrap();

        assert_eq!(created, 1);

        // Verify edge exists: obs EVIDENCES telos
        let edges = crud::get_edges(&conn, Some(&obs.id), None, None, None, 1000).unwrap();
        assert!(edges.iter().any(|e| e.edge_type == "EVIDENCES" && e.target_id == telos.id));
    }

    #[test]
    fn test_auto_wire_no_duplicates() {
        let conn = setup_db();
        let telos = create_test_node(&conn, "telos", "Test Telos");
        let obs = create_test_node(&conn, "observation", "Test Observation");

        // Wire twice
        auto_wire_edges(&conn, &obs.id, "observation", &[telos.id.clone()]).unwrap();
        let created = auto_wire_edges(&conn, &obs.id, "observation", &[telos.id.clone()]).unwrap();

        assert_eq!(created, 0, "Should not create duplicate edges");
    }

    #[test]
    fn test_auto_wire_action_decomposes_to_parent() {
        let conn = setup_db();
        let telos = create_test_node(&conn, "telos", "Test Telos");
        let action = create_test_node(&conn, "action", "Test Action");

        let created = auto_wire_edges(
            &conn,
            &action.id,
            "action",
            &[telos.id.clone()],
        )
        .unwrap();

        assert_eq!(created, 1);

        // Verify: telos DECOMPOSES_TO action (parent is source)
        let edges = crud::get_edges(&conn, Some(&telos.id), None, None, None, 1000).unwrap();
        assert!(edges.iter().any(|e| e.edge_type == "DECOMPOSES_TO" && e.target_id == action.id));
    }

    #[test]
    fn test_auto_wire_empty_parents() {
        let conn = setup_db();
        let obs = create_test_node(&conn, "observation", "Test Observation");

        let created = auto_wire_edges(&conn, &obs.id, "observation", &[]).unwrap();
        assert_eq!(created, 0);
    }

    #[test]
    fn test_auto_wire_nonexistent_parent_skipped() {
        let conn = setup_db();
        let obs = create_test_node(&conn, "observation", "Test Observation");

        let created = auto_wire_edges(
            &conn,
            &obs.id,
            "observation",
            &["nonexistent".to_string()],
        )
        .unwrap();

        assert_eq!(created, 0);
    }
}
