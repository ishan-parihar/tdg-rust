use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use crate::error::TdgResult;
use crate::mind::reflect_engine::{ReflectEngine, ReflectResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationReport {
    pub status: String,
    pub message: String,
    pub timestamp: String,
    pub elapsed_seconds: f64,
    pub nodes_count: i64,
    pub edges_count: i64,
    pub node_types: std::collections::HashMap<String, i64>,
    pub orphans: i64,
    pub reflection: Option<ReflectResult>,
    pub constraint_health: ConstraintHealth,
    pub insights: Vec<String>,
    pub patterns: Vec<String>,
    pub recommendations: Vec<String>,
    pub actions_taken: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintHealth {
    pub total: i64,
    pub active: i64,
}

pub struct ConsolidationEngine<'a> {
    conn: &'a Connection,
    lean: bool,
}

impl<'a> ConsolidationEngine<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn, lean: false }
    }

    pub fn with_lean(conn: &'a Connection, lean: bool) -> Self {
        Self { conn, lean }
    }

    pub fn run(&self) -> TdgResult<ConsolidationReport> {
        let start = std::time::Instant::now();
        let timestamp = chrono::Utc::now().to_rfc3339();

        if self.lean {
            return self.quick_health_snapshot(timestamp, start);
        }

        let mut actions_taken = Vec::new();
        let mut insights = Vec::new();
        let mut patterns = Vec::new();
        let mut recommendations = Vec::new();

        let node_types = self.count_by_type()?;
        let nodes_count: i64 = node_types.values().sum();

        let edges_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM edges",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        actions_taken.push(format!("Captured graph health: {} nodes, {} edges", nodes_count, edges_count));

        let reflect_result = self.run_reflection(&mut actions_taken, &mut patterns);

        let constraint_health = self.analyze_constraints(&mut recommendations)?;

        self.analyze_recent_activity(&mut insights)?;

        self.analyze_edge_density(nodes_count, edges_count, &node_types, &mut insights, &mut recommendations)?;

        let orphans = self.count_orphans()?;
        if orphans > 0 {
            recommendations.push(format!(
                "{} node(s) have no edges — they are disconnected from the graph.",
                orphans
            ));
        }

        if !node_types.is_empty() {
            let mut sorted: Vec<_> = node_types.iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(a.1));
            let top: Vec<String> = sorted.iter().take(5).map(|(t, c)| format!("{}: {}", t, c)).collect();
            insights.push(format!("Node distribution — top types: {}", top.join(", ")));
            if let Some((dominant_type, count)) = sorted.first() {
                patterns.push(format!(
                    "Graph composition: {} distinct node types, dominated by {} ({} nodes).",
                    node_types.len(), dominant_type, count
                ));
            }
        }

        let elapsed = start.elapsed().as_secs_f64();
        let status = if reflect_result.as_ref().map_or(false, |r| !r.skipped) {
            "consolidated".to_string()
        } else {
            "consolidated_partial".to_string()
        };

        let mut message_parts = vec![
            format!("Consolidation complete in {:.1}s.", elapsed),
            format!("Graph: {} nodes, {} edges.", nodes_count, edges_count),
        ];
        if let Some(ref r) = reflect_result {
            if !r.skipped {
                message_parts.push(format!(
                    "Reflection: {} clusters, {} skills.",
                    r.clusters_processed, r.skills_created
                ));
            } else {
                message_parts.push("Reflection phase skipped.".to_string());
            }
        }

        Ok(ConsolidationReport {
            status,
            message: message_parts.join(" "),
            timestamp,
            elapsed_seconds: elapsed,
            nodes_count,
            edges_count,
            node_types,
            orphans,
            reflection: reflect_result,
            constraint_health,
            insights,
            patterns,
            recommendations,
            actions_taken,
        })
    }

    fn quick_health_snapshot(&self, timestamp: String, start: std::time::Instant) -> TdgResult<ConsolidationReport> {
        let node_types = self.count_by_type()?;
        let nodes_count: i64 = node_types.values().sum();
        let edges_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM edges",
            [],
            |row| row.get(0),
        ).unwrap_or(0);
        let orphans = self.count_orphans()?;
        let elapsed = start.elapsed().as_secs_f64();

        Ok(ConsolidationReport {
            status: "consolidated_lean".to_string(),
            message: format!("Lean consolidation in {:.1}s. Graph: {} nodes, {} edges.", elapsed, nodes_count, edges_count),
            timestamp,
            elapsed_seconds: elapsed,
            nodes_count,
            edges_count,
            node_types,
            orphans,
            reflection: None,
            constraint_health: ConstraintHealth { total: 0, active: 0 },
            insights: vec!["Lean mode: cross-cutting analysis skipped.".to_string()],
            patterns: Vec::new(),
            recommendations: Vec::new(),
            actions_taken: vec![format!("Lean health snapshot: {} nodes, {} edges", nodes_count, edges_count)],
        })
    }

    fn count_by_type(&self) -> TdgResult<std::collections::HashMap<String, i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT node_type, COUNT(*) as cnt FROM nodes
             WHERE lifecycle_state = 'active'
             GROUP BY node_type
             ORDER BY cnt DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0).unwrap_or_default(), row.get(1)?))
        })?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            if let Ok((nt, cnt)) = row {
                map.insert(nt, cnt);
            }
        }
        Ok(map)
    }

    fn count_orphans(&self) -> TdgResult<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM nodes n
             WHERE NOT EXISTS (
                 SELECT 1 FROM edges e
                 WHERE (e.source_id = n.id OR e.target_id = n.id)
             )",
            [],
            |row| row.get(0),
        ).unwrap_or(0);
        Ok(count)
    }

    fn run_reflection(
        &self,
        actions_taken: &mut Vec<String>,
        patterns: &mut Vec<String>,
    ) -> Option<ReflectResult> {
        let engine = ReflectEngine::new(self.conn);
        match engine.run() {
            Ok(result) => {
                if !result.skipped {
                    actions_taken.push(format!(
                        "Reflection: analyzed {} obs, {} clusters, {} skills, {} discoveries",
                        result.observations_analyzed,
                        result.clusters_processed,
                        result.skills_created,
                        result.discoveries_created,
                    ));
                    if result.skills_created > 0 {
                        patterns.push(format!(
                            "Reflection engine discovered {} new skill pattern(s) from entity clustering.",
                            result.skills_created
                        ));
                    }
                } else {
                    actions_taken.push(format!("Reflection skipped: {}", result.skip_reason));
                }
                Some(result)
            }
            Err(e) => {
                actions_taken.push(format!("Reflection engine raised: {}", e));
                None
            }
        }
    }

    fn analyze_constraints(&self, recommendations: &mut Vec<String>) -> TdgResult<ConstraintHealth> {
        let total: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE node_type = 'constraint'",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        let active: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT source_id) FROM edges WHERE edge_type = 'BLOCKS'",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        if total > 0 && active < total {
            recommendations.push(format!(
                "{} constraint(s) have no active BLOCKS edges — review if they are still relevant.",
                total - active
            ));
        }

        Ok(ConstraintHealth { total, active })
    }

    fn analyze_recent_activity(&self, insights: &mut Vec<String>) -> TdgResult<()> {
        let since_24h = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM nodes
             WHERE node_type = 'observation'
               AND created_at >= ?1",
            rusqlite::params![since_24h],
            |row| row.get(0),
        ).unwrap_or(0);

        if count > 0 {
            insights.push(format!("{} observation(s) recorded in the last 24 hours.", count));
        } else {
            insights.push("No observations recorded in the last 24 hours — agent may be idle.".to_string());
        }

        Ok(())
    }

    fn analyze_edge_density(
        &self,
        _nodes_count: i64,
        edges_count: i64,
        node_types: &std::collections::HashMap<String, i64>,
        insights: &mut Vec<String>,
        recommendations: &mut Vec<String>,
    ) -> TdgResult<()> {
        let obs_count = node_types.get("observation").copied().unwrap_or(0);
        if obs_count > 0 && edges_count > 0 {
            let edge_per_obs = edges_count as f64 / obs_count as f64;
            let rounded = (edge_per_obs * 10.0).round() / 10.0;
            if rounded < 1.0 {
                recommendations.push(format!(
                    "Low edge density ({} edges/observation) — observations may be under-connected.",
                    rounded
                ));
            }
            insights.push(format!("Graph edge density: {} edges per observation.", rounded));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::crud;
    use crate::db::schema::{init_schema, run_migrations};

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_consolidation_empty_graph() {
        let conn = setup_db();
        let engine = ConsolidationEngine::new(&conn);
        let report = engine.run().unwrap();
        assert_eq!(report.status, "consolidated_partial");
        assert_eq!(report.nodes_count, 0);
        assert_eq!(report.edges_count, 0);
    }

    #[test]
    fn test_consolidation_lean_mode() {
        let conn = setup_db();
        let engine = ConsolidationEngine::with_lean(&conn, true);
        let report = engine.run().unwrap();
        assert_eq!(report.status, "consolidated_lean");
        assert!(report.reflection.is_none());
    }

    #[test]
    fn test_consolidation_with_nodes() {
        let conn = setup_db();
        for i in 0..10 {
            crud::add_node(&conn, &crate::models::NewNode {
                node_type: "observation".to_string(),
                name: format!("Obs {}", i),
                description: None,
                properties: None,
                quadrants: None,
                drives: None,
                lifecycle_state: Some("active".to_string()),
                teleological_level: None,
                developmental_stage: None,
                confidence: None,
                source: None,
                parent_ids: None,
                agent_id: None,
            }).unwrap();
        }
        let engine = ConsolidationEngine::new(&conn);
        let report = engine.run().unwrap();
        assert_eq!(report.nodes_count, 10);
        assert!(report.node_types.contains_key("observation"));
    }

    #[test]
    fn test_orphan_detection() {
        let conn = setup_db();
        crud::add_node(&conn, &crate::models::NewNode {
            node_type: "observation".to_string(),
            name: "Orphan".to_string(),
            description: None,
            properties: None,
            quadrants: None,
            drives: None,
            lifecycle_state: Some("active".to_string()),
            teleological_level: None,
            developmental_stage: None,
            confidence: None,
            source: None,
            parent_ids: None,
            agent_id: None,
        }).unwrap();
        let engine = ConsolidationEngine::new(&conn);
        let report = engine.run().unwrap();
        assert_eq!(report.orphans, 1);
        assert!(report.recommendations.iter().any(|r| r.contains("no edges")));
    }
}
