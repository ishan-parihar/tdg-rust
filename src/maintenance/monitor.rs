use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub module: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub fts5_coverage: f64,
    pub embedding_coverage: f64,
    pub drive_coverage: f64,
    pub stage_coverage: f64,
    pub edge_noise: f64,
    pub orphan_count: i64,
    pub event_growth_rate: f64,
    pub db_size_bytes: i64,
    pub health_score: f64,
    pub actions: Vec<Action>,
    pub timestamp: String,
}

pub struct HealthMonitor<'a> {
    conn: &'a Connection,
}

impl<'a> HealthMonitor<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn check(&self) -> Result<HealthReport> {
        let fts5 = self.check_fts5_coverage();
        let embedding = self.check_embedding_coverage();
        let drive = self.check_drive_coverage();
        let stage = self.check_stage_coverage();
        let noise = self.check_edge_noise();
        let orphans = self.check_orphan_count();
        let growth = self.check_event_growth();
        let db_size = self.check_db_size();

        let report = HealthReport {
            fts5_coverage: fts5,
            embedding_coverage: embedding,
            drive_coverage: drive,
            stage_coverage: stage,
            edge_noise: noise,
            orphan_count: orphans,
            event_growth_rate: growth,
            db_size_bytes: db_size,
            health_score: 0.0,
            actions: vec![],
            timestamp: Utc::now().to_rfc3339(),
        };

        let health_score = compute_score(&report);
        let actions = determine_actions(&report);

        Ok(HealthReport {
            health_score,
            actions,
            ..report
        })
    }

    fn active_node_count(&self) -> i64 {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE lifecycle_state != 'archived' AND valid_to IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0)
    }

    fn total_edge_count(&self) -> i64 {
        self.conn
            .query_row("SELECT COUNT(*) FROM edges WHERE valid_to IS NULL", [], |r| {
                r.get(0)
            })
            .unwrap_or(0)
    }

    fn check_fts5_coverage(&self) -> f64 {
        let active = self.active_node_count();
        if active == 0 {
            return 1.0;
        }
        let fts_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM nodes_fts", [], |r| r.get(0))
            .unwrap_or(0);
        fts_count as f64 / active as f64
    }

    fn check_embedding_coverage(&self) -> f64 {
        let active = self.active_node_count();
        if active == 0 {
            return 1.0;
        }
        let emb_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM embeddings", [], |r| r.get(0))
            .unwrap_or(0);
        emb_count as f64 / active as f64
    }

    fn check_drive_coverage(&self) -> f64 {
        let active = self.active_node_count();
        if active == 0 {
            return 1.0;
        }
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM nodes
                 WHERE drives_json IS NOT NULL AND drives_json != '{}' AND drives_json != ''
                 AND lifecycle_state != 'archived' AND valid_to IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        count as f64 / active as f64
    }

    fn check_stage_coverage(&self) -> f64 {
        let active = self.active_node_count();
        if active == 0 {
            return 1.0;
        }
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM nodes
                 WHERE developmental_stage IS NOT NULL AND developmental_stage != 0
                 AND lifecycle_state != 'archived' AND valid_to IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        count as f64 / active as f64
    }

    fn check_edge_noise(&self) -> f64 {
        let total = self.total_edge_count();
        if total == 0 {
            return 0.0;
        }
        let mentions: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE edge_type = 'MENTIONS' AND valid_to IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        mentions as f64 / total as f64
    }

    fn check_orphan_count(&self) -> i64 {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM edges e
                 WHERE e.valid_to IS NULL
                 AND (e.source_id NOT IN (SELECT id FROM nodes WHERE valid_to IS NULL AND lifecycle_state != 'archived')
                  OR e.target_id NOT IN (SELECT id FROM nodes WHERE valid_to IS NULL AND lifecycle_state != 'archived'))",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0)
    }

    fn check_event_growth(&self) -> f64 {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE timestamp >= datetime('now', '-1 day')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        count as f64
    }

    fn check_db_size(&self) -> i64 {
        let pages: i64 = self
            .conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap_or(0);
        let page_size: i64 = self
            .conn
            .query_row("PRAGMA page_size", [], |r| r.get(0))
            .unwrap_or(0);
        pages * page_size
    }
}

fn compute_score(report: &HealthReport) -> f64 {
    let orphan_ratio = 1.0 - (report.orphan_count as f64 / 1000.0).min(1.0);
    report.fts5_coverage * 0.25
        + report.embedding_coverage * 0.25
        + report.drive_coverage * 0.15
        + report.stage_coverage * 0.10
        + (1.0 - report.edge_noise) * 0.15
        + orphan_ratio * 0.10
}

fn determine_actions(report: &HealthReport) -> Vec<Action> {
    let mut actions = Vec::new();
    if report.fts5_coverage < 0.9 {
        actions.push(Action {
            module: "janitor".into(),
            reason: "FTS5 coverage low".into(),
        });
    }
    if report.embedding_coverage < 0.9 {
        actions.push(Action {
            module: "enricher".into(),
            reason: "Embedding coverage low".into(),
        });
    }
    if report.drive_coverage < 0.5 {
        actions.push(Action {
            module: "enricher".into(),
            reason: "Drive coverage low".into(),
        });
    }
    if report.stage_coverage < 0.5 {
        actions.push(Action {
            module: "enricher".into(),
            reason: "Stage coverage low".into(),
        });
    }
    if report.edge_noise > 0.8 {
        actions.push(Action {
            module: "archiver".into(),
            reason: "Edge noise high".into(),
        });
    }
    if report.orphan_count > 100 {
        actions.push(Action {
            module: "janitor".into(),
            reason: "Orphan count high".into(),
        });
    }
    if report.event_growth_rate > 1000.0 {
        actions.push(Action {
            module: "archiver".into(),
            reason: "Event growth high".into(),
        });
    }
    if report.db_size_bytes > 100 * 1024 * 1024 {
        actions.push(Action {
            module: "archiver".into(),
            reason: "DB size large".into(),
        });
    }
    actions
}
