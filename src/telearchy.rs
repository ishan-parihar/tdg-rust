//! Two-axis stage-gated telos hierarchy engine.
//!
//! Ported from Python `telearchy/tdg_telearchy_engine.py`.

use std::collections::HashMap;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use crate::db::crud;
use crate::error::TdgResult;
use crate::schema::{Stage, TelosLevel, MAX_PARENT_CHILD_STAGE_DELTA, BYPASS_RISK_THRESHOLD};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageEvidence {
    pub evidence_edges: usize,
    pub realization_edges: usize,
    pub constraints_resolved: usize,
    pub completed_children: usize,
    pub active_children: usize,
    pub total_children: usize,
    pub hypothesis_support: usize,
    pub digestion_events: usize,
    pub cross_quadrant_edges: usize,
    pub external_validations: usize,
    pub relationship_exchanges: usize,
    pub node_age_days: u32,
    pub bypass_risk: f64,
}

impl Default for StageEvidence {
    fn default() -> Self {
        Self {
            evidence_edges: 0,
            realization_edges: 0,
            constraints_resolved: 0,
            completed_children: 0,
            active_children: 0,
            total_children: 0,
            hypothesis_support: 0,
            digestion_events: 0,
            cross_quadrant_edges: 0,
            external_validations: 0,
            relationship_exchanges: 0,
            node_age_days: 0,
            bypass_risk: 0.0,
        }
    }
}

impl StageEvidence {
    pub fn total_evidence(&self) -> f64 {
        (self.evidence_edges as f64 * 1.0)
            + (self.realization_edges as f64 * 1.5)
            + (self.constraints_resolved as f64 * 2.0)
            + (self.hypothesis_support as f64 * 0.5)
            + (self.digestion_events as f64 * 1.0)
            + (self.cross_quadrant_edges as f64 * 1.2)
            + (self.external_validations as f64 * 2.0)
            + (self.relationship_exchanges as f64 * 0.8)
    }

    pub fn integration_score(&self) -> f64 {
        let child_completion = if self.total_children > 0 {
            self.completed_children as f64 / self.total_children as f64
        } else {
            0.0
        };
        let evidence_density = (self.total_evidence() / 50.0).min(1.0);
        let cross_quadrant = (self.cross_quadrant_edges as f64 / 10.0).min(1.0);
        0.3 * child_completion + 0.4 * evidence_density + 0.3 * cross_quadrant
    }
}

pub struct EvidenceCollector<'a> {
    conn: &'a Connection,
}

impl<'a> EvidenceCollector<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn collect(&self, node_id: &str) -> TdgResult<StageEvidence> {
        let edges = crud::get_edges(self.conn, Some(node_id), None, None, None, 10000)?;
        let incoming = crud::get_edges(self.conn, None, Some(node_id), None, None, 10000)?;

        let mut ev = StageEvidence::default();

        for e in &edges {
            match e.edge_type.as_str() {
                "EVIDENCES" | "ILLUMINATES" => ev.evidence_edges += 1,
                "REALIZES" | "ADVANCES" => ev.realization_edges += 1,
                "RESONATES_WITH" | "OPENS" => ev.hypothesis_support += 1,
                "DIGESTS_TO" => ev.digestion_events += 1,
                "REPLIES" | "CONTINUES" | "SENT" | "RECEIVED" => ev.relationship_exchanges += 1,
                "BLOCKS" => ev.constraints_resolved += 1,
                _ => {}
            }
        }

        for e in &incoming {
            match e.edge_type.as_str() {
                "EVIDENCES" | "ILLUMINATES" => ev.evidence_edges += 1,
                "SUPPORTS" => ev.external_validations += 1,
                _ => {}
            }
        }

        let children = crud::get_edges(self.conn, Some(node_id), None, Some("DECOMPOSES_TO"), None, 1000)?;
        ev.total_children = children.len();
        for c in &children {
            if let Ok(Some(child_node)) = crud::get_node(self.conn, &c.target_id) {
                if child_node.lifecycle_state == "completed" {
                    ev.completed_children += 1;
                } else if child_node.lifecycle_state != "archived" {
                    ev.active_children += 1;
                }
            }
        }

        if ev.total_children > 0 {
            let completed_ratio = ev.completed_children as f64 / ev.total_children as f64;
            ev.bypass_risk = 1.0 - completed_ratio;
        }

        Ok(ev)
    }
}

pub struct TelearchyEngine<'a> {
    conn: &'a Connection,
}

impl<'a> TelearchyEngine<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn validate_hierarchy(&self, root_id: &str) -> TdgResult<Vec<String>> {
        let mut issues = Vec::new();
        let root = crud::get_node(self.conn, root_id)?
            .ok_or_else(|| crate::error::TdgError::NotFound(root_id.to_string()))?;
        let collector = EvidenceCollector::new(self.conn);

        let children = crud::get_edges(self.conn, Some(root_id), None, Some("DECOMPOSES_TO"), None, 1000)?;
        for c in &children {
            if let Ok(Some(child)) = crud::get_node(self.conn, &c.target_id) {
                if let (Some(root_stage), Some(child_stage)) = (root.developmental_stage, child.developmental_stage) {
                    let delta = (root_stage as i8 - child_stage as i8).unsigned_abs();
                    if delta > MAX_PARENT_CHILD_STAGE_DELTA {
                        issues.push(format!(
                            "Stage gap too large: {} ({}) → {} ({})",
                            root.name, root_stage, child.name, child_stage
                        ));
                    }
                }

                if let (Some(root_tl), Some(child_tl)) = (&root.teleological_level, &child.teleological_level) {
                    if root_tl != "T4" && child_tl == "T0" {
                        issues.push(format!(
                            "T0 node {} has non-T4 parent {} ({})",
                            child.name, root.name, root_tl
                        ));
                    }
                }
            }
        }

        let ev = collector.collect(root_id)?;
        if ev.bypass_risk > BYPASS_RISK_THRESHOLD {
            issues.push(format!(
                "High bypass risk ({:.2}) for {}",
                ev.bypass_risk, root.name
            ));
        }

        Ok(issues)
    }

    pub fn compute_stage_evidence(&self, node_id: &str) -> TdgResult<StageEvidence> {
        let collector = EvidenceCollector::new(self.conn);
        collector.collect(node_id)
    }

    pub fn advance_stage(&self, node_id: &str) -> TdgResult<Option<Stage>> {
        let node = crud::get_node(self.conn, node_id)?
            .ok_or_else(|| crate::error::TdgError::NotFound(node_id.to_string()))?;
        let current = node.developmental_stage.unwrap_or(1) as u8;
        let stage = Stage::from_u8(current).unwrap_or(Stage::Survival);

        let ev = self.compute_stage_evidence(node_id)?;
        let reqs = crate::schema::stage_evidence_requirements();
        let age_gates = crate::schema::stage_age_gates();

        let next = match stage {
            Stage::Survival => Some(Stage::Identity),
            Stage::Identity => Some(Stage::Power),
            Stage::Power => Some(Stage::Heart),
            Stage::Heart => Some(Stage::Rational),
            Stage::Rational => Some(Stage::Pluralistic),
            Stage::Pluralistic => Some(Stage::Integral),
            Stage::Integral => Some(Stage::Harvest),
            Stage::Harvest => None,
        };

        if let Some(next_stage) = next {
            let required = reqs.get(&next_stage).copied().unwrap_or(usize::MAX);
            let evidence = ev.total_evidence() as usize;
            if evidence < required {
                return Ok(None);
            }

            if let Some(&min_days) = age_gates.get(&next_stage) {
                if ev.node_age_days < min_days {
                    return Ok(None);
                }
            }

            let mut updates = HashMap::new();
            updates.insert("developmental_stage".to_string(), serde_json::json!(next_stage.as_u8() as i32));
            crud::update_node(self.conn, node_id, &updates)?;

            return Ok(Some(next_stage));
        }

        Ok(None)
    }

    pub fn check_tlevel_promotion(&self, node_id: &str) -> TdgResult<Option<TelosLevel>> {
        let node = crud::get_node(self.conn, node_id)?
            .ok_or_else(|| crate::error::TdgError::NotFound(node_id.to_string()))?;
        let current_tl = node.teleological_level.as_deref().unwrap_or("T4");
        let tl: TelosLevel = current_tl.parse()
            .map_err(|e: String| crate::error::TdgError::Custom(format!("Invalid telos level: {}", e)))?;

        let promos = crate::schema::tlevel_promotion_stage();
        if let Some(&required_stage) = promos.get(&tl) {
            let current_stage = node.developmental_stage.unwrap_or(1) as u8;
            if current_stage >= required_stage.as_u8() {
                let next_tl = TelosLevel::from_u8(tl.as_u8().wrapping_sub(1));
                return Ok(next_tl);
            }
        }

        Ok(None)
    }

    pub fn promote_tlevel(&self, node_id: &str) -> TdgResult<Option<TelosLevel>> {
        if let Some(next_tl) = self.check_tlevel_promotion(node_id)? {
            let mut updates = HashMap::new();
            updates.insert("teleological_level".to_string(), serde_json::json!(next_tl.to_string()));
            crud::update_node(self.conn, node_id, &updates)?;
            return Ok(Some(next_tl));
        }
        Ok(None)
    }

    pub fn generate_telearchy_report(&self, root_id: &str) -> TdgResult<TelearchyReport> {
        let root = crud::get_node(self.conn, root_id)?
            .ok_or_else(|| crate::error::TdgError::NotFound(root_id.to_string()))?;
        let ev = self.compute_stage_evidence(root_id)?;
        let issues = self.validate_hierarchy(root_id)?;

        let children = crud::get_edges(self.conn, Some(root_id), None, Some("DECOMPOSES_TO"), None, 1000)?;
        let child_reports: Vec<ChildReport> = children.iter().filter_map(|c| {
            crud::get_node(self.conn, &c.target_id).ok().flatten().map(|child| ChildReport {
                id: child.id.clone(),
                name: child.name.clone(),
                node_type: child.node_type.clone(),
                stage: child.developmental_stage.unwrap_or(1) as u8,
                tlevel: child.teleological_level.clone().unwrap_or_else(|| "T4".to_string()),
                lifecycle_state: child.lifecycle_state.clone(),
            })
        }).collect();

        Ok(TelearchyReport {
            root_id: root.id.clone(),
            root_name: root.name.clone(),
            root_stage: root.developmental_stage.unwrap_or(1) as u8,
            root_tlevel: root.teleological_level.clone().unwrap_or_else(|| "T4".to_string()),
            stage_evidence: ev,
            children: child_reports,
            issues,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelearchyReport {
    pub root_id: String,
    pub root_name: String,
    pub root_stage: u8,
    pub root_tlevel: String,
    pub stage_evidence: StageEvidence,
    pub children: Vec<ChildReport>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildReport {
    pub id: String,
    pub name: String,
    pub node_type: String,
    pub stage: u8,
    pub tlevel: String,
    pub lifecycle_state: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::{init_schema, run_migrations};
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_stage_evidence_defaults() {
        let ev = StageEvidence::default();
        assert_eq!(ev.total_evidence(), 0.0);
        assert_eq!(ev.integration_score(), 0.0);
    }

    #[test]
    fn test_stage_evidence_weighted() {
        let ev = StageEvidence {
            evidence_edges: 10,
            realization_edges: 5,
            cross_quadrant_edges: 3,
            ..Default::default()
        };
        let total = ev.total_evidence();
        assert!((total - (10.0 + 7.5 + 3.6)).abs() < 0.01);
    }

    #[test]
    fn test_integration_score() {
        let ev = StageEvidence {
            completed_children: 3,
            total_children: 5,
            evidence_edges: 25,
            cross_quadrant_edges: 5,
            ..Default::default()
        };
        let score = ev.integration_score();
        assert!(score > 0.0 && score <= 1.0);
    }

    #[test]
    fn test_validate_empty_hierarchy() {
        let conn = setup_db();
        let engine = TelearchyEngine::new(&conn);
        let node = crud::add_node(&conn, &crate::models::NewNode {
            node_type: "telos".to_string(),
            name: "Test".to_string(),
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
        }).unwrap();
        let issues = engine.validate_hierarchy(&node.id).unwrap();
        assert!(issues.is_empty());
    }
}
