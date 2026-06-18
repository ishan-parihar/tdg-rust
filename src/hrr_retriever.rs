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
