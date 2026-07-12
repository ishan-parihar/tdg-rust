//! # Orphan Linker
//!
//! Semantically links orphaned nodes (nodes with zero active edges) back into
//! the graph by finding suitable candidates via keyword overlap, temporal
//! proximity, and cosine similarity of embeddings.
//!
//! ## Algorithm
//! 1. Fetch orphans: active nodes with no edges in either direction.
//! 2. Fetch candidates: all other active nodes.
//! 3. For each orphan, score each candidate via `suggest_links`.
//! 4. Keep the top-3 candidates with confidence >= 0.65.
//! 5. Create `RELATES_TO` structural edges using `crate::db::crud::add_edge`.
//!
//! ## Tier
//! This is a **Tier 1** maintenance operation. It never enqueues jobs or
//! touches the metabolism pipeline.

use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::models::NewEdge;

/// Confidence threshold: candidates below this score are not linked.
pub const LINK_CONFIDENCE_THRESHOLD: f64 = 0.65;

/// Maximum links created per orphan (top-N candidates).
pub const MAX_LINKS_PER_ORPHAN: usize = 3;

/// A candidate link produced by `suggest_links`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkCandidate {
    pub candidate_id: String,
    pub confidence: f64,
    /// Which signals contributed: "keyword", "temporal", "semantic"
    pub signals: Vec<String>,
}

/// Report produced by a `link_orphans` run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkerReport {
    pub orphans_found: i64,
    pub edges_created: i64,
    pub dry_run: bool,
    pub timestamp: String,
}

/// A lightweight node record used internally by the linker.
#[derive(Debug, Clone)]
struct NodeInfo {
    id: String,
    name: String,
    description: String,
    created_at: String,
}

/// Compute keyword-overlap score between two nodes.
///
/// Tokenises by whitespace + lowercasing, ignores single-character tokens,
/// and returns `|intersection| / |union|` (Jaccard index).
fn keyword_overlap(a_name: &str, a_desc: &str, b_name: &str, b_desc: &str) -> f64 {
    use std::collections::HashSet;

    fn tokenise(text: &str) -> HashSet<String> {
        text.split_whitespace()
            .map(|t| {
                t.to_lowercase()
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .to_string()
            })
            .filter(|t| t.len() > 1)
            .collect()
    }

    let a_tokens = tokenise(&format!("{} {}", a_name, a_desc));
    let b_tokens = tokenise(&format!("{} {}", b_name, b_desc));

    if a_tokens.is_empty() || b_tokens.is_empty() {
        return 0.0;
    }

    let intersection = a_tokens.intersection(&b_tokens).count();
    let union = a_tokens.union(&b_tokens).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Returns 1.0 if both nodes were created within 24 hours of each other, else 0.0.
fn temporal_proximity(a_created_at: &str, b_created_at: &str) -> f64 {
    use chrono::DateTime;
    let parse = |s: &str| -> Option<chrono::DateTime<chrono::FixedOffset>> {
        DateTime::parse_from_rfc3339(s).ok()
    };
    match (parse(a_created_at), parse(b_created_at)) {
        (Some(a), Some(b)) => {
            let diff = (a - b).num_seconds().unsigned_abs();
            if diff <= 86_400 {
                1.0
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

/// Decode a raw BLOB from the `embeddings` table into a `Vec<f32>`.
fn decode_blob(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// Fetch the embedding vector for a node (if available).
fn fetch_embedding(conn: &Connection, node_id: &str) -> Option<Vec<f32>> {
    conn.query_row(
        "SELECT vector FROM embeddings WHERE node_id = ?1",
        rusqlite::params![node_id],
        |row| row.get::<_, Vec<u8>>(0),
    )
    .ok()
    .map(|blob| decode_blob(&blob))
}

/// Score an orphan against a single candidate using three signals:
///
/// 1. **Keyword overlap** (Jaccard, weight 0.5)
/// 2. **Temporal proximity** (binary 24h window, weight 0.2)
/// 3. **Semantic cosine similarity** (if both have embeddings, weight 0.3)
///
/// Returns a `LinkCandidate` if `confidence >= LINK_CONFIDENCE_THRESHOLD`.
pub fn suggest_links(
    conn: &Connection,
    orphan_id: &str,
    orphan_name: &str,
    orphan_desc: &str,
    orphan_created_at: &str,
    candidate_id: &str,
    candidate_name: &str,
    candidate_desc: &str,
    candidate_created_at: &str,
) -> Option<LinkCandidate> {
    if orphan_id == candidate_id {
        return None;
    }

    let mut score = 0.0_f64;
    let mut signals: Vec<String> = Vec::new();

    // Signal 1: keyword overlap (weight 0.5)
    let kw = keyword_overlap(orphan_name, orphan_desc, candidate_name, candidate_desc);
    if kw > 0.0 {
        score += 0.5 * kw;
        signals.push("keyword".to_string());
    }

    // Signal 2: temporal proximity (weight 0.2)
    let tp = temporal_proximity(orphan_created_at, candidate_created_at);
    if tp > 0.0 {
        score += 0.2 * tp;
        signals.push("temporal".to_string());
    }

    // Signal 3: semantic cosine similarity (weight 0.3, only if embeddings exist)
    let orphan_emb = fetch_embedding(conn, orphan_id);
    let candidate_emb = fetch_embedding(conn, candidate_id);
    if let (Some(o_vec), Some(c_vec)) = (orphan_emb, candidate_emb) {
        let sim = crate::util::math::cosine_similarity(&o_vec, &c_vec);
        if sim > 0.0 {
            score += 0.3 * sim as f64;
            signals.push("semantic".to_string());
        }
    }

    if score >= LINK_CONFIDENCE_THRESHOLD && !signals.is_empty() {
        Some(LinkCandidate {
            candidate_id: candidate_id.to_string(),
            confidence: score,
            signals,
        })
    } else {
        None
    }
}

/// Fetch all active orphan nodes (nodes with no edges in either direction).
fn fetch_orphans(conn: &Connection) -> Result<Vec<NodeInfo>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, COALESCE(description, ''), created_at
         FROM nodes
         WHERE valid_to IS NULL
           AND lifecycle_state != 'archived'
           AND id NOT IN (
               SELECT DISTINCT source_id FROM edges WHERE valid_to IS NULL
               UNION
               SELECT DISTINCT target_id FROM edges WHERE valid_to IS NULL
           )",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(NodeInfo {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            created_at: row.get(3)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Fetch all active non-archived nodes as linking candidates.
fn fetch_candidates(conn: &Connection) -> Result<Vec<NodeInfo>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, COALESCE(description, ''), created_at
         FROM nodes
         WHERE valid_to IS NULL
           AND lifecycle_state != 'archived'",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(NodeInfo {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            created_at: row.get(3)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Link orphaned nodes back into the graph.
///
/// For each orphan, the top `MAX_LINKS_PER_ORPHAN` candidates with
/// `confidence >= LINK_CONFIDENCE_THRESHOLD` receive a `RELATES_TO` edge.
///
/// # Write safety
/// This function delegates all edge creation to `crate::db::crud::add_edge`,
/// which internally calls the circuit breaker and write guard.
pub fn link_orphans(conn: &Connection, dry_run: bool) -> Result<LinkerReport> {
    let mut report = LinkerReport {
        orphans_found: 0,
        edges_created: 0,
        dry_run,
        timestamp: Utc::now().to_rfc3339(),
    };

    info!("Linker starting (dry_run={})", dry_run);

    let orphans = fetch_orphans(conn)?;
    report.orphans_found = orphans.len() as i64;

    if orphans.is_empty() {
        info!("Linker: no orphans found");
        return Ok(report);
    }

    let candidates = fetch_candidates(conn)?;

    for orphan in &orphans {
        // Score all candidates
        let mut scored: Vec<LinkCandidate> = candidates
            .iter()
            .filter(|c| c.id != orphan.id)
            .filter_map(|c| {
                suggest_links(
                    conn,
                    &orphan.id,
                    &orphan.name,
                    &orphan.description,
                    &orphan.created_at,
                    &c.id,
                    &c.name,
                    &c.description,
                    &c.created_at,
                )
            })
            .collect();

        // Sort descending by confidence, keep top-N
        scored.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(MAX_LINKS_PER_ORPHAN);

        for candidate in &scored {
            if dry_run {
                info!(
                    "Linker [dry-run]: would link {} → {} (confidence={:.3}, signals={:?})",
                    orphan.id, candidate.candidate_id, candidate.confidence, candidate.signals
                );
                report.edges_created += 1;
            } else {
                let new_edge = NewEdge {
                    source_id: orphan.id.clone(),
                    target_id: candidate.candidate_id.clone(),
                    edge_type: "RELATES_TO".to_string(),
                    weight: Some(candidate.confidence),
                    properties: Some(serde_json::json!({
                        "auto_linked": true,
                        "signals": candidate.signals,
                        "confidence": candidate.confidence,
                    })),
                    agent_id: Some("linker".to_string()),
                };
                match crate::db::crud::add_edge(conn, &new_edge) {
                    Ok(_) => {
                        info!(
                            "Linker: linked {} → {} (confidence={:.3})",
                            orphan.id, candidate.candidate_id, candidate.confidence
                        );
                        report.edges_created += 1;
                    }
                    Err(e) => {
                        warn!(
                            "Linker: failed to create edge {} → {}: {}",
                            orphan.id, candidate.candidate_id, e
                        );
                    }
                }
            }
        }
    }

    info!(
        "Linker finished: {} orphans found, {} edges {}",
        report.orphans_found,
        report.edges_created,
        if dry_run { "would be created (dry run)" } else { "created" }
    );

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        crate::db::init_fts(&conn).unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    fn insert_node(conn: &Connection, id: &str, name: &str, desc: &str, created_at: &str) {
        conn.execute(
            "INSERT INTO nodes (id, node_type, name, description, lifecycle_state, created_at, updated_at)
             VALUES (?1, 'observation', ?2, ?3, 'active', ?4, ?4)",
            rusqlite::params![id, name, desc, created_at],
        )
        .unwrap();
    }

    // ──────────────────────────────────────────────────────────────────────────
    // keyword_overlap unit tests
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn keyword_overlap_identical_text() {
        let score = keyword_overlap("memory graph", "neural storage", "memory graph", "neural storage");
        // identical → union == intersection → Jaccard = 1.0
        assert!((score - 1.0).abs() < 1e-9, "expected 1.0, got {score}");
    }

    #[test]
    fn keyword_overlap_no_common_words() {
        let score = keyword_overlap("alpha beta", "gamma", "delta epsilon", "zeta");
        assert_eq!(score, 0.0, "expected 0.0 for disjoint token sets");
    }

    #[test]
    fn keyword_overlap_partial_match() {
        let score = keyword_overlap("neural memory", "", "memory storage", "");
        // tokens a: {neural, memory}, tokens b: {memory, storage}
        // intersection: {memory} = 1, union = 3 → Jaccard = 1/3
        assert!(score > 0.0 && score < 1.0, "expected partial score, got {score}");
    }

    #[test]
    fn keyword_overlap_single_char_tokens_ignored() {
        // Single-char tokens should be filtered out
        let score = keyword_overlap("a b c", "", "a b c", "");
        // All tokens are single-char and get filtered → both sets empty → 0.0
        assert_eq!(score, 0.0);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // temporal_proximity unit tests
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn temporal_proximity_within_24h() {
        let a = "2025-01-15T10:00:00+00:00";
        let b = "2025-01-15T20:00:00+00:00"; // 10h apart
        assert_eq!(temporal_proximity(a, b), 1.0);
    }

    #[test]
    fn temporal_proximity_exactly_24h() {
        let a = "2025-01-15T00:00:00+00:00";
        let b = "2025-01-16T00:00:00+00:00"; // exactly 86400s
        assert_eq!(temporal_proximity(a, b), 1.0);
    }

    #[test]
    fn temporal_proximity_beyond_24h() {
        let a = "2025-01-15T00:00:00+00:00";
        let b = "2025-01-17T00:00:00+00:00"; // 48h apart
        assert_eq!(temporal_proximity(a, b), 0.0);
    }

    #[test]
    fn temporal_proximity_invalid_timestamps() {
        // malformed timestamps → 0.0
        assert_eq!(temporal_proximity("not-a-date", "also-not-a-date"), 0.0);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // suggest_links integration tests (no embeddings path)
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn suggest_links_keyword_match_below_threshold() {
        let conn = setup_db();
        // keyword overlap alone: "neural memory" vs "memory storage"
        // tokens: {neural,memory} vs {memory,storage}, Jaccard = 1/3 ≈ 0.333
        // score = 0.5 * 0.333 = 0.167 < 0.65 → None
        let ts = "2025-01-15T10:00:00+00:00";
        let result = suggest_links(
            &conn,
            "n001", "neural memory", "processing",
            ts,
            "n002", "memory storage", "data",
            ts,
        );
        // temporal adds 0.2, kw adds ~0.167 → ~0.367, still below 0.65
        assert!(result.is_none() || result.as_ref().unwrap().confidence < 0.65,
            "should be None or below threshold");
    }

    #[test]
    fn suggest_links_strong_keyword_and_temporal() {
        let conn = setup_db();
        // Identical text → kw = 1.0 → score += 0.5; same time → temporal 1.0 → += 0.2
        // Total = 0.7 >= 0.65 → Some(...)
        let ts = "2025-01-15T10:00:00+00:00";
        let result = suggest_links(
            &conn,
            "n001", "neural memory graph", "store and recall",
            ts,
            "n002", "neural memory graph", "store and recall",
            ts,
        );
        assert!(result.is_some(), "expected a candidate link");
        let link = result.unwrap();
        assert!(link.confidence >= 0.65);
        assert!(link.signals.contains(&"keyword".to_string()));
        assert!(link.signals.contains(&"temporal".to_string()));
    }

    #[test]
    fn suggest_links_same_id_returns_none() {
        let conn = setup_db();
        let ts = "2025-01-15T10:00:00+00:00";
        let result = suggest_links(
            &conn,
            "n001", "some node", "desc", ts,
            "n001", "some node", "desc", ts,
        );
        assert!(result.is_none(), "orphan should not link to itself");
    }

    // ──────────────────────────────────────────────────────────────────────────
    // link_orphans integration tests
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn link_orphans_no_orphans() {
        let conn = setup_db();
        let report = link_orphans(&conn, true).unwrap();
        assert_eq!(report.orphans_found, 0);
        assert_eq!(report.edges_created, 0);
        assert!(report.dry_run);
    }

    #[test]
    fn link_orphans_dry_run_counts_without_creating() {
        let conn = setup_db();
        let ts = "2025-01-15T10:00:00+00:00";
        insert_node(&conn, "n001", "neural memory graph", "store recall embeddings", ts);
        insert_node(&conn, "n002", "neural storage graph", "store recall embeddings", ts);

        let report = link_orphans(&conn, true).unwrap();
        assert_eq!(report.orphans_found, 2);
        assert!(report.dry_run);

        // Verify no edges were actually inserted
        let edge_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM edges WHERE valid_to IS NULL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(edge_count, 0, "dry run must not create edges");
    }
}
