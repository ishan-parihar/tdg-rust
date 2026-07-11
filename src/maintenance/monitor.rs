use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::mcp::helpers::calculate_health_score;

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
        let node_count = self.active_node_count();
        let edge_count = self.total_edge_count();
        let type_count: i64 = self.conn
            .query_row(
                "SELECT COUNT(DISTINCT node_type) FROM nodes WHERE valid_to IS NULL AND lifecycle_state != 'archived'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let embedding_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM embeddings e
                 INNER JOIN nodes n ON n.id = e.node_id
                 WHERE n.valid_to IS NULL AND n.lifecycle_state != 'archived'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        // IMPORTANT: join nodes_fts with nodes and filter for active, non-archived
        // rows. The previous `SELECT COUNT(*) FROM nodes_fts` included archived and
        // soft-deleted nodes (FTS triggers fire on every INSERT regardless of
        // lifecycle_state), causing `fts5_coverage` to exceed 1.0 (e.g. 200% when
        // half the nodes were archived).
        let fts_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM nodes_fts f
                 INNER JOIN nodes n ON n.rowid = f.rowid
                 WHERE n.valid_to IS NULL AND n.lifecycle_state != 'archived'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let fts5 = if node_count > 0 {
            fts_count as f64 / node_count as f64
        } else {
            1.0
        };
        let embedding = if node_count > 0 {
            embedding_count as f64 / node_count as f64
        } else {
            1.0
        };
        let drive = self.check_drive_coverage();
        let stage = self.check_stage_coverage();
        let noise = self.check_edge_noise();
        let orphans = self.check_orphan_count();
        let growth = self.check_event_growth();
        let db_size = self.check_db_size();

        let health_score = calculate_health_score(
            node_count,
            edge_count,
            type_count,
            embedding_count,
            fts_count,
        );
        let report = HealthReport {
            fts5_coverage: fts5,
            embedding_coverage: embedding,
            drive_coverage: drive,
            stage_coverage: stage,
            edge_noise: noise,
            orphan_count: orphans,
            event_growth_rate: growth,
            db_size_bytes: db_size,
            health_score,
            actions: vec![],
            timestamp: Utc::now().to_rfc3339(),
        };

        let actions = determine_actions(&report);

        Ok(HealthReport { actions, ..report })
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
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE valid_to IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0)
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

    // Stage coverage: T0 (value 0) is a valid developmental stage for "being" nodes.
    // Coverage checks IS NOT NULL to include T0 nodes, since 0 is explicitly assigned
    // by the enricher (stage_by_type() in enricher.rs).
    fn check_stage_coverage(&self) -> f64 {
        let active = self.active_node_count();
        if active == 0 {
            return 1.0;
        }
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM nodes
                 WHERE developmental_stage IS NOT NULL
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
        // An orphan edge is one whose source OR target node has been
        // hard-deleted (no row at all) — NOT one whose endpoint is merely
        // archived. Archived nodes are intentionally retained; their edges
        // should be cleaned up by the archiver, not flagged as orphans here.
        // The previous query counted edges to archived nodes as orphans,
        // permanently depressing the health score after any archive operation.
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM edges e
                 WHERE e.valid_to IS NULL
                 AND (
                     NOT EXISTS (SELECT 1 FROM nodes n WHERE n.id = e.source_id)
                  OR NOT EXISTS (SELECT 1 FROM nodes n WHERE n.id = e.target_id)
                 )",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0)
    }

    fn check_event_growth(&self) -> f64 {
        // Wrap both sides in `datetime()` so the comparison is format-agnostic.
        // Events are written with either RFC3339 ("2026-01-15T12:34:56.789+00:00")
        // or strftime('%Y-%m-%dT%H:%M:%SZ', 'now') ("2026-01-15T12:34:56Z").
        // The previous query used `datetime('now', '-1 day')` which produces
        // "2026-01-14 12:34:56" (space separator). Lexicographic comparison
        // between "2026-01-14T11:00:00Z" and "2026-01-14 12:34:56" sorts the
        // event string AFTER the cutoff (T=0x54 > space=0x20), causing events
        // from 25h ago to be counted as "within the last day".
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE datetime(timestamp) >= datetime('now', '-1 day')",
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
