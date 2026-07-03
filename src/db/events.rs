use rusqlite::{params, Connection};

use crate::error::TdgResult;
use crate::models::Node;

/// Trust score computation: 60% confidence + 25% helpful rate + 15% normalized retrieval.
pub fn compute_trust(confidence: f64, helpful_count: i32, retrieval_count: i32) -> f64 {
    let helpful_rate = if helpful_count > 0 {
        helpful_count as f64 / (helpful_count as f64 + 1.0)
    } else {
        0.0
    };
    // Normalize retrieval count (diminishing returns via log scale)
    let retrieval_norm = if retrieval_count > 0 {
        (retrieval_count as f64).ln() / (1.0 + (retrieval_count as f64).ln())
    } else {
        0.0
    };

    0.6 * confidence + 0.25 * helpful_rate + 0.15 * retrieval_norm
}

/// Rate a node with helpful/unhelpful feedback. Updates helpful_count and recomputes trust.
///
/// Returns `Ok(None)` if the node doesn't exist or is archived.
/// Previously returned `Err` for missing nodes (violating the `Option` return type)
/// and mutated archived nodes (the UPDATE had `valid_to IS NULL` but the SELECT
/// did not, so archived rows were read and their confidence was mutated).
pub fn rate_node(conn: &Connection, node_id: &str, helpful: bool) -> TdgResult<Option<Node>> {
    let delta = if helpful { 1 } else { 0 };
    let now = crate::db::crud::now_iso();

    // Use MAX(0, ...) to prevent negative helpful_count (same fix as tdg_rate_memory)
    let affected = conn.execute(
        "UPDATE nodes SET helpful_count = MAX(0, helpful_count + ?1), updated_at = ?2
         WHERE id = ?3 AND valid_to IS NULL",
        params![delta, now, node_id],
    )?;

    if affected == 0 {
        // Node doesn't exist or is archived — return Ok(None) instead of Err
        return Ok(None);
    }

    // Recompute confidence based on helpful ratio.
    // Both SELECT and UPDATE now gate on valid_to IS NULL to avoid touching archived nodes.
    let row: Option<(i32, i32, f64)> = conn
        .query_row(
            "SELECT helpful_count, retrieval_count, confidence FROM nodes
             WHERE id = ?1 AND valid_to IS NULL",
            params![node_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok();

    if let Some((helpful_count, _retrieval_count, current_confidence)) = row {
        if helpful_count > 0 {
            let new_confidence = current_confidence * 0.8
                + (helpful_count as f64 / (helpful_count as f64 + 1.0)) * 0.2;
            conn.execute(
                "UPDATE nodes SET confidence = ?1 WHERE id = ?2 AND valid_to IS NULL",
                params![new_confidence, node_id],
            )?;
        }
    }

    crate::db::crud::get_node(conn, node_id)
}

/// Increment retrieval_count for a node.
///
/// Uses now_iso() for updated_at (consistent with all other timestamp writes).
/// Previously used datetime('now','subsec') which produces a THIRD timestamp
/// format (space-separated, no timezone) — breaking cross-table comparisons.
pub fn record_retrieval(conn: &Connection, node_id: &str) -> TdgResult<bool> {
    let now = crate::db::crud::now_iso();
    let affected = conn.execute(
        "UPDATE nodes SET retrieval_count = retrieval_count + 1, updated_at = ?2
         WHERE id = ?1 AND valid_to IS NULL",
        params![node_id, now],
    )?;
    Ok(affected > 0)
}

/// Get composite trust score for a node.
pub fn get_trust_score(conn: &Connection, node_id: &str) -> TdgResult<Option<f64>> {
    let result = conn.query_row(
        "SELECT confidence, helpful_count, retrieval_count FROM nodes WHERE id = ?1 AND valid_to IS NULL",
        params![node_id],
        |row| {
            let confidence: f64 = row.get(0)?;
            let helpful: i32 = row.get(1)?;
            let retrieval: i32 = row.get(2)?;
            Ok(compute_trust(confidence, helpful, retrieval))
        },
    );

    match result {
        Ok(score) => Ok(Some(score)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// List nodes sorted by trust score descending.
pub fn list_by_trust(
    conn: &Connection,
    node_type: Option<&str>,
    limit: i64,
) -> TdgResult<Vec<Node>> {
    let limit = limit.min(crate::validation::MAX_LIMIT);

    let (sql, param_value): (String, Option<String>) = match node_type {
        Some(nt) => (
            "SELECT id, node_type, name, description, properties_json, quadrants_json,
             drives_json, lifecycle_state, teleological_level, developmental_stage,
             confidence, source, parent_ids, agent_path, created_at, updated_at,
             valid_from, valid_to, helpful_count, retrieval_count, agent_id,
             synthesis_status, scale_code, tetra_ul, tetra_ur, tetra_ll, tetra_lr, octave_id,
         realm_placement, verticality_json, collectivity, nesting_sub, nesting_sup
             FROM nodes WHERE valid_to IS NULL AND node_type = ?1
             ORDER BY confidence DESC, helpful_count DESC, retrieval_count DESC
             LIMIT ?2"
                .to_string(),
            Some(nt.to_string()),
        ),
        None => (
            "SELECT id, node_type, name, description, properties_json, quadrants_json,
             drives_json, lifecycle_state, teleological_level, developmental_stage,
             confidence, source, parent_ids, agent_path, created_at, updated_at,
             valid_from, valid_to, helpful_count, retrieval_count, agent_id,
             synthesis_status, scale_code, tetra_ul, tetra_ur, tetra_ll, tetra_lr, octave_id,
         realm_placement, verticality_json, collectivity, nesting_sub, nesting_sup
             FROM nodes WHERE valid_to IS NULL
             ORDER BY confidence DESC, helpful_count DESC, retrieval_count DESC
             LIMIT ?1"
                .to_string(),
            None,
        ),
    };

    let mut stmt = conn.prepare(&sql)?;
    let rows = if let Some(ref pv) = param_value {
        stmt.query_map(params![pv, limit], crate::db::crud::row_to_node)?
    } else {
        stmt.query_map(params![limit], crate::db::crud::row_to_node)?
    };

    let mut nodes = Vec::new();
    for row in rows {
        nodes.push(row?);
    }
    Ok(nodes)
}

/// Aggregate statistics for the database.
pub fn stats(conn: &Connection) -> TdgResult<serde_json::Value> {
    let total_nodes: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL",
        [],
        |r| r.get(0),
    )?;
    let total_edges: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges WHERE valid_to IS NULL",
        [],
        |r| r.get(0),
    )?;
    let total_events: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;

    // Count by type
    let mut stmt = conn.prepare(
        "SELECT node_type, COUNT(*) FROM nodes WHERE valid_to IS NULL GROUP BY node_type ORDER BY COUNT(*) DESC"
    )?;
    let by_type: serde_json::Map<String, serde_json::Value> = stmt
        .query_map([], |row| {
            let t: String = row.get(0)?;
            let c: i64 = row.get(1)?;
            Ok((t, serde_json::json!(c)))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Count by source
    let mut stmt = conn.prepare(
        "SELECT source, COUNT(*) FROM nodes WHERE valid_to IS NULL AND source != '' GROUP BY source"
    )?;
    let by_source: serde_json::Map<String, serde_json::Value> = stmt
        .query_map([], |row| {
            let s: String = row.get(0)?;
            let c: i64 = row.get(1)?;
            Ok((s, serde_json::json!(c)))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Edge type counts
    let mut stmt = conn.prepare(
        "SELECT edge_type, COUNT(*) FROM edges WHERE valid_to IS NULL GROUP BY edge_type",
    )?;
    let edge_types: serde_json::Map<String, serde_json::Value> = stmt
        .query_map([], |row| {
            let t: String = row.get(0)?;
            let c: i64 = row.get(1)?;
            Ok((t, serde_json::json!(c)))
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(serde_json::json!({
        "total_nodes": total_nodes,
        "total_edges": total_edges,
        "total_events": total_events,
        "nodes_by_type": by_type,
        "nodes_by_source": by_source,
        "edges_by_type": edge_types,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{crud, init_fts, init_schema, run_migrations};
    use crate::models::NewNode;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn trust_score_computation() {
        assert!(
            (compute_trust(1.0, 10, 5)
                - (0.6 * 1.0
                    + 0.25 * (10.0 / 11.0)
                    + 0.15 * (5.0_f64.ln() / (1.0 + 5.0_f64.ln()))))
            .abs()
                < 1e-10
        );
        assert!((compute_trust(1.0, 0, 0) - 0.6).abs() < 1e-10);
        assert!((compute_trust(0.0, 0, 0) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn rate_node_helpful() {
        let conn = setup_db();
        let node = crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Rateable".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let updated = rate_node(&conn, &node.id, true).unwrap().unwrap();
        assert_eq!(updated.helpful_count, 1);

        let updated = rate_node(&conn, &node.id, true).unwrap().unwrap();
        assert_eq!(updated.helpful_count, 2);

        let updated = rate_node(&conn, &node.id, false).unwrap().unwrap();
        assert_eq!(updated.helpful_count, 2); // false doesn't increment
    }

    #[test]
    fn record_retrieval_test() {
        let conn = setup_db();
        let node = crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Retrieved".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(record_retrieval(&conn, &node.id).unwrap());
        assert!(record_retrieval(&conn, &node.id).unwrap());

        let fetched = crud::get_node(&conn, &node.id).unwrap().unwrap();
        assert_eq!(fetched.retrieval_count, 2);
    }

    #[test]
    fn trust_score_query() {
        let conn = setup_db();
        let node = crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Scored".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let score = get_trust_score(&conn, &node.id).unwrap().unwrap();
        // Initial: confidence=1.0, helpful=0, retrieval=0 => 0.6*1.0 = 0.6
        assert!((score - 0.6).abs() < 0.01);
    }

    #[test]
    fn list_by_trust_test() {
        let conn = setup_db();
        crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "High Trust".to_string(),
                confidence: Some(1.0),
                ..Default::default()
            },
        )
        .unwrap();
        crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Low Trust".to_string(),
                confidence: Some(0.3),
                ..Default::default()
            },
        )
        .unwrap();

        let ranked = list_by_trust(&conn, Some("observation"), 10).unwrap();
        assert_eq!(ranked.len(), 2);
        assert!(ranked[0].confidence >= ranked[1].confidence);
    }

    #[test]
    fn stats_test() {
        let conn = setup_db();
        crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "T1".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        crud::add_node(
            &conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "A1".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let s = stats(&conn).unwrap();
        assert_eq!(s["total_nodes"], 2);
    }
}
