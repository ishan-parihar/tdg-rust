use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

const VALID_LIFECYCLE_STATES: &[&str] = &["active", "archived", "emerging", "declining"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JanitorReport {
    pub fts5_indexed: i64,
    pub fts5_skipped: i64,
    pub lifecycle_fixed: i64,
    pub edges_pruned: i64,
    pub parents_backfilled: i64,
    pub vec_missing: i64,
    pub vec_embedded: i64,
    pub mutations_purged: i64,
    pub health_checks_purged: i64,
    pub dry_run: bool,
    pub timestamp: String,
}

pub struct Janitor<'a> {
    conn: &'a Connection,
}

impl<'a> Janitor<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn run(&self, dry_run: bool) -> Result<JanitorReport> {
        let mut report = JanitorReport {
            fts5_indexed: 0,
            fts5_skipped: 0,
            lifecycle_fixed: 0,
            edges_pruned: 0,
            parents_backfilled: 0,
            vec_missing: 0,
            vec_embedded: 0,
            mutations_purged: 0,
            health_checks_purged: 0,
            dry_run,
            timestamp: Utc::now().to_rfc3339(),
        };

        info!("Janitor starting (dry_run={})", dry_run);

        self.fix_fts5(&mut report, dry_run);
        self.validate_lifecycle(&mut report, dry_run);
        self.prune_orphaned_edges(&mut report, dry_run);
        self.backfill_parent_ids(&mut report, dry_run);
        self.backfill_vec(&mut report, dry_run);
        self.purge_old_audit_data(&mut report, dry_run);

        info!("Janitor finished: {}", report_summary(&report));
        Ok(report)
    }

    fn fix_fts5(&self, report: &mut JanitorReport, dry_run: bool) {
        let result = (|| -> Result<()> {
            if dry_run {
                let count: i64 = self.conn.query_row(
                    "SELECT COUNT(*) FROM nodes n
                     LEFT JOIN nodes_fts f ON n.rowid = f.rowid
                     WHERE n.valid_to IS NULL AND f.id IS NULL",
                    [],
                    |r| r.get(0),
                )?;
                report.fts5_skipped = count;
                info!("FTS5 dry run: {} nodes need indexing", count);
            } else {
                // Backfill: insert FTS entries for active nodes missing from FTS
                let count = self.conn.execute(
                    "INSERT OR IGNORE INTO nodes_fts(rowid, id, name, description)
                     SELECT n.rowid, n.id, n.name, n.description
                     FROM nodes n
                     LEFT JOIN nodes_fts f ON n.rowid = f.rowid
                     WHERE n.valid_to IS NULL AND f.id IS NULL",
                    [],
                )?;
                report.fts5_indexed = count as i64;
                info!("FTS5: {} indexed", count);
            }
            Ok(())
        })();
        if let Err(e) = result {
            warn!("FTS5 fix failed: {}", e);
        }
    }

    fn validate_lifecycle(&self, report: &mut JanitorReport, dry_run: bool) {
        let result = (|| -> Result<()> {
            let placeholders: Vec<String> = VALID_LIFECYCLE_STATES
                .iter()
                .map(|_| "?".to_string())
                .collect();
            let where_clause = placeholders.join(",");

            let ids: Vec<String> = {
                let sql = format!(
                    "SELECT id FROM nodes WHERE valid_to IS NULL AND lifecycle_state NOT IN ({where_clause})"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let params: Vec<Box<dyn rusqlite::types::ToSql>> = VALID_LIFECYCLE_STATES
                    .iter()
                    .map(|s| Box::new(s.to_string()) as Box<dyn rusqlite::types::ToSql>)
                    .collect();
                let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();
                let rows = stmt.query_map(param_refs.as_slice(), |row| row.get::<_, String>(0))?;
                rows.filter_map(|r| r.ok()).collect()
            };

            report.lifecycle_fixed = ids.len() as i64;
            if ids.is_empty() {
                info!("Lifecycle: all states valid");
                return Ok(());
            }

            info!(
                "Lifecycle: {} nodes with invalid state{}",
                ids.len(),
                if dry_run { " (dry run)" } else { "" }
            );

            if !dry_run {
                let now = Utc::now().to_rfc3339();
                for id in &ids {
                    self.conn
                        .execute(
                            "UPDATE nodes SET lifecycle_state = 'active', updated_at = ?1 WHERE id = ?2",
                            rusqlite::params![now, id],
                        )
                        .unwrap_or_else(|e| {
                            warn!("Failed to fix lifecycle for {}: {}", id, e);
                            0
                        });
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            warn!("Lifecycle validation failed: {}", e);
        }
    }

    fn prune_orphaned_edges(&self, report: &mut JanitorReport, dry_run: bool) {
        let result = (|| -> Result<()> {
            let edge_ids: Vec<String> = {
                let mut stmt = self.conn.prepare(
                    "SELECT e.id FROM edges e
                     WHERE e.valid_to IS NULL
                     AND (
                         NOT EXISTS (SELECT 1 FROM nodes n WHERE n.id = e.source_id AND n.valid_to IS NULL AND n.lifecycle_state != 'archived')
                         OR NOT EXISTS (SELECT 1 FROM nodes n WHERE n.id = e.target_id AND n.valid_to IS NULL AND n.lifecycle_state != 'archived')
                     )",
                )?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                rows.filter_map(|r| r.ok()).collect()
            };

            report.edges_pruned = edge_ids.len() as i64;
            if edge_ids.is_empty() {
                info!("Edges: no orphans found");
                return Ok(());
            }

            info!(
                "Edges: {} orphaned edges found{}",
                edge_ids.len(),
                if dry_run { " (dry run)" } else { "" }
            );

            if !dry_run {
                let now = Utc::now().to_rfc3339();
                for id in &edge_ids {
                    self.conn
                        .execute(
                            "UPDATE edges SET valid_to = ?1 WHERE id = ?2",
                            rusqlite::params![now, id],
                        )
                        .unwrap_or_else(|e| {
                            warn!("Failed to prune edge {}: {}", id, e);
                            0
                        });
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            warn!("Orphan pruning failed: {}", e);
        }
    }

    fn backfill_parent_ids(&self, report: &mut JanitorReport, dry_run: bool) {
        let result = (|| -> Result<()> {
            let node_ids: Vec<String> = {
                let mut stmt = self.conn.prepare(
                    "SELECT n.id FROM nodes n
                     WHERE n.valid_to IS NULL
                     AND (n.parent_ids IS NULL OR n.parent_ids = '[]' OR n.parent_ids = '')
                     AND EXISTS (
                         SELECT 1 FROM edges e
                         WHERE e.target_id = n.id
                         AND e.edge_type = 'DECOMPOSES_TO'
                         AND e.valid_to IS NULL
                     )",
                )?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                rows.filter_map(|r| r.ok()).collect()
            };

            report.parents_backfilled = node_ids.len() as i64;
            if node_ids.is_empty() {
                info!("Parents: nothing to backfill");
                return Ok(());
            }

            info!(
                "Parents: {} nodes need backfill{}",
                node_ids.len(),
                if dry_run { " (dry run)" } else { "" }
            );

            if !dry_run {
                let now = Utc::now().to_rfc3339();
                for nid in &node_ids {
                    let sources: Vec<String> = {
                        let mut stmt = self.conn.prepare(
                            "SELECT source_id FROM edges
                             WHERE target_id = ?1 AND edge_type = 'DECOMPOSES_TO' AND valid_to IS NULL",
                        )?;
                        let rows = stmt.query_map(rusqlite::params![nid], |row| {
                            row.get::<_, String>(0)
                        })?;
                        rows.filter_map(|r| r.ok()).collect()
                    };
                    if !sources.is_empty() {
                        let parent_ids =
                            serde_json::to_string(&sources).unwrap_or_else(|_| "[]".to_string());
                        self.conn.execute(
                            "UPDATE nodes SET parent_ids = ?1, updated_at = ?2 WHERE id = ?3",
                            rusqlite::params![parent_ids, now, nid],
                        )?;
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            warn!("Parent backfill failed: {}", e);
        }
    }

    fn backfill_vec(&self, report: &mut JanitorReport, dry_run: bool) {
        let result = (|| -> Result<()> {
            // Find active nodes without embeddings
            let nodes: Vec<(String, String, String)> = {
                let mut stmt = self.conn.prepare(
                    "SELECT n.id, n.name, COALESCE(n.description, '') FROM nodes n
                     WHERE n.valid_to IS NULL
                     AND n.id NOT IN (SELECT node_id FROM embeddings)",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
                rows.filter_map(|r| r.ok()).collect()
            };

            report.vec_missing = nodes.len() as i64;
            if nodes.is_empty() {
                info!("Vec: no nodes need embeddings");
                return Ok(());
            }

            info!(
                "Vec: {} nodes need embeddings{}",
                nodes.len(),
                if dry_run { " (dry run)" } else { "" }
            );

            if dry_run {
                return Ok(());
            }

            let mut embedded = 0i64;
            for (id, name, desc) in &nodes {
                let text = if desc.is_empty() {
                    name.clone()
                } else {
                    format!("{name} {desc}")
                };

                match crate::mind::embedding::embed(&text) {
                    Ok(result) => {
                        let dimension = result.vector.len() as i64;
                        crate::db::crud::upsert_embedding(
                            self.conn,
                            id,
                            &result.vector,
                            "onnx",
                            dimension,
                        )?;
                        embedded += 1;
                    }
                    Err(e) => {
                        warn!("Vec: failed to embed {}: {}", id, e);
                    }
                }
            }

            report.vec_embedded = embedded;
            report.vec_missing = nodes.len() as i64 - embedded;
            if embedded > 0 || report.vec_missing > 0 {
                info!("Vec: {} embedded, {} failed", embedded, report.vec_missing);
            }

            Ok(())
        })();
        if let Err(e) = result {
            warn!("Vec backfill failed: {}", e);
        }
    }

    /// Purge old mutation_log and health_checks rows to prevent unbounded growth.
    ///
    /// - mutation_log: keep 90 days (configurable via TDG_MUTATION_RETENTION_DAYS)
    /// - health_checks: keep 30 days (configurable via TDG_HEALTH_CHECK_RETENTION_DAYS)
    ///
    /// Previously, mutation_log and health_checks had NO purge mechanism — they
    /// grew forever. The events table had a 90-day purge in the Archiver, but
    /// only ran when SelfManager was explicitly invoked. Now the Janitor purges
    /// all three audit tables on every run.
    fn purge_old_audit_data(&self, report: &mut JanitorReport, dry_run: bool) {
        let mutation_retention_days = std::env::var("TDG_MUTATION_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(90);
        let health_check_retention_days = std::env::var("TDG_HEALTH_CHECK_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(30);

        if dry_run {
            let mutation_count: i64 = self.conn
                .query_row(
                    "SELECT COUNT(*) FROM mutation_log WHERE timestamp < ?1",
                    rusqlite::params![format!("{}T00:00:00Z",
                        chrono::Utc::now().naive_utc()
                            .checked_sub_signed(chrono::Duration::days(mutation_retention_days))
                            .map(|t| t.format("%Y-%m-%d").to_string())
                            .unwrap_or_default())],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let health_count: i64 = self.conn
                .query_row(
                    "SELECT COUNT(*) FROM health_checks WHERE timestamp < ?1",
                    rusqlite::params![format!("{}T00:00:00Z",
                        chrono::Utc::now().naive_utc()
                            .checked_sub_signed(chrono::Duration::days(health_check_retention_days))
                            .map(|t| t.format("%Y-%m-%d").to_string())
                            .unwrap_or_default())],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            report.mutations_purged = mutation_count;
            report.health_checks_purged = health_count;
            return;
        }

        // Purge old mutation_log rows
        let mutation_cutoff = chrono::Utc::now().naive_utc()
            .checked_sub_signed(chrono::Duration::days(mutation_retention_days))
            .map(|t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
        match self.conn.execute(
            "DELETE FROM mutation_log WHERE timestamp < ?1",
            rusqlite::params![&mutation_cutoff],
        ) {
            Ok(count) => {
                report.mutations_purged = count as i64;
                if count > 0 {
                    info!("Purged {} mutation_log rows older than {} days", count, mutation_retention_days);
                }
            }
            Err(e) => warn!("Failed to purge mutation_log: {}", e),
        }

        // Purge old health_checks rows
        let health_cutoff = chrono::Utc::now().naive_utc()
            .checked_sub_signed(chrono::Duration::days(health_check_retention_days))
            .map(|t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
        match self.conn.execute(
            "DELETE FROM health_checks WHERE timestamp < ?1",
            rusqlite::params![&health_cutoff],
        ) {
            Ok(count) => {
                report.health_checks_purged = count as i64;
                if count > 0 {
                    info!("Purged {} health_checks rows older than {} days", count, health_check_retention_days);
                }
            }
            Err(e) => warn!("Failed to purge health_checks: {}", e),
        }
    }
}

fn report_summary(report: &JanitorReport) -> String {
    let mut parts = Vec::new();
    if report.fts5_indexed > 0 || report.fts5_skipped > 0 {
        parts.push(format!(
            "FTS5: {} indexed, {} skipped",
            report.fts5_indexed, report.fts5_skipped
        ));
    }
    if report.lifecycle_fixed > 0 {
        parts.push(format!("Lifecycle: {} fixed", report.lifecycle_fixed));
    }
    if report.edges_pruned > 0 {
        parts.push(format!("Edges: {} pruned", report.edges_pruned));
    }
    if report.parents_backfilled > 0 {
        parts.push(format!(
            "Parents: {} backfilled",
            report.parents_backfilled
        ));
    }
    if report.vec_missing > 0 || report.vec_embedded > 0 {
        parts.push(format!(
            "Vec: {} missing, {} embedded",
            report.vec_missing, report.vec_embedded
        ));
    }
    if report.mutations_purged > 0 || report.health_checks_purged > 0 {
        parts.push(format!(
            "Purged: {} mutations, {} health_checks",
            report.mutations_purged, report.health_checks_purged
        ));
    }
    if parts.is_empty() {
        return "All clean — nothing to fix.".to_string();
    }
    let mode = if report.dry_run {
        "DRY RUN"
    } else {
        "APPLIED"
    };
    format!("[{}] {}", mode, parts.join("; "))
}
