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
    pub embedding_weight: f64,
}

impl Default for RetrievalWeights {
    fn default() -> Self {
        Self {
            fts_weight: 0.50,
            trust_weight: 0.30,
            recency_weight: 0.10,
            term_overlap_weight: 0.10,
            type_boost_weight: 0.15,
            embedding_weight: 0.20,
        }
    }
}

/// Stop words for filtering — expanded to match Python's 60+ word list.
static STOP_WORDS: &[&str] = &[
    // Articles & pronouns
    "the", "a", "an", "i", "you", "he", "she", "it", "we", "they", "me", "him", "her", "us",
    "them", "my", "your", "his", "its", "our", "their", "mine", "yours", "hers", "ours", "theirs",
    // Verbs
    "is", "are", "was", "were", "be", "been", "being", "have", "has", "had", "do", "does", "did",
    "will", "would", "could", "should", "may", "might", "can", "shall", "must", "need", "ought",
    "used", "to", "of", "in", "for", "on", "with", "at", "by", "from", "as", "into", "through",
    "during", "before", "after", "above", "below", "between", "out", "off", "over", "under",
    // Conjunctions & prepositions
    "again", "further", "then", "once", "here", "there", "when", "where", "why", "how", "all",
    "both", "each", "few", "more", "most", "other", "some", "such", "no", "nor", "not", "only",
    "own", "same", "so", "than", "too", "very", "just", "because", "but", "and", "or", "if",
    "while", "about", "against", "also",
];

/// High-value node types for boosting — matches Python's BOOSTED_TYPES.
static HIGH_VALUE_TYPES: &[&str] = &["action", "telos", "skill", "tool", "product", "capability"];

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

    pub fn search(
        &self,
        conn: &Connection,
        query: &str,
        limit: i64,
        node_type: Option<&str>,
    ) -> TdgResult<Vec<SearchResult>> {
        let limit = limit.min(50);

        let fts_query = self.prepare_fts_query(query);
        let mut results = self.fts_search(conn, &fts_query, limit * 2)?;

        if results.is_empty() {
            results = self.like_search(conn, query, limit * 2)?;
        }

        if results.is_empty() {
            results = self.type_search(conn, node_type, limit)?;
        }

        let embedding_map = self.build_embedding_map(conn, &results);

        let query_tokens = self.tokenize(query);
        let mut scored: Vec<SearchResult> = results
            .into_iter()
            .map(|node| {
                let fts_score = node.confidence;
                let trust_score = node.confidence;
                let recency = self.recency_score(&node.created_at);

                let node_tokens = self.tokenize(&format!("{} {}", node.name, node.description));
                let overlap = query_tokens
                    .iter()
                    .filter(|qt| node_tokens.contains(qt))
                    .count() as f64
                    / query_tokens.len().max(1) as f64;

                let type_boost = if HIGH_VALUE_TYPES.contains(&node.node_type.as_str()) {
                    0.15
                } else {
                    0.0
                };

                let cosine = embedding_map.get(&node.id).copied().unwrap_or(0.0);

                let total_score = self.weights.fts_weight * fts_score
                    + self.weights.trust_weight * trust_score
                    + self.weights.recency_weight * recency
                    + self.weights.term_overlap_weight * overlap
                    + self.weights.type_boost_weight * type_boost
                    + self.weights.embedding_weight * cosine;

                let method = if cosine > 0.0 {
                    "hybrid+embedding"
                } else {
                    "hybrid"
                };

                SearchResult {
                    node,
                    score: total_score,
                    method: method.to_string(),
                }
            })
            .filter(|r| r.node.confidence >= self.min_confidence)
            .collect();

        let mut seen = HashSet::new();
        scored.retain(|r| seen.insert(r.node.id.clone()));

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit as usize);

        for result in &scored {
            let _ = record_retrieval(conn, &result.node.id);
        }

        Ok(scored)
    }

    fn build_embedding_map(
        &self,
        conn: &Connection,
        nodes: &[Node],
    ) -> std::collections::HashMap<String, f64> {
        if nodes.is_empty() {
            return std::collections::HashMap::new();
        }

        let query_vec = match self.get_query_embedding_stub(conn) {
            Some(v) => v,
            None => return std::collections::HashMap::new(),
        };

        let mut map = std::collections::HashMap::new();
        let ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let placeholders: Vec<String> = ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let sql = format!(
            "SELECT node_id, embedding FROM embeddings WHERE node_id IN ({})",
            placeholders.join(",")
        );

        if let Ok(mut stmt) = conn.prepare(&sql) {
            let params_refs: Vec<Box<dyn rusqlite::types::ToSql>> =
                ids.iter().map(|id| Box::new(id.clone()) as Box<dyn rusqlite::types::ToSql>).collect();
            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params_refs.iter().map(|p| p.as_ref()).collect();
            if let Ok(rows) = stmt.query_map(&*param_refs, |row| {
                let node_id: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                Ok((node_id, blob))
            }) {
                for row in rows.flatten() {
                    if let Ok(vec) = decode_f32_vec(&row.1) {
                        let sim = cosine_similarity(&query_vec, &vec);
                        if sim > 0.0 {
                            map.insert(row.0, sim as f64);
                        }
                    }
                }
            }
        }

        map
    }

    fn get_query_embedding_stub(&self, _conn: &Connection) -> Option<Vec<f32>> {
        None
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
        let nodes: Vec<Node> = rows.flatten().collect();
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
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            all_params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(&*param_refs, crate::db::crud::row_to_node)?;
        let nodes: Vec<Node> = rows.flatten().collect();
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

        let nodes: Vec<Node> = rows.flatten().collect();
        Ok(nodes)
    }

    /// Prepare FTS5 query: strip special chars, handle phrases, add wildcards.
    fn prepare_fts_query(&self, query: &str) -> String {
        let cleaned: String = query
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '_' || *c == '"')
            .collect();
        let meaningful = self.get_meaningful_terms(&cleaned);
        if meaningful.is_empty() {
            return query.to_string();
        }
        meaningful
            .iter()
            .map(|term| format!("\"{}\"*", term))
            .collect::<Vec<_>>()
            .join(" OR ")
    }

    /// Extract meaningful terms (3+ chars, not stop words).
    fn get_meaningful_terms(&self, text: &str) -> Vec<String> {
        text.to_lowercase()
            .split_whitespace()
            .map(|w| {
                let clean: String = w
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                clean
            })
            .filter(|t| t.len() >= 3 && !STOP_WORDS.contains(&t.as_str()))
            .collect()
    }

    fn tokenize(&self, text: &str) -> Vec<String> {
        text.to_lowercase()
            .split_whitespace()
            .map(|w| {
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

impl Default for HybridRetriever {
    fn default() -> Self {
        Self::new()
    }
}

fn decode_f32_vec(blob: &[u8]) -> Result<Vec<f32>, ()> {
    if blob.len() % 4 != 0 {
        return Err(());
    }
    let chunks = blob.chunks_exact(4);
    Ok(chunks
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        0.0
    } else {
        dot / (mag_a * mag_b)
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
