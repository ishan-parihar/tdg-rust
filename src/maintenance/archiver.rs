use anyhow::Result;
use chrono::{Duration, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiverReport {
    pub events_archived: i64,
    pub edges_pruned: i64,
    pub mentions_compressed: i64,
    pub vacuum_freed_bytes: i64,
    pub dry_run: bool,
    pub timestamp: String,
}

pub struct Archiver<'a> {
    conn: &'a Connection,
}

impl<'a> Archiver<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn run(&self, dry_run: bool) -> Result<ArchiverReport> {
        let mut report = ArchiverReport {
            events_archived: 0,
            edges_pruned: 0,
            mentions_compressed: 0,
            vacuum_freed_bytes: 0,
            dry_run,
            timestamp: Utc::now().to_rfc3339(),
        };

        info!("Archiver starting (dry_run={})", dry_run);

        report.events_archived = self.archive_old_events(dry_run, 90);
        report.edges_pruned = self.prune_dead_edges(dry_run);
        report.mentions_compressed = self.compress_mentions(dry_run);
        report.vacuum_freed_bytes = self.vacuum(dry_run);

        info!("Archiver finished: {}", report_summary(&report));
        Ok(report)
    }

    fn archive_old_events(&self, dry_run: bool, keep_days: i64) -> i64 {
        let result = (|| -> Result<i64> {
            let cutoff = (Utc::now() - Duration::days(keep_days)).to_rfc3339();
            let count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM events WHERE timestamp < ?1",
                rusqlite::params![cutoff],
                |r| r.get(0),
            )?;

            if count == 0 {
                info!("Events: nothing older than {} days", keep_days);
                return Ok(0);
            }

            if dry_run {
                info!("Events: {} would be archived (dry run)", count);
                return Ok(count);
            }

            self.conn
                .execute("DELETE FROM events WHERE timestamp < ?1", rusqlite::params![cutoff])?;
            info!("Events: {} archived", count);
            Ok(count)
        })();

        result.unwrap_or_else(|e| {
            warn!("Event archival failed: {}", e);
            0
        })
    }

    fn prune_dead_edges(&self, dry_run: bool) -> i64 {
        let result = (|| -> Result<i64> {
            let count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM edges e
                 WHERE e.valid_to IS NULL
                 AND (
                     NOT EXISTS (SELECT 1 FROM nodes n WHERE n.id = e.source_id AND n.valid_to IS NULL)
                     OR NOT EXISTS (SELECT 1 FROM nodes n WHERE n.id = e.target_id AND n.valid_to IS NULL)
                 )",
                [],
                |r| r.get(0),
            )?;

            if count == 0 {
                info!("Edges: no dead edges found");
                return Ok(0);
            }

            if dry_run {
                info!("Edges: {} dead edges would be pruned (dry run)", count);
                return Ok(count);
            }

            self.conn.execute(
                "DELETE FROM edges
                 WHERE valid_to IS NULL
                 AND (
                     NOT EXISTS (SELECT 1 FROM nodes n WHERE n.id = edges.source_id AND n.valid_to IS NULL)
                     OR NOT EXISTS (SELECT 1 FROM nodes n WHERE n.id = edges.target_id AND n.valid_to IS NULL)
                 )",
                [],
            )?;
            info!("Edges: {} dead edges pruned", count);
            Ok(count)
        })();

        result.unwrap_or_else(|e| {
            warn!("Dead edge pruning failed: {}", e);
            0
        })
    }

    fn compress_mentions(&self, dry_run: bool) -> i64 {
        let result = (|| -> Result<i64> {
            let count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM edges e
                 WHERE e.edge_type = 'MENTIONS'
                 AND e.valid_to IS NULL
                 AND EXISTS (
                     SELECT 1 FROM nodes ns WHERE ns.id = e.source_id AND ns.lifecycle_state = 'archived'
                 )
                 AND EXISTS (
                     SELECT 1 FROM nodes nt WHERE nt.id = e.target_id AND nt.lifecycle_state = 'archived'
                 )",
                [],
                |r| r.get(0),
            )?;

            if count == 0 {
                info!("Mentions: no compressible edges found");
                return Ok(0);
            }

            if dry_run {
                info!("Mentions: {} edges would be compressed (dry run)", count);
                return Ok(count);
            }

            self.conn.execute(
                "DELETE FROM edges
                 WHERE edge_type = 'MENTIONS'
                 AND valid_to IS NULL
                 AND EXISTS (
                     SELECT 1 FROM nodes ns WHERE ns.id = edges.source_id AND ns.lifecycle_state = 'archived'
                 )
                 AND EXISTS (
                     SELECT 1 FROM nodes nt WHERE nt.id = edges.target_id AND nt.lifecycle_state = 'archived'
                 )",
                [],
            )?;
            info!("Mentions: {} edges compressed", count);
            Ok(count)
        })();

        result.unwrap_or_else(|e| {
            warn!("Mentions compression failed: {}", e);
            0
        })
    }

    fn vacuum(&self, dry_run: bool) -> i64 {
        let result = (|| -> Result<i64> {
            let pages_before: i64 = self.conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
            let page_size: i64 = self.conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
            let size_before = pages_before * page_size;

            if dry_run {
                info!("Vacuum: would reclaim space (dry run)");
                return Ok(0);
            }

            self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
            self.conn.execute_batch("VACUUM")?;

            let pages_after: i64 = self.conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
            let size_after = pages_after * page_size;
            let freed = size_before - size_after;

            info!("Vacuum: freed {} bytes", freed);
            Ok(freed)
        })();

        result.unwrap_or_else(|e| {
            warn!("Vacuum failed: {}", e);
            0
        })
    }
}

fn report_summary(report: &ArchiverReport) -> String {
    let mut parts = Vec::new();
    if report.events_archived > 0 {
        parts.push(format!("Events: {} archived", report.events_archived));
    }
    if report.edges_pruned > 0 {
        parts.push(format!("Edges: {} pruned", report.edges_pruned));
    }
    if report.mentions_compressed > 0 {
        parts.push(format!(
            "Mentions: {} compressed",
            report.mentions_compressed
        ));
    }
    if report.vacuum_freed_bytes > 0 {
        parts.push(format!(
            "Vacuum: {} bytes freed",
            report.vacuum_freed_bytes
        ));
    }
    if parts.is_empty() {
        return "All clean — nothing to compress.".to_string();
    }
    let mode = if report.dry_run {
        "DRY RUN"
    } else {
        "APPLIED"
    };
    format!("[{}] {}", mode, parts.join("; "))
}
