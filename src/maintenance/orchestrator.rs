use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{error, info};

use super::archiver::{Archiver, ArchiverReport};
use super::enricher::{Enricher, EnricherReport};
use super::janitor::{Janitor, JanitorReport};
use super::monitor::{HealthMonitor, HealthReport};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfManagerReport {
    pub health_before: Option<HealthReport>,
    pub janitor: Option<JanitorReport>,
    pub enricher: Option<EnricherReport>,
    pub archiver: Option<ArchiverReport>,
    pub health_after: Option<HealthReport>,
    pub health_delta: f64,
    pub dry_run: bool,
    pub timestamp: String,
    pub duration_seconds: f64,
    pub succeeded: Vec<String>,
    pub failed: Vec<String>,
}

pub struct SelfManager<'a> {
    conn: &'a Connection,
}

impl<'a> SelfManager<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn run(&self, dry_run: bool) -> Result<SelfManagerReport> {
        let start = Instant::now();
        let mut report = SelfManagerReport {
            health_before: None,
            janitor: None,
            enricher: None,
            archiver: None,
            health_after: None,
            health_delta: 0.0,
            dry_run,
            timestamp: Utc::now().to_rfc3339(),
            duration_seconds: 0.0,
            succeeded: Vec::new(),
            failed: Vec::new(),
        };

        let monitor = HealthMonitor::new(self.conn);

        match monitor.check() {
            Ok(h) => report.health_before = Some(h),
            Err(e) => error!("HealthMonitor.check() failed: {}", e),
        }

        match Janitor::new(self.conn).run(dry_run) {
            Ok(j) => {
                report.janitor = Some(j);
                report.succeeded.push("janitor".into());
            }
            Err(e) => {
                error!("Janitor.run() failed: {}", e);
                report.failed.push("janitor".into());
            }
        }

        match Enricher::new(self.conn).run(dry_run) {
            Ok(e) => {
                report.enricher = Some(e);
                report.succeeded.push("enricher".into());
            }
            Err(e) => {
                error!("Enricher.run() failed: {}", e);
                report.failed.push("enricher".into());
            }
        }

        match Archiver::new(self.conn).run(dry_run) {
            Ok(a) => {
                report.archiver = Some(a);
                report.succeeded.push("archiver".into());
            }
            Err(e) => {
                error!("Archiver.run() failed: {}", e);
                report.failed.push("archiver".into());
            }
        }

        match monitor.check() {
            Ok(h) => report.health_after = Some(h),
            Err(e) => error!("HealthMonitor.check() failed: {}", e),
        }

        // Advance developmental stages for telos nodes.
        //
        // Previously, `advance_stage` and `promote_tlevel` were dead code —
        // never called from any production path. The entire 8-stage developmental
        // framework (Survival → Identity → Power → Heart → Rational → Pluralistic
        // → Integral → Harvest) was non-functional. We now call advance_stage for
        // all active telos nodes during the SelfManager cycle.
        if !dry_run {
            let telearchy = crate::telearchy::TelearchyEngine::new(self.conn);
            match crate::db::crud::query_nodes(
                self.conn,
                &crate::models::NodeQuery {
                    node_type: Some("telos".to_string()),
                    limit: Some(1000),
                    ..Default::default()
                },
            ) {
                Ok(telos_nodes) => {
                    let mut promoted = 0usize;
                    for telos in &telos_nodes {
                        match telearchy.advance_stage(&telos.id) {
                            Ok(Some(_)) => promoted += 1,
                            Ok(None) => {}
                            Err(e) => {
                                tracing::debug!(
                                    "advance_stage failed for telos {}: {}",
                                    telos.id, e
                                );
                            }
                        }
                    }
                    if promoted > 0 {
                        info!("Telearchy: {} telos nodes advanced to next stage", promoted);
                        report.succeeded.push(format!("telearchy({})", promoted));
                    }
                }
                Err(e) => {
                    error!("Failed to query telos nodes for stage advancement: {}", e);
                    report.failed.push("telearchy".into());
                }
            }
        }

        // Promote high-confidence hypotheses to capabilities.
        //
        // Previously, promote_hypothesis_to_capability was dead code — never
        // called from any production path. The entire hypothesis→capability
        // promotion path (which creates PROMOTES_TO edges and is the entire
        // point of the developmental pipeline) never executed. Hypotheses
        // accumulated forever; capabilities were never auto-created from them.
        //
        // We now promote hypotheses with confidence > 0.7 that have SUPPORTS
        // edges (indicating evidence backing).
        if !dry_run {
            match crate::db::crud::query_nodes(
                self.conn,
                &crate::models::NodeQuery {
                    node_type: Some("hypothesis".to_string()),
                    limit: Some(1000),
                    ..Default::default()
                },
            ) {
                Ok(hypotheses) => {
                    let mut promoted = 0usize;
                    for hyp in &hypotheses {
                        // Only promote hypotheses with confidence > 0.7
                        if hyp.confidence < 0.7 {
                            continue;
                        }
                        // Check if this hypothesis has SUPPORTS edges (evidence backing)
                        let supports_count: i64 = self.conn
                            .query_row(
                                "SELECT COUNT(*) FROM edges WHERE source_id = ?1 AND edge_type = 'SUPPORTS' AND valid_to IS NULL",
                                rusqlite::params![&hyp.id],
                                |r| r.get(0),
                            )
                            .unwrap_or(0);
                        if supports_count < 3 {
                            continue;
                        }
                        // Check if already promoted (has a PROMOTES_TO edge)
                        let already_promoted: i64 = self.conn
                            .query_row(
                                "SELECT COUNT(*) FROM edges WHERE source_id = ?1 AND edge_type = 'PROMOTES_TO' AND valid_to IS NULL",
                                rusqlite::params![&hyp.id],
                                |r| r.get(0),
                            )
                            .unwrap_or(0);
                        if already_promoted > 0 {
                            continue;
                        }
                        let engine = crate::digestion::DigestionEngine::new(self.conn);
                        match engine.promote_hypothesis_to_capability(&hyp.id) {
                            Ok(cap) => {
                                promoted += 1;
                                info!("Promoted hypothesis {} to capability {}", hyp.id, cap.id);
                            }
                            Err(e) => {
                                tracing::debug!("Failed to promote hypothesis {}: {}", hyp.id, e);
                            }
                        }
                    }
                    if promoted > 0 {
                        info!("Digestion: {} hypotheses promoted to capabilities", promoted);
                        report.succeeded.push(format!("hypothesis_promotion({})", promoted));
                    }
                }
                Err(e) => {
                    error!("Failed to query hypotheses for promotion: {}", e);
                    report.failed.push("hypothesis_promotion".into());
                }
            }
        }

        if let (Some(before), Some(after)) = (&report.health_before, &report.health_after) {
            report.health_delta = after.health_score - before.health_score;
        }

        report.duration_seconds = start.elapsed().as_secs_f64();
        Ok(report)
    }
}
