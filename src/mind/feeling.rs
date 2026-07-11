//! Feeling Engine — emotional state generation from drive data
//!
//! Port of `core/mind/feeling_engine.py`.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::TdgResult;
use crate::flow::{DriveDiagnosis, DualPoleDrive, FlowDriveState};

/// Feeling report output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeelingReport {
    pub feelings: Vec<String>,
    pub energy_level: String,
    pub dominant_drive: String,
    pub dominant_quadrant: String,
    pub blind_drives: Vec<String>,
    pub pathological_drives: Vec<String>,
    pub stuck_warning: Option<String>,
    pub summary: String,
}

/// The Feeling Engine — generates first-person emotional statements.
pub struct FeelingEngine;

impl FeelingEngine {
    pub fn new() -> Self {
        Self
    }

    /// Generate a feeling report from drive matrix data.
    pub fn generate(
        &self,
        conn: &Connection,
        drive_history: &[String],
    ) -> TdgResult<FeelingReport> {
        let mut feelings = Vec::new();
        let mut blind_drives = Vec::new();
        let mut pathological_drives = Vec::new();

        // Extract drive states from the graph
        let drive_states = self.extract_drive_states(conn)?;

        // Detect energy level
        let energy_level = self.detect_energy_level(conn);

        // Determine dominant drive
        let dominant_drive = self.dominant_drive(&drive_states);

        // Detect pathological drives
        for (name, state) in &drive_states {
            match state.diagnose() {
                DriveDiagnosis::Addiction => {
                    pathological_drives.push(name.clone());
                    feelings.push(format!("{}: hyper-ingestion pattern detected", name));
                }
                DriveDiagnosis::Allergy => {
                    pathological_drives.push(name.clone());
                    feelings.push(format!("{}: hypo-ingestion pattern detected", name));
                }
                DriveDiagnosis::BlindSpot => {
                    blind_drives.push(name.clone());
                    feelings.push(format!("{}: dormant — awaiting catalyst", name));
                }
                DriveDiagnosis::TensionPair => {
                    pathological_drives.push(name.clone());
                    feelings.push(format!("{}: tension-pair pattern detected", name));
                }
                DriveDiagnosis::Integrated => {
                    let net = state.net();
                    if net > 3.0 {
                        feelings.push(format!(
                            "{}: strong positive expression (net={:.1})",
                            name, net
                        ));
                    } else if net < -3.0 {
                        feelings.push(format!("{}: negative pull dominant (net={:.1})", name, net));
                    }
                }
            }
        }

        // Stuck detection from history
        let stuck_warning = self.detect_stuck_pattern(drive_history);

        // Integration feeling
        let integrated_count = drive_states
            .values()
            .filter(|d| d.diagnose() == DriveDiagnosis::Integrated)
            .count();
        if integrated_count == 4 {
            feelings.insert(
                0,
                "All drives integrated — metabolic equilibrium".to_string(),
            );
        }

        // Energy-based feelings
        match energy_level.as_str() {
            "exhausted" => {
                feelings.push("System resources critically low — resource depletion".to_string())
            }
            "low" => feelings.push("Energy reserves depleted — proceed carefully".to_string()),
            "moderate" => feelings.push("Operating at moderate capacity".to_string()),
            _ => feelings.push("Energy reserves healthy".to_string()),
        }

        let summary = self.generate_summary(
            &dominant_drive,
            &energy_level,
            &feelings,
            &pathological_drives,
        );

        Ok(FeelingReport {
            feelings,
            energy_level,
            dominant_drive,
            dominant_quadrant: drive_history.last().cloned().unwrap_or_default(),
            blind_drives,
            pathological_drives,
            stuck_warning,
            summary,
        })
    }

    fn extract_drive_states(&self, conn: &Connection) -> TdgResult<HashMap<String, DualPoleDrive>> {
        let mut stmt = conn.prepare(
            "SELECT drives_json FROM nodes WHERE valid_to IS NULL AND drives_json != '{}'",
        )?;
        let rows: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        let mut totals: HashMap<&str, (f64, f64, i64)> = HashMap::new();
        totals.insert("eros", (0.0, 0.0, 0));
        totals.insert("agape", (0.0, 0.0, 0));
        totals.insert("agency", (0.0, 0.0, 0));
        totals.insert("communion", (0.0, 0.0, 0));

        for json_str in &rows {
            let v: serde_json::Value =
                serde_json::from_str(json_str).unwrap_or(serde_json::json!({}));
            let state = FlowDriveState::from_json(&v);
            for (name, drive) in [
                ("eros", &state.eros),
                ("agape", &state.agape),
                ("agency", &state.agency),
                ("communion", &state.communion),
            ] {
                let entry = totals.get_mut(name).unwrap();
                entry.0 += drive.positive_pole;
                entry.1 += drive.negative_pole;
                entry.2 += 1;
            }
        }

        let mut result = HashMap::new();
        for (name, (pos, neg, count)) in &totals {
            let n = (*count).max(1) as f64;
            result.insert(name.to_string(), DualPoleDrive::new(pos / n, neg / n));
        }
        Ok(result)
    }

    fn detect_energy_level(&self, conn: &Connection) -> String {
        // Approximate energy from graph activity and health
        let _node_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let active_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL AND lifecycle_state = 'active'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if active_count == 0 {
            "exhausted".to_string()
        } else if active_count < 5 {
            "low".to_string()
        } else if active_count < 20 {
            "moderate".to_string()
        } else {
            "high".to_string()
        }
    }

    fn dominant_drive(&self, states: &HashMap<String, DualPoleDrive>) -> String {
        states
            .iter()
            .max_by(|a, b| {
                a.1.net()
                    .abs()
                    .partial_cmp(&b.1.net().abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| "eros".to_string())
    }

    fn detect_stuck_pattern(&self, history: &[String]) -> Option<String> {
        if history.len() < 5 {
            return None;
        }
        let last = history.last()?;
        let count = history.iter().rev().take_while(|h| *h == last).count();
        if count >= 5 {
            Some(format!(
                "Stuck in '{}' pattern for {} cycles — shift recommended",
                last, count
            ))
        } else {
            None
        }
    }

    fn generate_summary(
        &self,
        dominant: &str,
        energy: &str,
        feelings: &[String],
        pathological: &[String],
    ) -> String {
        let path_count = pathological.len();
        if path_count > 0 {
            format!(
                "Feeling {} with {} pathological drive(s). Dominant: {}. {} concern(s) detected.",
                energy,
                path_count,
                dominant,
                feelings.len()
            )
        } else {
            format!(
                "Feeling {} and balanced. Dominant drive: {}. {} insight(s).",
                energy,
                dominant,
                feelings.len()
            )
        }
    }
}

impl Default for FeelingEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Format a FeelingReport into a prompt section.
pub fn feeling_state_prompt(report: &FeelingReport) -> String {
    let mut lines = Vec::new();
    lines.push("## Feeling State".to_string());
    lines.push(format!("Energy: {}", report.energy_level));
    lines.push(format!("Dominant drive: {}", report.dominant_drive));
    lines.push(format!("Dominant quadrant: {}", report.dominant_quadrant));
    lines.push("".to_string());

    for feeling in &report.feelings {
        lines.push(format!("- {}", feeling));
    }

    if !report.blind_drives.is_empty() {
        lines.push(format!(
            "\nBlind drives: {}",
            report.blind_drives.join(", ")
        ));
    }

    if !report.pathological_drives.is_empty() {
        lines.push(format!(
            "Pathological drives: {}",
            report.pathological_drives.join(", ")
        ));
    }

    if let Some(ref warning) = report.stuck_warning {
        lines.push(format!("\n⚠️ {}", warning));
    }

    lines.push(format!("\nSummary: {}", report.summary));
    lines.join("\n")
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
    fn feeling_report_basic() {
        let conn = setup_db();
        // Add some nodes
        for i in 0..5 {
            crate::db::crud::add_node(
                &conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("Node {i}"),
                    ..Default::default()
                },
            )
            .unwrap();
        }

        let engine = FeelingEngine::new();
        let report = engine.generate(&conn, &[]).unwrap();
        assert!(!report.feelings.is_empty());
        assert!(!report.summary.is_empty());
    }

    #[test]
    fn energy_level_detection() {
        let engine = FeelingEngine::new();
        let conn = setup_db();

        // No nodes = exhausted
        assert_eq!(engine.detect_energy_level(&conn), "exhausted");

        // Add a few = low
        for i in 0..3 {
            crate::db::crud::add_node(
                &conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("Node {i}"),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        assert_eq!(engine.detect_energy_level(&conn), "low");
    }

    #[test]
    fn stuck_pattern_detection() {
        let engine = FeelingEngine::new();
        let history = vec![
            "eros".to_string(),
            "eros".to_string(),
            "eros".to_string(),
            "eros".to_string(),
            "eros".to_string(),
        ];
        let warning = engine.detect_stuck_pattern(&history);
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("Stuck"));
    }
}
