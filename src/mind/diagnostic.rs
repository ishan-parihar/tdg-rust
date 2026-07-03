//! Diagnostic Engine — behavioral pattern analysis and suggestions
//!
//! Port of `core/mind/diagnostic_engine.py`.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::TdgResult;
use crate::flow::FlowDriveState;

/// Severity levels for diagnostic flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Soft,
    Strong,
    Mandatory,
}

/// A single diagnostic pattern flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternFlag {
    pub category: String,
    pub severity: Severity,
    pub message: String,
    pub drive: Option<String>,
    pub quadrant: Option<String>,
}

/// Drive label categories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriveLabel {
    Healthy,
    Pathological,
    Addicted,
    Allergic,
    Blind,
    Conflicted,
}

/// Phantom node detection — drive expressed in wrong quadrant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhantomNode {
    pub node_id: String,
    pub node_name: String,
    pub drive: String,
    pub expected_quadrant: String,
    pub actual_quadrant: String,
    pub confidence: f64,
}

/// Diagnostic report output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub pattern_flags: Vec<PatternFlag>,
    pub drive_distribution: HashMap<String, f64>,
    pub quadrant_distribution: HashMap<String, f64>,
    pub blind_spots: Vec<String>,
    pub persistence_warnings: Vec<String>,
    pub drive_labels: HashMap<String, DriveLabel>,
    pub phantom_nodes: Vec<PhantomNode>,
    pub ghost_nodes: i64,
    pub metrics_staleness: bool,
    pub escalation_level: Severity,
    pub suggestion: String,
}

/// Threshold configuration for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticThresholds {
    pub addiction_positive_min: f64,
    pub allergy_negative_min: f64,
    pub blind_spot_pct: f64,
    pub drive_persistence_soft: i64,
    pub drive_persistence_strong: i64,
    pub drive_persistence_mandatory: i64,
    pub quadrant_imbalance_pct: f64,
    pub quadrant_persistence_cycles: usize,
}

impl Default for DiagnosticThresholds {
    fn default() -> Self {
        Self {
            addiction_positive_min: 7.0,
            allergy_negative_min: 5.0,
            blind_spot_pct: 10.0,
            drive_persistence_soft: 3,
            drive_persistence_strong: 5,
            drive_persistence_mandatory: 8,
            quadrant_imbalance_pct: 40.0,
            quadrant_persistence_cycles: 4,
        }
    }
}

/// The Diagnostic Engine — retrospective dashboard for agent self-awareness.
pub struct DiagnosticEngine {
    thresholds: DiagnosticThresholds,
}

impl DiagnosticEngine {
    pub fn new() -> Self {
        Self {
            thresholds: DiagnosticThresholds::default(),
        }
    }

    pub fn with_thresholds(thresholds: DiagnosticThresholds) -> Self {
        Self { thresholds }
    }

    /// Analyze the current state and produce a diagnostic report.
    pub fn analyze(
        &self,
        conn: &Connection,
        drive_history: &[String],
        quadrant_history: &[String],
    ) -> TdgResult<DiagnosticReport> {
        let mut flags = Vec::new();
        let mut persistence_warnings = Vec::new();

        // 1. Drive distribution analysis
        let drive_dist = self.compute_drive_distribution(conn)?;
        let quadrant_dist = self.compute_quadrant_distribution(quadrant_history);

        // 2. Pattern detection — label drive patterns
        let drive_flags = self.label_drive_patterns(conn)?;
        flags.extend(drive_flags);

        // 3. Quadrant imbalance detection
        if let Some(q_flags) = self.detect_quadrant_imbalance(&quadrant_dist) {
            flags.extend(q_flags);
        }

        // 4. Blind spot detection
        let blind_spots = self.detect_blind_spots(&quadrant_dist);

        // 5. Drive persistence warnings
        if let Some(p_warnings) = self.detect_drive_persistence(drive_history) {
            persistence_warnings.extend(p_warnings);
        }

        // 6. Quadrant repetition persistence
        if let Some(q_warnings) = self.detect_quadrant_persistence(quadrant_history) {
            persistence_warnings.extend(q_warnings);
        }

        // 7. Ghost nodes (unclassified)
        let ghost_nodes = self.count_ghost_nodes(conn)?;

        // 8. Drive label categorization
        let drive_labels = self.categorize_drive_labels(&drive_dist);

        // 9. Phantom node detection (eros in wrong quadrant)
        let phantom_nodes = self.detect_phantom_nodes(conn)?;

        // 10. Metrics staleness check
        let metrics_staleness = self.check_metrics_staleness(conn)?;

        // 11. Escalation level
        let escalation_level = self.compute_escalation_level(&flags, metrics_staleness);

        // 12. Generate suggestion
        let suggestion = self.generate_suggestion(&flags, &blind_spots);

        Ok(DiagnosticReport {
            pattern_flags: flags,
            drive_distribution: drive_dist,
            quadrant_distribution: quadrant_dist,
            blind_spots,
            persistence_warnings,
            drive_labels,
            phantom_nodes,
            ghost_nodes,
            metrics_staleness,
            escalation_level,
            suggestion,
        })
    }

    fn compute_drive_distribution(&self, conn: &Connection) -> TdgResult<HashMap<String, f64>> {
        let mut stmt = conn.prepare(
            "SELECT drives_json FROM nodes WHERE valid_to IS NULL AND drives_json != '{}'",
        )?;
        let rows: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        let mut totals: HashMap<&str, (f64, f64)> = HashMap::new();
        totals.insert("eros", (0.0, 0.0));
        totals.insert("agape", (0.0, 0.0));
        totals.insert("agency", (0.0, 0.0));
        totals.insert("communion", (0.0, 0.0));

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
            }
        }

        let count = rows.len().max(1) as f64;
        let mut result = HashMap::new();
        for (name, (pos, neg)) in &totals {
            result.insert(name.to_string(), (pos / count) - (neg / count));
        }
        Ok(result)
    }

    fn compute_quadrant_distribution(&self, history: &[String]) -> HashMap<String, f64> {
        let total = history.len().max(1) as f64;
        let mut counts: HashMap<String, f64> = HashMap::new();
        for q in history {
            *counts.entry(q.clone()).or_insert(0.0) += 1.0;
        }
        for v in counts.values_mut() {
            *v = (*v / total) * 100.0;
        }
        counts
    }

    fn label_drive_patterns(&self, conn: &Connection) -> TdgResult<Vec<PatternFlag>> {
        let mut flags = Vec::new();
        let drive_dist = self.compute_drive_distribution(conn)?;

        // Also compute pos/neg averages in one pass
        let mut stmt = conn.prepare(
            "SELECT drives_json FROM nodes WHERE valid_to IS NULL AND drives_json != '{}'",
        )?;
        let rows: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        let mut pos_avgs: HashMap<&str, f64> = HashMap::new();
        let mut neg_avgs: HashMap<&str, f64> = HashMap::new();
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
                *pos_avgs.entry(name).or_insert(0.0) += drive.positive_pole;
                *neg_avgs.entry(name).or_insert(0.0) += drive.negative_pole;
            }
        }
        let n = rows.len().max(1) as f64;
        for v in pos_avgs.values_mut() {
            *v /= n;
        }
        for v in neg_avgs.values_mut() {
            *v /= n;
        }

        for (name, &net) in &drive_dist {
            let pos_avg = pos_avgs.get(name.as_str()).copied().unwrap_or(0.0);
            let neg_avg = neg_avgs.get(name.as_str()).copied().unwrap_or(0.0);

            if pos_avg > self.thresholds.addiction_positive_min {
                flags.push(PatternFlag {
                    category: "drive_pathology".to_string(),
                    severity: Severity::Strong,
                    message: format!("{name}: addiction pattern detected (pos_avg={pos_avg:.1})"),
                    drive: Some(name.clone()),
                    quadrant: None,
                });
            }
            if neg_avg > self.thresholds.allergy_negative_min {
                flags.push(PatternFlag {
                    category: "drive_pathology".to_string(),
                    severity: Severity::Strong,
                    message: format!("{name}: allergy pattern detected (neg_avg={neg_avg:.1})"),
                    drive: Some(name.clone()),
                    quadrant: None,
                });
            }
            if net.abs() < 0.5 {
                flags.push(PatternFlag {
                    category: "drive_blind_spot".to_string(),
                    severity: Severity::Soft,
                    message: format!("{name}: near-dormant (net={net:.1})"),
                    drive: Some(name.clone()),
                    quadrant: None,
                });
            }
        }
        Ok(flags)
    }

    fn detect_quadrant_imbalance(&self, dist: &HashMap<String, f64>) -> Option<Vec<PatternFlag>> {
        let mut flags = Vec::new();
        for (q, &pct) in dist {
            if pct > self.thresholds.quadrant_imbalance_pct {
                flags.push(PatternFlag {
                    category: "quadrant_imbalance".to_string(),
                    severity: Severity::Strong,
                    message: format!(
                        "{q} at {pct:.0}% — exceeds {}% threshold",
                        self.thresholds.quadrant_imbalance_pct
                    ),
                    drive: None,
                    quadrant: Some(q.clone()),
                });
            }
        }
        if flags.is_empty() {
            None
        } else {
            Some(flags)
        }
    }

    fn detect_blind_spots(&self, dist: &HashMap<String, f64>) -> Vec<String> {
        let mut spots = Vec::new();
        for q in &["UL", "UR", "LL", "LR"] {
            let pct = dist.get(q.to_owned()).copied().unwrap_or(0.0);
            if pct < self.thresholds.blind_spot_pct {
                spots.push(q.to_string());
            }
        }
        spots
    }

    fn detect_drive_persistence(&self, history: &[String]) -> Option<Vec<String>> {
        if history.is_empty() {
            return None;
        }
        let mut warnings = Vec::new();
        let last = history.last()?;
        let count = history.iter().rev().take_while(|h| *h == last).count() as i64;

        if count >= self.thresholds.drive_persistence_mandatory {
            warnings.push(format!(
                "🔴 MANDATORY: '{last}' sustained for {count} cycles — shift required"
            ));
        } else if count >= self.thresholds.drive_persistence_strong {
            warnings.push(format!(
                "🟠 STRONG: '{last}' sustained for {count} cycles — consider shifting"
            ));
        } else if count >= self.thresholds.drive_persistence_soft {
            warnings.push(format!(
                "🟡 SOFT: '{last}' sustained for {count} cycles — monitor"
            ));
        }

        if warnings.is_empty() {
            None
        } else {
            Some(warnings)
        }
    }

    fn detect_quadrant_persistence(&self, history: &[String]) -> Option<Vec<String>> {
        if history.len() < self.thresholds.quadrant_persistence_cycles {
            return None;
        }
        let window = &history[history.len() - self.thresholds.quadrant_persistence_cycles..];
        let first = window.first()?;
        let count = window.iter().filter(|q| *q == first).count();

        if count == window.len() {
            Some(vec![format!(
                "⚠️ Quadrant '{}' repeated for {} consecutive cycles",
                first,
                window.len()
            )])
        } else {
            None
        }
    }

    fn count_ghost_nodes(&self, conn: &Connection) -> TdgResult<i64> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL AND quadrants_json = '{}'",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    fn categorize_drive_labels(
        &self,
        drive_dist: &HashMap<String, f64>,
    ) -> HashMap<String, DriveLabel> {
        let mut labels = HashMap::new();
        for (name, &net) in drive_dist {
            let label = if net.abs() < 0.5 {
                DriveLabel::Blind
            } else if net > 7.0 {
                DriveLabel::Addicted
            } else if net < -5.0 {
                DriveLabel::Allergic
            } else if net.abs() > 8.0 {
                DriveLabel::Pathological
            } else {
                DriveLabel::Healthy
            };
            labels.insert(name.clone(), label);
        }
        labels
    }

    fn detect_phantom_nodes(&self, conn: &Connection) -> TdgResult<Vec<PhantomNode>> {
        let mut stmt = conn.prepare(
            "SELECT id, name, drives_json, quadrants_json FROM nodes WHERE valid_to IS NULL AND node_type = 'observation'",
        )?;
        let rows: Vec<(String, String, String, String)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut phantoms = Vec::new();
        // Each quadrant is associated with a primary drive. A "phantom node" is
        // an observation whose drive state is strong in a drive that does NOT
        // match its recorded quadrant — e.g. a UL observation (agape) with a
        // very strong `eros` signal is "phantom": it's acting like an LR node
        // while labeled UL.
        let quadrant_drive: HashMap<&str, &str> = HashMap::from([
            ("UL", "agape"),
            ("UR", "agency"),
            ("LL", "communion"),
            ("LR", "eros"),
        ]);

        for (id, name, drives_json, quadrants_json) in &rows {
            // drives_json is stored as {"eros": {"positive_pole": 5.0, "negative_pole": 2.0}, ...}
            // (see flow.rs FlowDriveState::to_json). The previous implementation
            // did `drives.get("eros").as_f64()` which always returned None
            // (because the value is an object, not a float), so phantom_nodes
            // was ALWAYS empty. We now compute the net drive = positive - negative.
            let drives: serde_json::Value = match serde_json::from_str(drives_json) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        "detect_phantom_nodes: corrupt drives_json for node {} ({}): {}",
                        id, name, e
                    );
                    continue;
                }
            };
            let quadrants: serde_json::Value =
                serde_json::from_str(quadrants_json).unwrap_or(serde_json::json!({}));

            let active_quadrant = quadrants
                .get("primary")
                .or_else(|| quadrants.get("active"))
                .and_then(|v| v.as_str())
                .unwrap_or("LR");

            let expected_drive = quadrant_drive.get(active_quadrant).copied().unwrap_or("eros");

            for (drive_name, _drive_label) in &[
                ("eros", "Eros"),
                ("agape", "Agape"),
                ("agency", "Agency"),
                ("communion", "Communion"),
            ] {
                let drive_obj = match drives.get(*drive_name) {
                    Some(v) => v,
                    None => continue,
                };
                let pos = drive_obj
                    .get("positive_pole")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let neg = drive_obj
                    .get("negative_pole")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let net = pos - neg;

                // Phantom: strong drive activity (>7.0 net magnitude) in a
                // drive that does NOT correspond to this node's quadrant.
                if drive_name != &expected_drive && net.abs() > 7.0 {
                    // Find which quadrant this drive actually belongs to.
                    let actual_quadrant = quadrant_drive
                        .iter()
                        .find(|(_, d)| *d == drive_name)
                        .map(|(q, _)| *q)
                        .unwrap_or("LR");
                    phantoms.push(PhantomNode {
                        node_id: id.clone(),
                        node_name: name.clone(),
                        drive: drive_name.to_string(),
                        expected_quadrant: active_quadrant.to_string(),
                        actual_quadrant: actual_quadrant.to_string(),
                        confidence: net.abs() / 10.0,
                    });
                }
            }
        }

        Ok(phantoms)
    }

    fn check_metrics_staleness(&self, conn: &Connection) -> TdgResult<bool> {
        // Previously this queried for a `metrics_updated` event that no code
        // path ever writes, causing staleness to be permanently `true` and the
        // diagnostic dashboard to always show "⚠️ Metrics data is stale".
        //
        // We now use the most recent `enrichment_completed` event (written by
        // the Enricher at the end of every run) as the freshness signal. If
        // none exists, we fall back to the most recent `node_updated` event —
        // if the graph hasn't seen any mutation in 24h, metrics are stale.
        let result: Option<String> = conn
            .query_row(
                "SELECT timestamp FROM events
                 WHERE event_action IN ('enrichment_completed', 'node_updated', 'node_created')
                 ORDER BY timestamp DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        match result {
            Some(timestamp_str) => {
                // Events are written with either RFC3339 (chrono::Utc::now().to_rfc3339())
                // or strftime('%Y-%m-%dT%H:%M:%SZ', 'now'). Try parsing both.
                let parsed = chrono::NaiveDateTime::parse_from_str(
                    &timestamp_str,
                    "%Y-%m-%dT%H:%M:%S%.fZ",
                )
                .or_else(|_| {
                    chrono::NaiveDateTime::parse_from_str(&timestamp_str, "%Y-%m-%dT%H:%M:%SZ")
                })
                .or_else(|_| {
                    chrono::NaiveDateTime::parse_from_str(&timestamp_str, "%Y-%m-%dT%H:%M:%S%.f")
                })
                .or_else(|_| {
                    chrono::NaiveDateTime::parse_from_str(&timestamp_str, "%Y-%m-%dT%H:%M:%S")
                });
                match parsed {
                    Ok(ts) => {
                        let now = chrono::Utc::now().naive_utc();
                        let hours = (now - ts).num_hours();
                        Ok(hours > 24)
                    }
                    Err(_) => Ok(true),
                }
            }
            None => Ok(true),
        }
    }

    fn compute_escalation_level(&self, flags: &[PatternFlag], metrics_staleness: bool) -> Severity {
        let has_mandatory = flags.iter().any(|f| f.severity == Severity::Mandatory);
        let has_strong = flags.iter().any(|f| f.severity == Severity::Strong);
        let pathology_count = flags
            .iter()
            .filter(|f| f.category == "drive_pathology")
            .count();

        if has_mandatory || pathology_count >= 3 {
            Severity::Mandatory
        } else if has_strong || metrics_staleness || pathology_count >= 2 {
            Severity::Strong
        } else {
            Severity::Soft
        }
    }

    pub fn diagnostic_prompt_section(&self, report: &DiagnosticReport) -> String {
        let mut section = String::new();
        section.push_str("## Diagnostic Dashboard\n\n");
        section.push_str(&format!("**Escalation**: {:?}\n", report.escalation_level));
        section.push_str(&format!("**Suggestion**: {}\n\n", report.suggestion));

        section.push_str("### Drive Labels\n");
        for (drive, label) in &report.drive_labels {
            section.push_str(&format!("- {}: {:?}\n", drive, label));
        }

        if !report.phantom_nodes.is_empty() {
            section.push_str("\n### Phantom Nodes\n");
            for p in &report.phantom_nodes {
                section.push_str(&format!(
                    "- {} ({}) in {} — {}\n",
                    p.node_name, p.drive, p.actual_quadrant, p.expected_quadrant
                ));
            }
        }

        if !report.blind_spots.is_empty() {
            section.push_str(&format!("\n### Blind Spots: {:?}\n", report.blind_spots));
        }

        if report.metrics_staleness {
            section.push_str("\n⚠️ Metrics data is stale (>24h)\n");
        }

        section
    }

    fn generate_suggestion(&self, flags: &[PatternFlag], blind_spots: &[String]) -> String {
        let mandatory_count = flags
            .iter()
            .filter(|f| f.severity == Severity::Mandatory)
            .count();
        let strong_count = flags
            .iter()
            .filter(|f| f.severity == Severity::Strong)
            .count();

        if mandatory_count > 0 {
            "⚠️ MANDATORY: Address critical drive/quadrant imbalances immediately before proceeding."
                .to_string()
        } else if strong_count > 0 {
            format!(
                "Review {} strong signals. Consider shifting focus to underrepresented areas: {:?}",
                strong_count, blind_spots
            )
        } else if !blind_spots.is_empty() {
            format!("Explore blind spots: {:?}", blind_spots)
        } else {
            "System nominal — maintain current trajectory.".to_string()
        }
    }
}

impl Default for DiagnosticEngine {
    fn default() -> Self {
        Self::new()
    }
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
    fn diagnostic_report_basic() {
        let conn = setup_db();
        crate::db::crud::add_node(
            &conn,
            &crate::models::NewNode {
                node_type: "observation".to_string(),
                name: "Test".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let engine = DiagnosticEngine::new();
        let quadrant_history = vec![
            "UL".to_string(),
            "UR".to_string(),
            "LL".to_string(),
            "LR".to_string(),
        ];
        let report = engine.analyze(&conn, &[], &quadrant_history).unwrap();
        assert!(report.suggestion.contains("nominal"));
        assert_eq!(report.ghost_nodes, 1);
        assert!(!report.drive_labels.is_empty());
    }

    #[test]
    fn drive_persistence_detection() {
        let engine = DiagnosticEngine::new();
        let history = vec![
            "eros".to_string(),
            "eros".to_string(),
            "eros".to_string(),
            "eros".to_string(),
            "eros".to_string(),
        ];
        let warnings = engine.detect_drive_persistence(&history).unwrap();
        assert!(warnings.iter().any(|w| w.contains("STRONG")));
    }

    #[test]
    fn blind_spot_detection() {
        let engine = DiagnosticEngine::new();
        let mut dist = HashMap::new();
        dist.insert("UL".to_string(), 50.0);
        dist.insert("UR".to_string(), 50.0);
        dist.insert("LL".to_string(), 0.0);
        dist.insert("LR".to_string(), 0.0);

        let spots = engine.detect_blind_spots(&dist);
        assert!(spots.contains(&"LL".to_string()));
        assert!(spots.contains(&"LR".to_string()));
    }

    #[test]
    fn drive_label_categorization() {
        let engine = DiagnosticEngine::new();
        let mut dist = HashMap::new();
        dist.insert("eros".to_string(), 8.0);
        dist.insert("agape".to_string(), -6.0);
        dist.insert("agency".to_string(), 3.0);
        dist.insert("communion".to_string(), 0.3);

        let labels = engine.categorize_drive_labels(&dist);
        assert_eq!(labels.get("eros").unwrap(), &DriveLabel::Addicted);
        assert_eq!(labels.get("agape").unwrap(), &DriveLabel::Allergic);
        assert_eq!(labels.get("agency").unwrap(), &DriveLabel::Healthy);
        assert_eq!(labels.get("communion").unwrap(), &DriveLabel::Blind);
    }

    #[test]
    fn escalation_level_computation() {
        let engine = DiagnosticEngine::new();
        let flags = vec![
            PatternFlag {
                category: "drive_pathology".to_string(),
                severity: Severity::Strong,
                message: "test".to_string(),
                drive: Some("eros".to_string()),
                quadrant: None,
            },
            PatternFlag {
                category: "drive_pathology".to_string(),
                severity: Severity::Strong,
                message: "test2".to_string(),
                drive: Some("agape".to_string()),
                quadrant: None,
            },
        ];
        let level = engine.compute_escalation_level(&flags, false);
        assert_eq!(level, Severity::Strong);

        let flags3 = vec![
            PatternFlag {
                category: "drive_pathology".to_string(),
                severity: Severity::Strong,
                message: "test".to_string(),
                drive: Some("eros".to_string()),
                quadrant: None,
            },
            PatternFlag {
                category: "drive_pathology".to_string(),
                severity: Severity::Strong,
                message: "test2".to_string(),
                drive: Some("agape".to_string()),
                quadrant: None,
            },
            PatternFlag {
                category: "drive_pathology".to_string(),
                severity: Severity::Strong,
                message: "test3".to_string(),
                drive: Some("agency".to_string()),
                quadrant: None,
            },
        ];
        let level3 = engine.compute_escalation_level(&flags3, false);
        assert_eq!(level3, Severity::Mandatory);
    }

    #[test]
    fn diagnostic_prompt_section_output() {
        let engine = DiagnosticEngine::new();
        let mut labels = HashMap::new();
        labels.insert("eros".to_string(), DriveLabel::Healthy);
        let report = DiagnosticReport {
            pattern_flags: vec![],
            drive_distribution: HashMap::new(),
            quadrant_distribution: HashMap::new(),
            blind_spots: vec![],
            persistence_warnings: vec![],
            drive_labels: labels,
            phantom_nodes: vec![],
            ghost_nodes: 0,
            metrics_staleness: false,
            escalation_level: Severity::Soft,
            suggestion: "System nominal".to_string(),
        };
        let section = engine.diagnostic_prompt_section(&report);
        assert!(section.contains("Diagnostic Dashboard"));
        assert!(section.contains("Drive Labels"));
    }

    #[test]
    fn metrics_staleness_check() {
        let conn = setup_db();
        let engine = DiagnosticEngine::new();
        let stale = engine.check_metrics_staleness(&conn).unwrap();
        assert!(stale);
    }
}
