use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;
use tracing::{info, warn};


fn drives_by_type() -> &'static HashMap<&'static str, HashMap<&'static str, i64>> {
    static MAP: OnceLock<HashMap<&str, HashMap<&str, i64>>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert(
            "observation",
            HashMap::from([
                ("eros", 3),
                ("agency", 5),
                ("communion", 2),
                ("agape", 1),
            ]),
        );
        m.insert(
            "capability",
            HashMap::from([
                ("eros", 5),
                ("agency", 7),
                ("communion", 3),
                ("agape", 2),
            ]),
        );
        m.insert(
            "telos",
            HashMap::from([
                ("eros", 7),
                ("agency", 8),
                ("communion", 4),
                ("agape", 5),
            ]),
        );
        m.insert(
            "action",
            HashMap::from([
                ("eros", 4),
                ("agency", 6),
                ("communion", 3),
                ("agape", 2),
            ]),
        );
        m.insert(
            "being",
            HashMap::from([
                ("eros", 6),
                ("agency", 5),
                ("communion", 6),
                ("agape", 4),
            ]),
        );
        m.insert(
            "constraint",
            HashMap::from([
                ("eros", 2),
                ("agency", 4),
                ("communion", 2),
                ("agape", 1),
            ]),
        );
        m
    })
}

fn stage_by_type() -> &'static HashMap<&'static str, i32> {
    static MAP: OnceLock<HashMap<&str, i32>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert("observation", 2);  // T2
        m.insert("capability", 3);   // T3
        m.insert("telos", 1);        // T1
        m.insert("action", 4);       // T4
        m.insert("being", 0);        // T0
        m.insert("constraint", 2);   // T2
        m
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnricherReport {
    pub drives_enriched: i64,
    pub stages_enriched: i64,
    pub parents_enriched: i64,
    pub embeddings_enriched: i64,
    pub embeddings_failed: i64,
    pub dry_run: bool,
    pub timestamp: String,
}

pub struct Enricher<'a> {
    conn: &'a Connection,
}

impl<'a> Enricher<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn run(&self, dry_run: bool) -> Result<EnricherReport> {
        let mut report = EnricherReport {
            drives_enriched: 0,
            stages_enriched: 0,
            parents_enriched: 0,
            embeddings_enriched: 0,
            embeddings_failed: 0,
            dry_run,
            timestamp: Utc::now().to_rfc3339(),
        };

        info!("Enricher starting (dry_run={})", dry_run);

        self.enrich_embeddings(&mut report, dry_run);
        self.enrich_drives(&mut report, dry_run);
        self.enrich_stages(&mut report, dry_run);
        self.enrich_parents(&mut report, dry_run);

        info!("Enricher finished: {}", report_summary(&report));
        Ok(report)
    }

    fn enrich_embeddings(&self, report: &mut EnricherReport, dry_run: bool) {
        let result = (|| -> Result<()> {
            let rows: Vec<(String, String, String)> = {
                let mut stmt = self.conn.prepare(
                    "SELECT id, name, COALESCE(description, '') FROM nodes
                     WHERE valid_to IS NULL
                     AND id NOT IN (SELECT node_id FROM embeddings)",
                )?;
                let mapped = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?;
                mapped.filter_map(|r| r.ok()).collect()
            };

            if rows.is_empty() {
                return Ok(());
            }

            for (id, name, description) in &rows {
                let text = crate::mind::embedding::build_embedding_text(
                    &self.conn,
                    &id,
                    &name,
                    &description,
                    3,
                );
                match crate::mind::embedding::embed(&text) {
                    Ok(result) => {
                        if !dry_run {
                            let blob = crate::db::crud::serialize_vector(&result.vector);
                            self.conn.execute(
                                "INSERT OR REPLACE INTO embeddings (node_id, vector, model, updated_at)
                                 VALUES (?1, ?2, 'onnx', datetime('now'))",
                                rusqlite::params![id, blob],
                            )?;
                        }
                        report.embeddings_enriched += 1;
                    }
                    Err(e) => {
                        warn!("Embedding failed for node {}: {}", id, e);
                        report.embeddings_failed += 1;
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            warn!("Embedding enrichment failed: {}", e);
        }
    }

    fn enrich_drives(&self, report: &mut EnricherReport, dry_run: bool) {
        let result = (|| -> Result<()> {
            let rows: Vec<(String, String)> = {
                let mut stmt = self.conn.prepare(
                    "SELECT id, node_type FROM nodes
                     WHERE valid_to IS NULL
                     AND (drives_json IS NULL OR drives_json = '{}' OR drives_json = '')",
                )?;
                let mapped = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                mapped.filter_map(|r| r.ok()).collect()
            };

            if rows.is_empty() {
                return Ok(());
            }

            let now = Utc::now().to_rfc3339();
            let drives_map = drives_by_type();
            for (id, node_type) in &rows {
                if let Some(drives) = drives_map.get(node_type.as_str()) {
                    if !dry_run {
                        let drives_json =
                            serde_json::to_string(drives).unwrap_or_else(|_| "{}".to_string());
                        self.conn.execute(
                            "UPDATE nodes SET drives_json = ?1, updated_at = ?2 WHERE id = ?3",
                            rusqlite::params![drives_json, now, id],
                        )?;
                    }
                    report.drives_enriched += 1;
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            warn!("Drive enrichment failed: {}", e);
        }
    }

    fn enrich_stages(&self, report: &mut EnricherReport, dry_run: bool) {
        let result = (|| -> Result<()> {
            let rows: Vec<(String, String)> = {
                let mut stmt = self.conn.prepare(
                    "SELECT id, node_type FROM nodes
                     WHERE valid_to IS NULL
                     AND (developmental_stage IS NULL OR developmental_stage = 0)",
                )?;
                let mapped = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                mapped.filter_map(|r| r.ok()).collect()
            };

            if rows.is_empty() {
                return Ok(());
            }

            let now = Utc::now().to_rfc3339();
            let stages_map = stage_by_type();
            for (id, node_type) in &rows {
                if let Some(stage) = stages_map.get(node_type.as_str()) {
                    if !dry_run {
                        self.conn.execute(
                            "UPDATE nodes SET developmental_stage = ?1, updated_at = ?2 WHERE id = ?3",
                            rusqlite::params![stage, now, id],
                        )?;
                    }
                    report.stages_enriched += 1;
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            warn!("Stage enrichment failed: {}", e);
        }
    }

    fn enrich_parents(&self, report: &mut EnricherReport, dry_run: bool) {
        let result = (|| -> Result<()> {
            let node_ids: Vec<String> = {
                let mut stmt = self.conn.prepare(
                    "SELECT id FROM nodes
                     WHERE valid_to IS NULL
                     AND (parent_ids IS NULL OR parent_ids = '[]' OR parent_ids = '')",
                )?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                rows.filter_map(|r| r.ok()).collect()
            };

            if node_ids.is_empty() {
                return Ok(());
            }

            let now = Utc::now().to_rfc3339();
            for nid in &node_ids {
                let sources: Vec<String> = {
                    let mut stmt = self.conn.prepare(
                        "SELECT source_id FROM edges
                         WHERE target_id = ?1 AND edge_type = 'DECOMPOSES_TO' AND valid_to IS NULL
                         LIMIT 10",
                    )?;
                    let rows =
                        stmt.query_map(rusqlite::params![nid], |row| row.get::<_, String>(0))?;
                    rows.filter_map(|r| r.ok()).collect()
                };

                if sources.is_empty() {
                    continue;
                }

                let mut unique: Vec<String> = sources.into_iter().collect();
                unique.sort();
                unique.dedup();

                if !dry_run {
                    let parent_ids =
                        serde_json::to_string(&unique).unwrap_or_else(|_| "[]".to_string());
                    self.conn.execute(
                        "UPDATE nodes SET parent_ids = ?1, updated_at = ?2 WHERE id = ?3",
                        rusqlite::params![parent_ids, now, nid],
                    )?;
                }
                report.parents_enriched += 1;
            }
            Ok(())
        })();
        if let Err(e) = result {
            warn!("Parent enrichment failed: {}", e);
        }
    }
}

fn report_summary(report: &EnricherReport) -> String {
    let mut parts = Vec::new();
    if report.drives_enriched > 0 {
        parts.push(format!("drives: {}", report.drives_enriched));
    }
    if report.stages_enriched > 0 {
        parts.push(format!("stages: {}", report.stages_enriched));
    }
    if report.parents_enriched > 0 {
        parts.push(format!("parents: {}", report.parents_enriched));
    }
    if report.embeddings_enriched > 0 {
        parts.push(format!("embeddings: {}", report.embeddings_enriched));
    }
    if report.embeddings_failed > 0 {
        parts.push(format!("embeddings_failed: {}", report.embeddings_failed));
    }
    let mode = if report.dry_run {
        "DRY RUN"
    } else {
        "APPLIED"
    };
    if parts.is_empty() {
        return format!("Enricher [{}] nothing to do", mode);
    }
    format!("[{}] {}", mode, parts.join("; "))
}
