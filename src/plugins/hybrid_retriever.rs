//! Hybrid Retriever — combined FTS5 + trust + recency scoring
//!
//! Port of `plugins/tdg/hybrid_retriever.py`.

use std::collections::HashSet;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db::events::record_retrieval;
use crate::error::TdgResult;
use crate::models::Node;
use crate::util::math::cosine_similarity;
use crate::util::stopwords::STOP_WORDS;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QueryIntent {
    Factual,   // Entity lookup → FTS5-first
    Semantic,  // Concept search → Embedding-first
    Global,    // Pattern scan → PageRank-like
    Hybrid,    // Default balanced
}

fn route_query(query: &str) -> QueryIntent {
    let lower = query.to_lowercase();

    // Factual: proper nouns, IDs, specific names
    let uppercase_words = query
        .split_whitespace()
        .filter(|w| w.chars().next().map_or(false, |c| c.is_uppercase()))
        .count();
    if uppercase_words >= 2 || lower.contains("id:") || lower.contains("node_") {
        return QueryIntent::Factual;
    }

    // Global: aggregate/pattern queries
    let global_words = [
        "all", "list", "every", "pattern", "trend", "most common", "overview", "summary",
    ];
    if global_words.iter().any(|w| lower.contains(w)) {
        return QueryIntent::Global;
    }

    // Semantic: abstract concepts
    let semantic_words = [
        "how", "why", "explain", "relationship", "concept", "idea", "think", "meaning",
    ];
    if semantic_words.iter().any(|w| lower.contains(w)) {
        return QueryIntent::Semantic;
    }

    QueryIntent::Hybrid
}

fn weights_for_intent(intent: QueryIntent) -> RetrievalWeights {
    // All weight sets are normalized to sum to exactly 1.0.
    // The previous implementation had weights summing to 1.20 (Factual, Semantic)
    // or 0.80 (Global), causing total_score to exceed 1.0 or be systematically
    // under-weighted. This made cross-query score comparisons misleading and
    // broke min_confidence thresholds.
    match intent {
        QueryIntent::Factual => RetrievalWeights {
            fts_weight: 0.45,
            trust_weight: 0.15,
            recency_weight: 0.05,
            term_overlap_weight: 0.10,
            type_boost_weight: 0.05,
            embedding_weight: 0.20,
        },
        QueryIntent::Semantic => RetrievalWeights {
            fts_weight: 0.15,
            trust_weight: 0.15,
            recency_weight: 0.05,
            term_overlap_weight: 0.05,
            type_boost_weight: 0.10,
            embedding_weight: 0.50,
        },
        QueryIntent::Global => RetrievalWeights {
            fts_weight: 0.15,
            trust_weight: 0.35,
            recency_weight: 0.10,
            term_overlap_weight: 0.05,
            type_boost_weight: 0.05,
            embedding_weight: 0.30,
        },
        QueryIntent::Hybrid => RetrievalWeights::default(),
    }
}

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
        // Normalized to sum to exactly 1.0 (was 1.35 before fix).
        Self {
            fts_weight: 0.30,
            trust_weight: 0.20,
            recency_weight: 0.05,
            term_overlap_weight: 0.10,
            type_boost_weight: 0.15,
            embedding_weight: 0.20,
        }
    }
}

/// High-value node types for boosting.
/// "tool" and "product" were removed (not valid node types in the schema).
static HIGH_VALUE_TYPES: &[&str] = &["action", "telos", "skill", "capability", "discovery", "insight"];

/// The Hybrid Retriever — combined FTS5 + trust + recency scoring.
/// ponytail: weights are per-query via `weights_for_intent()`, not stored on the struct.
pub struct HybridRetriever {
    min_confidence: f64,
}

impl HybridRetriever {
    pub fn new() -> Self {
        Self {
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

        let intent = route_query(query);
        let weights = weights_for_intent(intent);

        let fts_query = self.prepare_fts_query(query);
        let fts_results = self.fts_search(conn, &fts_query, limit * 2)?;

        let mut results: Vec<Node>;
        let rank_map: std::collections::HashMap<String, f64>;

        if fts_results.is_empty() {
            results = self.like_search(conn, query, limit * 2)?;
            rank_map = std::collections::HashMap::new();
        } else {
            rank_map = fts_results
                .iter()
                .map(|(n, r)| (n.id.clone(), *r))
                .collect();
            results = fts_results.into_iter().map(|(n, _)| n).collect();
        };

        if results.is_empty() {
            results = self.type_search(conn, node_type, limit)?;
        }

        let embedding_map = self.build_embedding_map(conn, &results, query);

        let query_tokens = self.tokenize(query);
        let mut scored: Vec<SearchResult> = results
            .into_iter()
            .map(|node| {
                // FTS5 rank: lower = better match; normalize to (0,1] via 1/(1+|rank|)
                let fts_score = rank_map
                    .get(&node.id)
                    .map(|r| 1.0 / (1.0 + r.abs()))
                    .unwrap_or(node.confidence);
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

                let total_score = weights.fts_weight * fts_score
                    + weights.trust_weight * trust_score
                    + weights.recency_weight * recency
                    + weights.term_overlap_weight * overlap
                    + weights.type_boost_weight * type_boost
                    + weights.embedding_weight * cosine;

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

        self.graph_rerank(conn, &mut scored, 0.15);

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 1-hop graph expansion
        scored = self.expand_1_hop(conn, &scored, 20, 0.3);

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

    fn graph_rerank(
        &self,
        conn: &Connection,
        results: &mut Vec<SearchResult>,
        weight: f64,
    ) {
        use crate::db::crud::get_edges;

        for result in results.iter_mut() {
            let out_edges = get_edges(conn, Some(&result.node.id), None, None, None, 100)
                .map(|e| e.len() as f64)
                .unwrap_or(0.0);
            let in_edges = get_edges(conn, None, Some(&result.node.id), None, None, 100)
                .map(|e| e.len() as f64)
                .unwrap_or(0.0);
            let degree = out_edges + in_edges;

            // Normalize: cap at 20 edges → centrality in [0, 1]
            let centrality = (degree / 20.0).min(1.0);

            result.score = (1.0 - weight) * result.score + weight * centrality;
        }
    }

    fn build_embedding_map(
        &self,
        conn: &Connection,
        nodes: &[Node],
        query: &str,
    ) -> std::collections::HashMap<String, f64> {
        if nodes.is_empty() {
            return std::collections::HashMap::new();
        }

        let query_vec = match self.get_query_embedding(query) {
            Some(v) => v,
            None => return std::collections::HashMap::new(),
        };

        let query_dim = query_vec.len() as i32;

        let mut map = std::collections::HashMap::new();
        let ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let placeholders: Vec<String> = ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect();
        // Dimension filtering: only compare vectors with the same dimension as the query
        // This prevents errors when comparing mismatched vectors (e.g., 384-dim vs 768-dim)
        let sql = format!(
            "SELECT node_id, vector FROM embeddings WHERE node_id IN ({}) AND dimension = ?{}",
            placeholders.join(","),
            ids.len() + 1
        );

        if let Ok(mut stmt) = conn.prepare(&sql) {
            let params_refs: Vec<Box<dyn rusqlite::types::ToSql>> = ids
                .iter()
                .map(|id| Box::new(id.clone()) as Box<dyn rusqlite::types::ToSql>)
                .chain(std::iter::once(Box::new(query_dim) as Box<dyn rusqlite::types::ToSql>))
                .collect();
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

    fn get_query_embedding(&self, query: &str) -> Option<Vec<f32>> {
        crate::mind::embedding::embed(query).ok().map(|r| r.vector)
    }

    fn fts_search(
        &self,
        conn: &Connection,
        query: &str,
        limit: i64,
    ) -> TdgResult<Vec<(Node, f64)>> {
        let sql = "
            SELECT n.id, n.node_type, n.name, n.description, n.properties_json, n.quadrants_json,
                   n.drives_json, n.lifecycle_state, n.teleological_level, n.developmental_stage,
                   n.confidence, n.source, n.parent_ids, n.agent_path, n.created_at, n.updated_at,
                   n.valid_from, n.valid_to, n.helpful_count, n.retrieval_count, n.agent_id,
                   n.synthesis_status, n.scale_code, n.tetra_ul, n.tetra_ur, n.tetra_ll,
                   n.tetra_lr, n.octave_id,
                   n.realm_placement, n.verticality_json, n.collectivity, n.nesting_sub, n.nesting_sup,
                   fts.rank
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

        let rows = stmt.query_map(params![query, limit], |row| {
            let node = crate::db::crud::row_to_node(row)?;
            let rank: f64 = row.get(33)?;
            Ok((node, rank))
        })?;
        let results: Vec<(Node, f64)> = rows.flatten().collect();
        Ok(results)
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
                   valid_from, valid_to, helpful_count, retrieval_count, agent_id,
                   synthesis_status, scale_code, tetra_ul, tetra_ur, tetra_ll, tetra_lr, octave_id,
         realm_placement, verticality_json, collectivity, nesting_sub, nesting_sup
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
                 valid_from, valid_to, helpful_count, retrieval_count, agent_id,
                 synthesis_status, scale_code, tetra_ul, tetra_ur, tetra_ll, tetra_lr, octave_id,
         realm_placement, verticality_json, collectivity, nesting_sub, nesting_sup
                 FROM nodes WHERE valid_to IS NULL AND node_type = ?1
                 ORDER BY confidence DESC LIMIT ?2"
                    .to_string(),
                Some(nt.to_string()),
            ),
            None => (
                "SELECT id, node_type, name, description, properties_json, quadrants_json,
                 drives_json, lifecycle_state, teleological_level, developmental_stage,
                 confidence, source, parent_ids, agent_path, created_at, updated_at,
                 valid_from, valid_to, helpful_count, retrieval_count, agent_id,
                 synthesis_status, scale_code, tetra_ul, tetra_ur, tetra_ll, tetra_lr, octave_id,
         realm_placement, verticality_json, collectivity, nesting_sub, nesting_sup
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
        // Parse the timestamp robustly. `crud::now_iso()` produces RFC3339 with
        // +00:00 offset (e.g. "2026-01-15T12:34:56.789+00:00"). The previous
        // implementation used NaiveDateTime::parse_from_str with format
        // "%Y-%m-%dT%H:%M:%S%.f" after stripping 'Z' — but that format does NOT
        // accept timezone offsets like "+00:00", so EVERY parse failed and
        // recency_score always returned 0.5. Recency never affected ranking.
        //
        // We now try DateTime::parse_from_rfc3339 first (handles any RFC3339
        // including +00:00, +05:30, Z), then fall back to NaiveDateTime formats
        // for legacy/external timestamps.
        let parsed = chrono::DateTime::parse_from_rfc3339(created_at)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc).naive_utc())
            .or_else(|| {
                chrono::NaiveDateTime::parse_from_str(
                    created_at.replace('Z', "").as_str(),
                    "%Y-%m-%dT%H:%M:%S%.f",
                )
                .ok()
            })
            .or_else(|| {
                chrono::NaiveDateTime::parse_from_str(created_at, "%Y-%m-%d %H:%M:%S").ok()
            });

        if let Some(created) = parsed {
            let now = chrono::Utc::now().naive_utc();
            let age_days = (now - created).num_days() as f64;
            // Decay: 1.0 at day 0, 0.5 at day 30, approaches 0
            (1.0 / (1.0 + age_days / 30.0)).max(0.0)
        } else {
            tracing::debug!("recency_score: unparseable timestamp '{}'", created_at);
            0.5 // default mid-score
        }
    }

    /// Expand top-K results with 1-hop graph neighbors.
    ///
    /// For each node in `results`, fetches outgoing and incoming edges,
    /// retrieves neighbor nodes, and adds them with a decayed score.
    /// Only edges with weight >= `min_edge_weight` are considered.
    fn expand_1_hop(
        &self,
        conn: &Connection,
        results: &[SearchResult],
        max_expansion: usize,
        min_edge_weight: f64,
    ) -> Vec<SearchResult> {
        let mut expanded: Vec<SearchResult> = results.to_vec();
        let mut seen: HashSet<String> = results.iter().map(|r| r.node.id.clone()).collect();
        let mut added = 0usize;

        for parent in results {
            if added >= max_expansion {
                break;
            }

            // Outgoing edges (node is source)
            if let Ok(out_edges) = crate::db::crud::get_edges(
                conn,
                Some(&parent.node.id),
                None,
                None,
                None,
                50,
            ) {
                for edge in &out_edges {
                    if added >= max_expansion {
                        break;
                    }
                    if edge.weight < min_edge_weight || seen.contains(&edge.target_id) {
                        continue;
                    }
                    if let Ok(Some(neighbor)) =
                        crate::db::crud::get_node(conn, &edge.target_id)
                    {
                        let score = parent.score * 0.7;
                        seen.insert(neighbor.id.clone());
                        added += 1;
                        expanded.push(SearchResult {
                            node: neighbor,
                            score,
                            method: "1-hop".to_string(),
                        });
                    }
                }
            }

            // Incoming edges (node is target)
            if let Ok(in_edges) = crate::db::crud::get_edges(
                conn,
                None,
                Some(&parent.node.id),
                None,
                None,
                50,
            ) {
                for edge in &in_edges {
                    if added >= max_expansion {
                        break;
                    }
                    if edge.weight < min_edge_weight || seen.contains(&edge.source_id) {
                        continue;
                    }
                    if let Ok(Some(neighbor)) =
                        crate::db::crud::get_node(conn, &edge.source_id)
                    {
                        let score = parent.score * 0.7;
                        seen.insert(neighbor.id.clone());
                        added += 1;
                        expanded.push(SearchResult {
                            node: neighbor,
                            score,
                            method: "1-hop".to_string(),
                        });
                    }
                }
            }
        }

        expanded
    }
}

impl Default for HybridRetriever {
    fn default() -> Self {
        Self::new()
    }
}

fn decode_f32_vec(blob: &[u8]) -> Result<Vec<f32>, ()> {
    if !blob.len().is_multiple_of(4) {
        return Err(());
    }
    let chunks = blob.chunks_exact(4);
    Ok(chunks
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
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
