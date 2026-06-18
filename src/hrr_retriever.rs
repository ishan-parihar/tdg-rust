//! Graph-aware HRR retriever.
//!
//! Combines HRR vector algebra (binding, unbinding, bundling) with graph
//! structure from the database to enable high-level retrieval operations:
//!
//! - **probe**: text query → most similar nodes
//! - **related**: node → structurally adjacent nodes via HRR role unbinding
//! - **reason**: two nodes → emergent knowledge via bundled probe
//! - **contradict**: node → potential contradictions via negation probe
//!
//! All database queries are dispatched through `tokio::task::spawn_blocking`
//! to avoid blocking the async runtime.

use std::collections::HashMap;
use std::sync::Arc;

use ndarray::Array1;

use crate::db::ConnectionPool;
use crate::error::{TdgError, TdgResult};
use crate::hrr::{self, HrrMemoryBank, HRR_DIM, ROLE_ENTITY};

// ─── Public Types ─────────────────────────────────────────────────────────

/// Result of a probe / reason / contradict query.
#[derive(Debug, Clone)]
pub struct ProbedNode {
    pub node_id: String,
    pub label: String,
    pub score: f64,
}

/// Result of a related query (includes the graph edge type).
#[derive(Debug, Clone)]
pub struct RelatedNode {
    pub node_id: String,
    pub label: String,
    pub score: f64,
    pub edge_type: String,
}

// ─── HrrRetriever ─────────────────────────────────────────────────────────

/// High-level retrieval operations combining HRR with graph structure.
pub struct HrrRetriever {
    pool: Arc<ConnectionPool>,
}

impl HrrRetriever {
    /// Create a new retriever that wraps the given connection pool.
    pub fn new(pool: ConnectionPool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// Probe: find nodes most similar to a text query.
    ///
    /// 1. Convert `query` to an HRR vector via deterministic hash-based encoding.
    /// 2. Load every node embedding from the database.
    /// 3. Compute cosine similarity against the query vector.
    /// 4. Return the `top_k` most similar nodes with their scores.
    pub async fn probe(&self, query: &str, top_k: usize) -> TdgResult<Vec<ProbedNode>> {
        let bank = self.build_bank().await?;
        let query_vec = Self::text_to_hrr(query);

        let mut results: Vec<ProbedNode> = bank
            .entries()
            .iter()
            .map(|(name, vec)| {
                let score = hrr::cosine_similarity(&query_vec, vec);
                let (node_id, label) = parse_entry_name(name);
                ProbedNode {
                    node_id,
                    label,
                    score,
                }
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        Ok(results)
    }

    /// Related: given a node, find structurally adjacent nodes via HRR.
    ///
    /// 1. Load the target node's embedding from the database.
    /// 2. Unbind the vector with the entity key (`ROLE_ENTITY`).
    /// 3. Find the most similar vectors to the unbound result.
    /// 4. Filter to only candidates that share at least one graph edge with the target.
    /// 5. Include the edge type in each result.
    pub async fn related(&self, node_id: &str, top_k: usize) -> TdgResult<Vec<RelatedNode>> {
        let node_id = node_id.to_string();
        let pool = self.pool.clone();
        let node_id_for_blocking = node_id.clone();

        // ── spawn_blocking: build bank + fetch edges for target node ──
        let (bank, edge_map) = tokio::task::spawn_blocking(move || {
            build_bank_inner(&pool).and_then(|bank| {
                let edges = fetch_edges_for_node_inner(&pool, &node_id_for_blocking)?;
                Ok((bank, edges))
            })
        })
        .await
        .map_err(|e| TdgError::Custom(format!("HRR retriever blocking task panicked: {e}")))??;

        // ── HRR computation (pure, no blocking needed) ──
        let entity_key = Self::text_to_hrr(ROLE_ENTITY);

        // Locate the target node's entry
        let target_entry = bank
            .entries()
            .iter()
            .find(|(name, _)| parse_entry_name(name).0 == node_id);

        let (target_name, target_vec) = match target_entry {
            Some((n, v)) => (n.clone(), v.clone()),
            None => return Ok(Vec::new()),
        };

        // Unbind with entity key to obtain the "context" vector
        let unbound = hrr::unbind(&target_vec, &entity_key);

        // Score all other entries against the unbound vector
        let mut candidates: Vec<(String, f64)> = bank
            .entries()
            .iter()
            .filter(|(name, _)| *name != target_name)
            .map(|(name, vec)| (name.clone(), hrr::cosine_similarity(&unbound, vec)))
            .collect();

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Filter to only graph-connected nodes and annotate with edge type
        let mut results: Vec<RelatedNode> = candidates
            .into_iter()
            .filter_map(|(name, score)| {
                let (nid, label) = parse_entry_name(&name);
                edge_map.get(&nid).map(|et| RelatedNode {
                    node_id: nid,
                    label,
                    score,
                    edge_type: et.clone(),
                })
            })
            .collect();

        results.truncate(top_k);
        Ok(results)
    }

    /// Reason: combine two node vectors to discover emergent knowledge.
    ///
    /// 1. Load both node embeddings.
    /// 2. Bundle them: `normalize(normalize(a) + normalize(b))`.
    /// 3. Probe the memory bank with the bundled vector.
    /// 4. Return the top-k results, excluding the two input nodes.
    pub async fn reason(
        &self,
        node_a: &str,
        node_b: &str,
        top_k: usize,
    ) -> TdgResult<Vec<ProbedNode>> {
        let bank = self.build_bank().await?;

        let vec_a = find_node_vector(&bank, node_a);
        let vec_b = find_node_vector(&bank, node_b);

        let (va, vb) = match (vec_a, vec_b) {
            (Some(a), Some(b)) => (a, b),
            _ => return Ok(Vec::new()),
        };

        // Bundle = normalised sum
        let combined = hrr::normalize(&(&hrr::normalize(&va) + &hrr::normalize(&vb)));

        let mut results: Vec<ProbedNode> = bank
            .entries()
            .iter()
            .filter(|(name, _)| {
                let id = parse_entry_name(name).0;
                id != node_a && id != node_b
            })
            .map(|(name, vec)| {
                let score = hrr::cosine_similarity(&combined, vec);
                let (node_id, label) = parse_entry_name(name);
                ProbedNode {
                    node_id,
                    label,
                    score,
                }
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        Ok(results)
    }

    /// Contradict: find nodes that semantically conflict with a given node.
    ///
    /// 1. Load the node's embedding.
    /// 2. Negate it (element-wise multiplication by -1).
    /// 3. Probe the bank with the negated vector.
    /// 4. Return the top-k results (these represent potential contradictions).
    pub async fn contradict(&self, node_id: &str, top_k: usize) -> TdgResult<Vec<ProbedNode>> {
        let bank = self.build_bank().await?;

        let vector = find_node_vector(&bank, node_id);
        let vec = match vector {
            Some(v) => v,
            None => return Ok(Vec::new()),
        };

        // Negate
        let negated = -&vec;

        let mut results: Vec<ProbedNode> = bank
            .entries()
            .iter()
            .filter(|(name, _)| {
                let id = parse_entry_name(name).0;
                id != node_id
            })
            .map(|(name, vec)| {
                let score = hrr::cosine_similarity(&negated, vec);
                let (node_id, label) = parse_entry_name(name);
                ProbedNode {
                    node_id,
                    label,
                    score,
                }
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        Ok(results)
    }

    // ─── Private Helpers ───────────────────────────────────────────────

    /// Build a memory bank from all nodes that have embeddings in the database.
    async fn build_bank(&self) -> TdgResult<HrrMemoryBank> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || build_bank_inner(&pool))
            .await
            .map_err(|e| TdgError::Custom(format!("HRR retriever blocking task panicked: {e}")))?
    }

    /// Deterministically encode arbitrary text into an HRR vector.
    ///
    /// Uses `ahash` with fixed seeds so that the same text always produces
    /// the same vector (essential for role-key reproducibility).
    fn text_to_hrr(text: &str) -> Array1<f64> {
        // Fixed seeds guarantee deterministic output across runs.
        let state = ahash::RandomState::with_seeds(42, 42, 42, 42);
        let hash: u64 = state.hash_one(text);

        // Expand the 64-bit hash into HRR_DIM values via an LCG.
        let mut result = Array1::zeros(HRR_DIM);
        let mut seed = hash;
        for i in 0..HRR_DIM {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            // Map the upper bits of the seed into [-1, 1]
            let val = (seed >> 11) as f64 / (1u64 << 53) as f64;
            result[i] = val * 2.0 - 1.0;
        }

        hrr::normalize(&result)
    }
}

// ─── Internal Helpers (sync, used inside spawn_blocking) ──────────────────

/// Load all node embeddings from the database and populate an `HrrMemoryBank`.
///
/// Each entry is stored as `"node_id||label"` so that the two components can
/// be recovered later by [`parse_entry_name`].
fn build_bank_inner(pool: &ConnectionPool) -> TdgResult<HrrMemoryBank> {
    pool.with_connection(|conn| {
        let mut stmt = conn.prepare(
            "SELECT n.id, n.name, e.vector
             FROM nodes n
             JOIN embeddings e ON n.id = e.node_id
             WHERE n.valid_to IS NULL
               AND e.vector IS NOT NULL",
        )?;

        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let vector_blob: Vec<u8> = row.get(2)?;
            Ok((id, name, vector_blob))
        })?;

        let mut bank = HrrMemoryBank::new();
        for row in rows {
            let (id, name, blob) = row?;
            // Only accept vectors whose byte-length matches HRR_DIM f64 values
            // (embeddings are stored as f32, so 4 bytes per element)
            if blob.len() == HRR_DIM * 4 {
                let hrr_vec = blob_to_hrr_vector(&blob);
                bank.store(format!("{id}||{name}"), hrr_vec);
            }
        }

        Ok(bank)
    })
}

/// Fetch all graph neighbours of `node_id` together with the edge type.
///
/// Returns a map: `neighbour_id → edge_type`.
fn fetch_edges_for_node_inner(
    pool: &ConnectionPool,
    node_id: &str,
) -> TdgResult<HashMap<String, String>> {
    pool.with_connection(|conn| {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT
                CASE WHEN source_id = ?1 THEN target_id ELSE source_id END AS neighbour,
                edge_type
             FROM edges
             WHERE (source_id = ?1 OR target_id = ?1)
               AND valid_to IS NULL",
        )?;

        let rows = stmt.query_map(rusqlite::params![node_id], |row| {
            let neighbour: String = row.get(0)?;
            let edge_type: String = row.get(1)?;
            Ok((neighbour, edge_type))
        })?;

        let mut map = HashMap::new();
        for row in rows {
            let (neighbour, edge_type) = row?;
            // Keep the first edge type seen (arbitrary for multi-edge pairs)
            map.entry(neighbour).or_insert(edge_type);
        }

        Ok(map)
    })
}

// ─── Pure Helpers ─────────────────────────────────────────────────────────

/// Convert an embedding BLOB (f32 little-endian bytes) to an HRR vector (f64).
fn blob_to_hrr_vector(blob: &[u8]) -> Array1<f64> {
    let f32_vec: Vec<f32> = blob
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let f64_vec: Vec<f64> = f32_vec.iter().map(|&x| x as f64).collect();
    Array1::from_vec(f64_vec)
}

/// Parse a bank entry name stored as `"node_id||label"`.
fn parse_entry_name(name: &str) -> (String, String) {
    if let Some((id, label)) = name.split_once("||") {
        (id.to_string(), label.to_string())
    } else {
        (name.to_string(), String::new())
    }
}

    /// Find the vector for a node by its ID inside an already-built bank.
fn find_node_vector(bank: &HrrMemoryBank, node_id: &str) -> Option<Array1<f64>> {
    bank.entries()
        .iter()
        .find(|(name, _)| parse_entry_name(name).0 == node_id)
        .map(|(_, vec)| vec.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_schema, run_migrations, ConnectionPool};
    use crate::hrr;
    use ndarray::Array1;

    // ── Helper: set up a temp-file pool with schema ──────────────────
    fn setup_pool() -> (ConnectionPool, tempfile::NamedTempFile) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let pool = ConnectionPool::new(&path, 5, 30000).unwrap();
        // Initialize schema on a direct connection
        let conn = rusqlite::Connection::open(&path).unwrap();
        init_schema(&conn).unwrap();
        run_migrations(&conn).unwrap();
        drop(conn);
        (pool, tmp)
    }

    // ── Helper: insert a node + embedding into the DB via pool ───────
    fn insert_node_with_embedding(
        pool: &ConnectionPool,
        node_id: &str,
        name: &str,
        node_type: &str,
        vector: &Array1<f64>,
    ) {
        pool.with_connection(|conn| {
            conn.execute(
                "INSERT INTO nodes (id, name, node_type, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, datetime('now'), datetime('now'))",
                rusqlite::params![node_id, name, node_type, format!("desc for {name}")],
            )?;
            let blob: Vec<u8> = vector.iter().flat_map(|v| (*v as f32).to_le_bytes()).collect();
            conn.execute(
                "INSERT INTO embeddings (node_id, vector, model, updated_at) VALUES (?1, ?2, 'test', datetime('now'))",
                rusqlite::params![node_id, blob],
            )?;
            Ok(())
        })
        .unwrap();
    }

    // ── Helper: insert an edge ───────────────────────────────────────
    fn insert_edge(pool: &ConnectionPool, src: &str, tgt: &str, edge_type: &str) {
        pool.with_connection(|conn| {
            conn.execute(
                "INSERT INTO edges (id, source_id, target_id, edge_type, created_at) VALUES (?1, ?2, ?3, ?4, datetime('now'))",
                rusqlite::params![format!("{src}-{tgt}"), src, tgt, edge_type],
            )?;
            Ok(())
        })
        .unwrap();
    }

    // ── parse_entry_name ─────────────────────────────────────────────

    #[test]
    fn parse_entry_name_with_delimiter() {
        let (id, label) = parse_entry_name("abc-123||Test Node");
        assert_eq!(id, "abc-123");
        assert_eq!(label, "Test Node");
    }

    #[test]
    fn parse_entry_name_without_delimiter() {
        let (id, label) = parse_entry_name("bare-id");
        assert_eq!(id, "bare-id");
        assert_eq!(label, "");
    }

    #[test]
    fn parse_entry_name_empty() {
        let (id, label) = parse_entry_name("");
        assert_eq!(id, "");
        assert_eq!(label, "");
    }

    #[test]
    fn parse_entry_name_multiple_delimiters() {
        let (id, label) = parse_entry_name("a||b||c");
        assert_eq!(id, "a");
        assert_eq!(label, "b||c");
    }

    // ── blob_to_hrr_vector ──────────────────────────────────────────

    #[test]
    fn blob_to_hrr_vector_roundtrip() {
        let original: Vec<f32> = vec![1.0, -0.5, 0.0, 3.14];
        let blob: Vec<u8> = original.iter().flat_map(|v| v.to_le_bytes()).collect();
        let vec = blob_to_hrr_vector(&blob);
        assert_eq!(vec.len(), 4);
        for (got, expected) in vec.iter().zip(original.iter()) {
            assert!((got - *expected as f64).abs() < 1e-6);
        }
    }

    #[test]
    fn blob_to_hrr_vector_full_dim() {
        let f32_vec: Vec<f32> = (0..HRR_DIM as i32).map(|i| i as f32 * 0.01).collect();
        let blob: Vec<u8> = f32_vec.iter().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(blob.len(), HRR_DIM * 4);
        let vec = blob_to_hrr_vector(&blob);
        assert_eq!(vec.len(), HRR_DIM);
    }

    // ── text_to_hrr determinism ──────────────────────────────────────

    #[test]
    fn text_to_hrr_deterministic() {
        let v1 = HrrRetriever::text_to_hrr("hello world");
        let v2 = HrrRetriever::text_to_hrr("hello world");
        assert_eq!(v1.len(), HRR_DIM);
        assert_eq!(v2.len(), HRR_DIM);
        for (a, b) in v1.iter().zip(v2.iter()) {
            assert!((a - b).abs() < 1e-15);
        }
    }

    #[test]
    fn text_to_hrr_different_inputs_differ() {
        let v1 = HrrRetriever::text_to_hrr("alpha");
        let v2 = HrrRetriever::text_to_hrr("beta");
        let sim = hrr::cosine_similarity(&v1, &v2);
        // Orthogonal-ish: score should be small (not identical)
        assert!(sim.abs() < 0.15, "similarity {sim} too high for different inputs");
    }

    #[test]
    fn text_to_hrr_normalized() {
        let v = HrrRetriever::text_to_hrr("test");
        let norm = v.mapv(|x| x * x).sum().sqrt();
        assert!((norm - 1.0).abs() < 1e-10, "norm should be ~1.0, got {norm}");
    }

    // ── find_node_vector ─────────────────────────────────────────────

    #[test]
    fn find_node_vector_found() {
        let mut bank = HrrMemoryBank::new();
        let v = Array1::from_vec((0..HRR_DIM).map(|i| i as f64).collect::<Vec<_>>());
        bank.store("node1||Label A".to_string(), v.clone());
        let found = find_node_vector(&bank, "node1");
        assert!(found.is_some());
        assert_eq!(found.unwrap(), v);
    }

    #[test]
    fn find_node_vector_not_found() {
        let bank = HrrMemoryBank::new();
        assert!(find_node_vector(&bank, "nonexistent").is_none());
    }

    // ── HrrRetriever construction ───────────────────────────────────

    #[test]
    fn retriever_new_succeeds() {
        let (pool, _tmp) = setup_pool();
        let _retriever = HrrRetriever::new(pool);
        // Just verify it compiles and runs
    }

    // ── probe with empty bank ────────────────────────────────────────

    #[tokio::test]
    async fn probe_empty_bank_returns_empty() {
        let (pool, _tmp) = setup_pool();
        let retriever = HrrRetriever::new(pool);
        let results = retriever.probe("anything", 10).await.unwrap();
        assert!(results.is_empty());
    }

    // ── probe with nodes ─────────────────────────────────────────────

    #[tokio::test]
    async fn probe_returns_sorted_results() {
        let (pool, _tmp) = setup_pool();
        let v1 = hrr::normalize(&HrrRetriever::text_to_hrr("machine learning"));
        let v2 = hrr::normalize(&HrrRetriever::text_to_hrr("deep learning"));
        insert_node_with_embedding(&pool, "n1", "ML", "concept", &v1);
        insert_node_with_embedding(&pool, "n2", "DL", "concept", &v2);

        let retriever = HrrRetriever::new(pool);
        let results = retriever.probe("machine learning", 10).await.unwrap();
        assert_eq!(results.len(), 2);
        // Scores should be in descending order
        assert!(results[0].score >= results[1].score);
    }

    #[tokio::test]
    async fn probe_respects_top_k() {
        let (pool, _tmp) = setup_pool();
        for i in 0..5 {
            let v = hrr::normalize(&HrrRetriever::text_to_hrr(&format!("concept_{i}")));
            insert_node_with_embedding(&pool, &format!("n{i}"), &format!("C{i}"), "concept", &v);
        }
        let retriever = HrrRetriever::new(pool);
        let results = retriever.probe("concept", 3).await.unwrap();
        assert!(results.len() <= 3);
    }

    // ── reason ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn reason_excludes_input_nodes() {
        let (pool, _tmp) = setup_pool();
        let v1 = hrr::normalize(&HrrRetriever::text_to_hrr("node_a"));
        let v2 = hrr::normalize(&HrrRetriever::text_to_hrr("node_b"));
        let v3 = hrr::normalize(&HrrRetriever::text_to_hrr("node_c"));
        insert_node_with_embedding(&pool, "a", "A", "concept", &v1);
        insert_node_with_embedding(&pool, "b", "B", "concept", &v2);
        insert_node_with_embedding(&pool, "c", "C", "concept", &v3);

        let retriever = HrrRetriever::new(pool);
        let results = retriever.reason("a", "b", 10).await.unwrap();
        assert!(!results.iter().any(|r| r.node_id == "a"));
        assert!(!results.iter().any(|r| r.node_id == "b"));
    }

    #[tokio::test]
    async fn reason_missing_node_returns_empty() {
        let (pool, _tmp) = setup_pool();
        let v = hrr::normalize(&HrrRetriever::text_to_hrr("only_node"));
        insert_node_with_embedding(&pool, "n1", "N1", "concept", &v);

        let retriever = HrrRetriever::new(pool);
        let results = retriever.reason("n1", "nonexistent", 10).await.unwrap();
        assert!(results.is_empty());
    }

    // ── contradict ───────────────────────────────────────────────────

    #[tokio::test]
    async fn contradict_excludes_self() {
        let (pool, _tmp) = setup_pool();
        let v1 = hrr::normalize(&HrrRetriever::text_to_hrr("statement_a"));
        let v2 = hrr::normalize(&HrrRetriever::text_to_hrr("statement_b"));
        insert_node_with_embedding(&pool, "a", "A", "claim", &v1);
        insert_node_with_embedding(&pool, "b", "B", "claim", &v2);

        let retriever = HrrRetriever::new(pool);
        let results = retriever.contradict("a", 10).await.unwrap();
        assert!(!results.iter().any(|r| r.node_id == "a"));
    }

    #[tokio::test]
    async fn contradict_missing_node_returns_empty() {
        let (pool, _tmp) = setup_pool();
        let retriever = HrrRetriever::new(pool);
        let results = retriever.contradict("nonexistent", 10).await.unwrap();
        assert!(results.is_empty());
    }

    // ── related ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn related_no_edges_returns_empty() {
        let (pool, _tmp) = setup_pool();
        let v = hrr::normalize(&HrrRetriever::text_to_hrr("isolated"));
        insert_node_with_embedding(&pool, "n1", "N1", "concept", &v);
        // No edges inserted

        let retriever = HrrRetriever::new(pool);
        let results = retriever.related("n1", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn related_with_edges_returns_connected_nodes() {
        let (pool, _tmp) = setup_pool();
        let v1 = hrr::normalize(&HrrRetriever::text_to_hrr("source_node"));
        let v2 = hrr::normalize(&HrrRetriever::text_to_hrr("target_node"));
        insert_node_with_embedding(&pool, "src", "Source", "concept", &v1);
        insert_node_with_embedding(&pool, "tgt", "Target", "concept", &v2);
        insert_edge(&pool, "src", "tgt", "relates_to");

        let retriever = HrrRetriever::new(pool);
        let results = retriever.related("src", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node_id, "tgt");
        assert_eq!(results[0].edge_type, "relates_to");
    }

    // ── snapshot test for ProbedNode debug format ────────────────────

    #[test]
    fn snapshot_probed_node_debug() {
        let node = ProbedNode {
            node_id: "abc-123".to_string(),
            label: "Test Node".to_string(),
            score: 0.8765,
        };
        insta::assert_debug_snapshot!(node, @r###"
        ProbedNode {
            node_id: "abc-123",
            label: "Test Node",
            score: 0.8765,
        }
        "###);
    }

    #[test]
    fn snapshot_related_node_debug() {
        let node = RelatedNode {
            node_id: "n42".to_string(),
            label: "Connected".to_string(),
            score: 0.42,
            edge_type: "depends_on".to_string(),
        };
        insta::assert_debug_snapshot!(node, @r###"
        RelatedNode {
            node_id: "n42",
            label: "Connected",
            score: 0.42,
            edge_type: "depends_on",
        }
        "###);
    }
}
