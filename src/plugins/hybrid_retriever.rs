//! Hybrid Retriever — combined FTS5 + trust + recency scoring
//!
//! Port of `plugins/tdg/hybrid_retriever.py`.

use std::collections::HashSet;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db::events::record_retrieval;
use crate::error::TdgResult;
use crate::models::Node;

/// Search result with scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub node: Node,
    pub score: f64,
    pub method: String,
}

/// Hybrid retrieval weights.
#[derive(Debug, Clone)]
pub struct RetrievalWeights {
    pub fts_weight: f64,
    pub trust_weight: f64,
    pub recency_weight: f64,
    pub term_overlap_weight: f64,
    pub type_boost_weight: f64,
}

impl Default for RetrievalWeights {
    fn default() -> Self {
        Self {
            fts_weight: 0.50,
            trust_weight: 0.30,
            recency_weight: 0.10,
            term_overlap_weight: 0.10,
            type_boost_weight: 0.15,
        }
    }
}

/// Stop words for filtering.
static STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "have", "has",
    "had", "do", "does", "did", "will", "would", "could", "should", "may",
    "might", "can", "to", "of", "in", "for", "on", "with", "at", "by", "from",
    "as", "into", "and", "but", "or", "if", "it", "its", "this", "that",
];

/// High-value node types for boosting.
static HIGH_VALUE_TYPES: &[&str] = &["action", "telos", "skill", "hypothesis", "discovery"];

/// The Hybrid Retriever — combined FTS5 + trust + recency scoring.
pub struct HybridRetriever {
    weights: RetrievalWeights,
    min_confidence: f64,
}

impl HybridRetriever {
    pub fn new() -> Self {
        Self {
            weights: RetrievalWeights::default(),
            min_confidence: 0.0,
        }
    }

    pub fn with_weights(weights: RetrievalWeights) -> Self {
        Self {
            weights,
            min_confidence: 0.0,
        }
    }

    /// Hybrid search combining FTS5, trust, recency, and type boost.
    pub fn search(
        &self,
        conn: &Connection,
        query: &str,
        limit: i64,
        node_type: Option<&str>,
    ) -> TdgResult<Vec<SearchResult>> {
        let limit = limit.min(50);

        // Phase 1: FTS5 search
        let mut results = self.fts_search(conn, query, limit * 2)?;

        // Phase 2: LIKE fallback if FTS returns nothing
        if results.is_empty() {
            results = self.like_search(conn, query, limit * 2)?;
        }

        // Phase 3: Type-filtered fallback
        if results.is_empty() {
            results = self.type_search(conn, node_type, limit)?;
        }

        // Score and rank
        let query_tokens = self.tokenize(query);
        let mut scored: Vec<SearchResult> = results
            .into_iter()
            .map(|node| {
                let fts_score = node.confidence; // Use confidence as FTS proxy
                let trust_score = node.confidence;

                // Recency score (based on created_at age)
                let recency = self.recency_score(&node.created_at);

                // Term overlap
                let node_tokens = self.tokenize(&format!("{} {}", node.name, node.description));
                let overlap = query_tokens
                    .iter()
                    .filter(|qt| node_tokens.contains(qt))
                    .count() as f64
                    / query_tokens.len().max(1) as f64;

                // Type boost
                let type_boost = if HIGH_VALUE_TYPES.contains(&node.node_type.as_str()) {
                    0.15
                } else {
                    0.0
                };

                let total_score = self.weights.fts_weight * fts_score
                    + self.weights.trust_weight * trust_score
                    + self.weights.recency_weight * recency
                    + self.weights.term_overlap_weight * overlap
                    + self.weights.type_boost_weight * type_boost;

                SearchResult {
                    node,
                    score: total_score,
                    method: "hybrid".to_string(),
                }
            })
            .filter(|r| r.node.confidence >= self.min_confidence)
            .collect();

        // Deduplicate by node ID
        let mut seen = HashSet::new();
        scored.retain(|r| seen.insert(r.node.id.clone()));

        // Sort by score descending
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit as usize);

        // Record retrievals for trust tracking
        for result in &scored {
            let _ = record_retrieval(conn, &result.node.id);
        }

        Ok(scored)
    }

    fn fts_search(&self, conn: &Connection, query: &str, limit: i64) -> TdgResult<Vec<Node>> {
        let sql = "
            SELECT n.id, n.node_type, n.name, n.description, n.properties_json, n.quadrants_json,
                   n.drives_json, n.lifecycle_state, n.teleological_level, n.developmental_stage,
                   n.confidence, n.source, n.parent_ids, n.agent_path, n.created_at, n.updated_at,
                   n.valid_from, n.valid_to, n.helpful_count, n.retrieval_count, n.agent_id
            FROM nodes_fts fts
            JOIN nodes n ON fts.rowid = n.rowid
            WHERE nodes_fts MATCH ?1 AND n.valid_to IS NULL
            ORDER BY rank
            LIMIT ?2
        ";

        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Ok(Vec::new()),
        };

        let rows = stmt.query_map(params![query, limit], crate::db::crud::row_to_node)?;
        let mut nodes = Vec::new();
        for row in rows {
            if let Ok(node) = row {
                nodes.push(node);
            }
        }
        Ok(nodes)
    }

    fn like_search(&self, conn: &Connection, query: &str, limit: i64) -> TdgResult<Vec<Node>> {
        // Filter out stop words for LIKE search
        let meaningful_terms: Vec<String> = self
            .tokenize(query)
            .into_iter()
            .filter(|t| !STOP_WORDS.contains(&t.as_str()))
            .collect();

        if meaningful_terms.is_empty() {
            return Ok(Vec::new());
        }

        // Build individual LIKE conditions for each term (OR logic)
        let mut conditions = Vec::new();
        let mut all_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        for term in &meaningful_terms {
            let pat = format!("%{}%", term);
            conditions.push("(name LIKE ? OR description LIKE ?)");
            all_params.push(Box::new(pat.clone()));
            all_params.push(Box::new(pat));
        }
        let where_clause = conditions.join(" OR ");
        let sql = format!(
            "SELECT id, node_type, name, description, properties_json, quadrants_json,
                   drives_json, lifecycle_state, teleological_level, developmental_stage,
                   confidence, source, parent_ids, agent_path, created_at, updated_at,
                   valid_from, valid_to, helpful_count, retrieval_count, agent_id
            FROM nodes
            WHERE valid_to IS NULL AND ({})
            ORDER BY confidence DESC, created_at DESC
            LIMIT {}",
            where_clause, limit
        );

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = all_params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(&*param_refs, crate::db::crud::row_to_node)?;
        let mut nodes = Vec::new();
        for row in rows {
            if let Ok(node) = row {
                nodes.push(node);
            }
        }
        Ok(nodes)
    }

    fn type_search(
        &self,
        conn: &Connection,
        node_type: Option<&str>,
        limit: i64,
    ) -> TdgResult<Vec<Node>> {
        let (sql, param): (String, Option<String>) = match node_type {
            Some(nt) => (
                "SELECT id, node_type, name, description, properties_json, quadrants_json,
                 drives_json, lifecycle_state, teleological_level, developmental_stage,
                 confidence, source, parent_ids, agent_path, created_at, updated_at,
                 valid_from, valid_to, helpful_count, retrieval_count, agent_id
                 FROM nodes WHERE valid_to IS NULL AND node_type = ?1
                 ORDER BY confidence DESC LIMIT ?2"
                    .to_string(),
                Some(nt.to_string()),
            ),
            None => (
                "SELECT id, node_type, name, description, properties_json, quadrants_json,
                 drives_json, lifecycle_state, teleological_level, developmental_stage,
                 confidence, source, parent_ids, agent_path, created_at, updated_at,
                 valid_from, valid_to, helpful_count, retrieval_count, agent_id
                 FROM nodes WHERE valid_to IS NULL
                 ORDER BY confidence DESC LIMIT ?1"
                    .to_string(),
                None,
            ),
        };

        let mut stmt = conn.prepare(&sql)?;
        let rows = if let Some(ref pv) = param {
            stmt.query_map(params![pv, limit], crate::db::crud::row_to_node)?
        } else {
            stmt.query_map(params![limit], crate::db::crud::row_to_node)?
        };

        let mut nodes = Vec::new();
        for row in rows {
            if let Ok(node) = row {
                nodes.push(node);
            }
        }
        Ok(nodes)
    }

    fn tokenize(&self, text: &str) -> Vec<String> {
        text.to_lowercase()
            .split_whitespace()
            .map(|w| {
                // Strip common suffixes
                let clean: String = w
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                clean
            })
            .filter(|t| t.len() > 2 && !STOP_WORDS.contains(&t.as_str()))
            .collect()
    }

    fn recency_score(&self, created_at: &str) -> f64 {
        if let Ok(created) = chrono::NaiveDateTime::parse_from_str(
            created_at.replace('Z', "").as_str(),
            "%Y-%m-%dT%H:%M:%S%.f",
        ) {
            let now = chrono::Utc::now().naive_utc();
            let age_days = (now - created).num_days() as f64;
            // Decay: 1.0 at day 0, 0.5 at day 30, approaches 0
            (1.0 / (1.0 + age_days / 30.0)).max(0.0)
        } else {
            0.5 // default mid-score
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_fts, init_schema, run_migrations};
    use crate::models::NewNode;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn hybrid_search_fts() {
        let conn = setup_db();
        crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Rust memory safety".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Python GIL".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let retriever = HybridRetriever::new();
        let results = retriever.search(&conn, "Rust", 10, None).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].node.name.contains("Rust"));
    }

    #[test]
    fn hybrid_like_fallback() {
        let conn = setup_db();
        crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Custom entity name".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let retriever = HybridRetriever::new();
        let results = retriever
            .search(&conn, "Custom entity name", 10, None)
            .unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn hybrid_type_filter() {
        let conn = setup_db();
        crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Main Goal".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "Some Action".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let retriever = HybridRetriever::new();
        let results = retriever
            .search(&conn, "nothing matches", 10, Some("telos"))
            .unwrap();
        assert!(results.iter().all(|r| r.node.node_type == "telos"));
    }

    #[test]
    fn tokenize_basic() {
        let retriever = HybridRetriever::new();
        let tokens = retriever.tokenize("The Quick Brown Fox");
        assert_eq!(tokens.len(), 3); // "quick", "brown", "fox" (stop word "the" removed)
    }
}
