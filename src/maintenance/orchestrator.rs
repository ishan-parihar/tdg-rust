use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::error;

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

        if let (Some(before), Some(after)) = (&report.health_before, &report.health_after) {
            report.health_delta = after.health_score - before.health_score;
        }

        report.duration_seconds = start.elapsed().as_secs_f64();
        Ok(report)
    }
}
