//! Pulse Engine — structural gap detection per node type
//!
//! Port of `core/mind/pulse_engine.py`.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::TdgResult;

/// Closure rules for a node type.
#[derive(Debug, Clone)]
pub struct ClosureRule {
    pub min_edges: i64,
    pub required_edge_types: Vec<&'static str>,
    pub edge_direction: &'static str, // "outgoing" or "incoming"
    pub age_multiplier: f64,
}

impl ClosureRule {
    fn new(
        min_edges: i64,
        required_edge_types: &'static [&'static str],
        edge_direction: &'static str,
        age_multiplier: f64,
    ) -> Self {
        Self {
            min_edges,
            required_edge_types: required_edge_types.to_vec(),
            edge_direction,
            age_multiplier,
        }
    }
}

/// Default closure rules per node type.
fn closure_rules() -> HashMap<&'static str, ClosureRule> {
    let mut m = HashMap::new();
    m.insert(
        "telos",
        ClosureRule::new(2, &["DECOMPOSES_TO", "ENABLES"], "outgoing", 1.0),
    );
    m.insert(
        "action",
        ClosureRule::new(
            2,
            &["DECOMPOSES_TO", "DEPENDS_ON", "ENABLES"],
            "outgoing",
            1.2,
        ),
    );
    m.insert(
        "capability",
        ClosureRule::new(1, &["ENABLES", "HAS_CAPABILITY"], "outgoing", 1.0),
    );
    m.insert(
        "observation",
        ClosureRule::new(1, &["EVIDENCES", "SUPPORTS", "RELATES_TO"], "outgoing", 1.5),
    );
    m.insert(
        "hypothesis",
        ClosureRule::new(
            2,
            &["EVIDENCES", "SUPPORTS", "CONTRADICTS"],
            "incoming",
            1.3,
        ),
    );
    m.insert(
        "constraint",
        ClosureRule::new(0, &[], "outgoing", 1.0), // latent
    );
    m.insert(
        "discovery",
        ClosureRule::new(1, &["EVIDENCES", "SYNTHESIZES"], "outgoing", 1.2),
    );
    m.insert(
        "project",
        ClosureRule::new(
            2,
            &["DECOMPOSES_TO", "DEPENDS_ON", "ADVANCES"],
            "outgoing",
            1.4,
        ),
    );
    m.insert(
        "trajectory",
        ClosureRule::new(2, &["PRECEDES", "DEPENDS_ON"], "outgoing", 1.0),
    );
    m.insert(
        "synthesis",
        ClosureRule::new(
            2,
            &["SYNTHESIZES", "SUPPORTS", "CONTRADICTS"],
            "incoming",
            1.1,
        ),
    );
    m.insert(
        "people",
        ClosureRule::new(1, &["OWNS", "PURSUES", "HAS_CAPABILITY"], "outgoing", 1.0),
    );
    m.insert(
        "being",
        ClosureRule::new(1, &["SENT", "RECEIVED", "CREATES"], "outgoing", 1.0),
    );
    m.insert(
        "communication",
        ClosureRule::new(1, &["SENT", "RECEIVED", "REPLIES"], "outgoing", 1.2),
    );
    m.insert(
        "event",
        ClosureRule::new(1, &["TRIGGERED", "DETECTED", "RELATES_TO"], "outgoing", 1.5),
    );
    m.insert(
        "insight",
        ClosureRule::new(
            1,
            &["EVIDENCES", "SYNTHESIZES", "ILLUMINATES"],
            "outgoing",
            1.3,
        ),
    );
    m.insert(
        "question",
        ClosureRule::new(1, &["SEEKS", "OPENS", "RELATES_TO"], "outgoing", 1.5),
    );
    m.insert(
        "skill",
        ClosureRule::new(0, &[], "outgoing", 1.0), // latent
    );
    m.insert(
        "artifact",
        ClosureRule::new(1, &["CREATES", "REFERENCES", "CONTEXT"], "incoming", 1.0),
    );
    m
}

/// Severity of a structural gap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PulseSeverity {
    Critical,
    Gap,
    Minor,
    Healthy,
}

/// A structural gap detected for a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PulseResult {
    pub node_id: String,
    pub node_type: String,
    pub name: String,
    pub gap_score: f64,
    pub existing_edges: i64,
    pub required_edges: i64,
    pub severity: PulseSeverity,
    pub age_days: i64,
}

/// The Pulse Engine — detects structural gaps in the graph.
pub struct PulseEngine {
    rules: HashMap<&'static str, ClosureRule>,
}

impl PulseEngine {
    pub fn new() -> Self {
        Self {
            rules: closure_rules(),
        }
    }

    /// Analyze all active nodes for structural gaps.
    pub fn pulse(&self, conn: &Connection, exclude_types: &[&str]) -> TdgResult<Vec<PulseResult>> {
        let mut results = Vec::new();

        let mut stmt = conn.prepare(
            "SELECT id, node_type, name, created_at FROM nodes WHERE valid_to IS NULL AND lifecycle_state = 'active'",
        )?;

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

        let now = chrono::Utc::now().naive_utc();

        for (id, node_type, name, created_at) in &rows {
            if exclude_types.contains(&node_type.as_str()) {
                continue;
            }

            let rule = match self.rules.get(node_type.as_str()) {
                Some(r) => r,
                None => continue,
            };

            // Skip latent types
            if rule.min_edges == 0 {
                continue;
            }

            // Count matching edges
            let edge_count = self.count_matching_edges(conn, id, rule)?;

            // Calculate age in days
            let age_days = chrono::NaiveDateTime::parse_from_str(
                created_at.replace('Z', "").as_str(),
                "%Y-%m-%dT%H:%M:%S%.f",
            )
            .ok()
            .map(|created| (now - created).num_days())
            .unwrap_or(0);

            if edge_count >= rule.min_edges {
                continue; // No gap
            }

            let gap = rule.min_edges - edge_count;
            let gap_score = gap as f64 * (age_days as f64 / 30.0).max(1.0) * rule.age_multiplier;

            let severity = if gap_score > 10.0 {
                PulseSeverity::Critical
            } else if gap_score > 3.0 {
                PulseSeverity::Gap
            } else {
                PulseSeverity::Minor
            };

            results.push(PulseResult {
                node_id: id.clone(),
                node_type: node_type.clone(),
                name: name.clone(),
                gap_score,
                existing_edges: edge_count,
                required_edges: rule.min_edges,
                severity,
                age_days,
            });
        }

        // Sort by gap_score descending (most critical first)
        results.sort_by(|a, b| {
            b.gap_score
                .partial_cmp(&a.gap_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }

    fn count_matching_edges(
        &self,
        conn: &Connection,
        node_id: &str,
        rule: &ClosureRule,
    ) -> TdgResult<i64> {
        if rule.required_edge_types.is_empty() {
            return Ok(0);
        }

        let placeholders: Vec<String> = rule
            .required_edge_types
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 2))
            .collect();

        let sql = if rule.edge_direction == "outgoing" {
            format!(
                "SELECT COUNT(*) FROM edges WHERE source_id = ?1 AND valid_to IS NULL AND edge_type IN ({})",
                placeholders.join(",")
            )
        } else {
            format!(
                "SELECT COUNT(*) FROM edges WHERE target_id = ?1 AND valid_to IS NULL AND edge_type IN ({})",
                placeholders.join(",")
            )
        };

        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new(node_id.to_string()));
        for et in &rule.required_edge_types {
            params.push(Box::new(et.to_string()));
        }

        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let count: i64 = conn.query_row(&sql, params_ref.as_slice(), |row| row.get(0))?;
        Ok(count)
    }

    /// Get a summary of pulse results.
    pub fn summarize(&self, results: &[PulseResult]) -> serde_json::Value {
        let total = results.len();
        let critical = results
            .iter()
            .filter(|r| r.severity == PulseSeverity::Critical)
            .count();
        let gap = results
            .iter()
            .filter(|r| r.severity == PulseSeverity::Gap)
            .count();
        let minor = results
            .iter()
            .filter(|r| r.severity == PulseSeverity::Minor)
            .count();

        let by_type: HashMap<String, usize> = results.iter().fold(HashMap::new(), |mut acc, r| {
            *acc.entry(r.node_type.clone()).or_insert(0) += 1;
            acc
        });

        serde_json::json!({
            "total_gaps": total,
            "critical": critical,
            "gap": gap,
            "minor": minor,
            "by_type": by_type,
            "top_gaps": results.iter().take(10).map(|r| serde_json::json!({
                "node_id": r.node_id,
                "name": r.name,
                "node_type": r.node_type,
                "gap_score": r.gap_score,
                "severity": format!("{:?}", r.severity).to_lowercase(),
            })).collect::<Vec<_>>(),
        })
    }
}

impl Default for PulseEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_fts, init_schema, run_migrations};
    use crate::models::NewNode;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn pulse_detects_gaps() {
        let conn = setup_db();

        // Add a telos with no edges (requires 2)
        crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Gap Telos".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Add a telos with edges (satisfies closure)
        let good_telos = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Good Telos".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        let action1 = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "Action 1".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: good_telos.id.clone(),
                target_id: action1.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Add a second edge to satisfy min_edges=2 for telos
        let action2 = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "Action 2".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: good_telos.id.clone(),
                target_id: action2.id.clone(),
                edge_type: "ENABLES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let engine = PulseEngine::new();
        let results = engine.pulse(&conn, &[]).unwrap();

        // The gap telos should appear, the good telos should not
        assert!(results.iter().any(|r| r.name == "Gap Telos"));
        assert!(!results.iter().any(|r| r.name == "Good Telos"));
    }

    #[test]
    fn pulse_skips_latent_types() {
        let conn = setup_db();
        crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "constraint".to_string(),
                name: "Constraint".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let engine = PulseEngine::new();
        let results = engine.pulse(&conn, &[]).unwrap();
        assert!(!results.iter().any(|r| r.node_type == "constraint"));
    }

    #[test]
    fn pulse_summary() {
        let engine = PulseEngine::new();
        let results = vec![PulseResult {
            node_id: "n1".to_string(),
            node_type: "telos".to_string(),
            name: "T1".to_string(),
            gap_score: 15.0,
            existing_edges: 0,
            required_edges: 2,
            severity: PulseSeverity::Critical,
            age_days: 30,
        }];
        let summary = engine.summarize(&results);
        assert_eq!(summary["critical"], 1);
        assert_eq!(summary["total_gaps"], 1);
    }
}
