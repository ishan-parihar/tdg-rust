# TDG-Rust Gap Analysis: Missing Python Upgrades

**Date:** 2026-06-28
**Status:** Critical gaps identified — implementation required

---

## Executive Summary

TDG-Rust has **significant gaps** in embedding and vector search functionality compared to Python TDG. The Rust version has:
- ✅ SelfManager module (fully ported)
- ✅ FTS5 search (working)
- ✅ Brute-force cosine similarity (working)
- ❌ **EmbeddingGemma-300M GGUF** (NOT ported — only ONNX)
- ❌ **sqlite-vec ANN search** (NOT ported — brute-force only)
- ❌ **Hybrid search embedding component** (STUB — `get_query_embedding_stub()` returns None)
- ❌ **Embedding backfill system** (NOT ported)

**Impact:** The Rust version's hybrid search is degraded — it has the weights for embedding boost (20%) but the actual embedding component is disabled.

---

## Critical Gaps

### 1. Embedding Engine: GGUF Backend Missing

**Python TDG:** Supports two backends:
- ONNX (all-MiniLM-L6-v2, 384-dim) — via onnxruntime
- GGUF (EmbeddingGemma-300M, 768-dim) — via llama-cpp-python

**Rust TDG:** Only supports ONNX (feature-gated). No GGUF backend.

**What needs to be ported:**
- GGUF model loading via llama-cpp bindings
- EmbeddingGemma-300M support (768-dim)
- Configuration for GGUF strategy in config

**File to modify:** `src/mind/embedding.rs`

### 2. Vector Search: sqlite-vec Missing

**Python TDG:** Uses sqlite-vec for ANN (Approximate Nearest Neighbor) search:
- `vec_nodes` virtual table for vector storage
- `search_vector()` for ANN queries
- Graceful fallback if sqlite-vec unavailable

**Rust TDG:** Uses brute-force cosine similarity only:
- No `vec_nodes` table
- No ANN search
- All similarity computed in-memory

**What needs to be ported:**
- sqlite-vec Rust crate integration
- `vec_nodes` table creation
- `search_vector()` function
- Connection pool integration (load extension on every connection)

**File to modify:** `src/db/crud.rs`, `src/db/schema.rs`

### 3. Hybrid Search: Embedding Component Stub

**Python TDG:** Hybrid search uses embedding cosine similarity (20% weight):
- Embeds query text
- Searches vec_nodes for similar nodes
- Combines FTS5 + trust + recency + embedding scores

**Rust TDG:** Hybrid search has the weights but the embedding component is a stub:
```rust
fn get_query_embedding_stub(&self, _conn: &Connection) -> Option<Vec<f32>> {
    None  // <-- STUB: never returns embeddings
}
```

**Impact:** The `embedding_weight: 0.20` in `RetrievalWeights` is dead weight — it never contributes to the score.

**What needs to be ported:**
- Wire `get_query_embedding_stub()` to the actual embedding engine
- Implement query embedding generation
- Add vec_nodes search to `build_embedding_map()`

**File to modify:** `src/plugins/hybrid_retriever.rs`

### 4. Embedding Backfill System Missing

**Python TDG:** Has `migrations/backfill_memory.py` that:
- Backfills FTS5 index for existing nodes
- Generates embeddings for nodes without vectors
- Syncs embeddings to vec_nodes table
- Is idempotent (safe to run multiple times)

**Rust TDG:** No equivalent system exists.

**What needs to be ported:**
- `backfill_fts5()` method
- `backfill_vec()` method
- `backfill_parent_ids()` method
- Integration with SelfManager's Janitor

**Files to create:** `src/maintenance/backfill.rs` or extend `src/maintenance/janitor.rs`

---

## Minor Gaps

### 5. Embedding Configuration

**Python TDG:** Rich config in `config/embeddings.json`:
- Strategy selection (onnx/gguf)
- Model paths, dimensions, thread count
- Lean mode settings
- Batch size, fallback options

**Rust TDG:** Basic config in `src/mind/embedding.rs`:
- Only ONNX config
- No GGUF config
- No lean mode integration

### 6. Lean Mode Integration

**Python TDG:** `TDG_LEAN=true` disables embeddings, HRR, reduces cron interval.

**Rust TDG:** No lean mode concept exists.

---

## Implementation Priority

| Priority | Gap | Effort | Impact |
|----------|-----|--------|--------|
| **P0** | Wire embedding engine to hybrid search | 1 day | Restores 20% of search quality |
| **P0** | Add sqlite-vec for ANN search | 2 days | 10x faster vector search at scale |
| **P1** | Add GGUF backend (EmbeddingGemma) | 2 days | Better embeddings (768-dim vs 384-dim)
| **P1** | Port backfill system | 1 day | Enables index synchronization
| **P2** | Add lean mode | 0.5 day | Resource-constrained environments
| **P2** | Add embedding config | 0.5 day | Runtime configuration

**Total: ~7 days**

---

## What's Already Working

| Feature | Python | Rust | Status |
|---------|--------|------|--------|
| **SelfManager** | ✅ | ✅ | ✅ Fully ported |
| **HealthMonitor** | ✅ | ✅ | ✅ Fully ported |
| **Janitor** | ✅ | ✅ | ✅ Fully ported |
| **Enricher** | ✅ | ✅ | ✅ Fully ported |
| **Archiver** | ✅ | ✅ | ✅ Fully ported |
| **MCP tools** | 27 | 27 | ✅ Fully ported |
| **FTS5 search** | ✅ | ✅ | ✅ Working |
| **Brute-force cosine** | ✅ | ✅ | ✅ Working |
| **Trust/recency scoring** | ✅ | ✅ | ✅ Working |
| **Type boosting** | ✅ | ✅ | ✅ Working |
| **Stop words** | ✅ | ✅ | ✅ Ported |
| **Schema versioning** | ✅ | ✅ | ✅ Added |
| **Mutation log** | ✅ | ✅ | ✅ Added |
| **Lease management** | ✅ | ✅ | ✅ Added |

---

## Recommendation

**Immediate action needed:** The hybrid search embedding component is a stub. This means 20% of search quality is disabled. Wire the embedding engine to the hybrid retriever as the first fix.

**Then:** Add sqlite-vec for ANN search to enable scalable vector search.

**Finally:** Add GGUF backend for better embeddings.

This is not a cosmetic gap — it's a functional degradation that affects search quality for every query.
