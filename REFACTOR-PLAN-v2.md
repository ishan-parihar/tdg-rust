# TDG-Rust Refactor Plan v2: Dead Code Removal

**Date:** 2026-06-28
**Source:** Re-audit after Phase 1-4 (3,050 lines removable)
**Goal:** Remove dead modules, unused dependencies, and partial dead code

---

## Phase 5: Delete Dead Modules (~2,400 lines)

### 5A: Delete ops.rs (~897 lines)
- File: `src/ops.rs`
- Remove: `pub mod ops;` from `src/lib.rs`
- Remove: Any references in `src/main.rs` (if any)
- Verify: grep for `ops::` or `TdgOps` across codebase

### 5B: Delete hrr_retriever.rs (~673 lines)
- File: `src/hrr_retriever.rs`
- Remove: `pub mod hrr_retriever;` from `src/lib.rs`
- Remove: Any references in `src/main.rs` or `src/mcp/tools.rs`
- Verify: grep for `HrrRetriever` across codebase

### 5C: Delete eventsourcing/mod.rs (~500 lines)
- File: `src/eventsourcing/mod.rs`
- Remove: `pub mod eventsourcing;` from `src/lib.rs`
- Remove: Any references in `src/audit.rs` (string literal only)
- Verify: grep for `EventJournal`, `ReplayEngine`, `SnapshotManager`

### 5D: Delete graph_algorithms.rs (~250 lines)
- File: `src/graph_algorithms.rs`
- Remove: `pub mod graph_algorithms;` from `src/lib.rs`
- Remove: Any references in `src/graph_projection.rs` (dead methods)
- Verify: grep for `graph_algorithms::` across codebase

### 5E: Delete test_utils.rs (~100 lines)
- File: `src/test_utils.rs`
- Remove: `pub mod test_utils;` from `src/lib.rs`
- Remove: Any references in test files
- Verify: grep for `test_utils::` across codebase

---

## Phase 6: Delete Unused Dependencies

### 6A: Remove from Cargo.toml
```
ahash = "0.8"
rustworkx-core = "0.17"
leiden-rs = "0.8"
```

### 6B: Verify no references remain
- grep for `ahash::` or `use ahash` across codebase
- grep for `rustworkx_core::` or `use rustworkx_core` across codebase
- grep for `leiden` across codebase

---

## Phase 7: Clean Up Partial Dead Code (~600 lines)

### 7A: Trim graph_projection.rs (~80 lines)
- File: `src/graph_projection.rs`
- Keep: `build()` method and struct fields
- Delete: `shortest_path`, `stats`, `betweenness_centrality`, `degree_centrality`, `is_connected`, `graph_density`, `leiden_communities`, `neighbors`, `to_d3_json`, `to_d3_string`, `to_dot`
- Update: `src/mcp/tools.rs` if it calls any deleted methods

### 7B: Delete visualization::html_export (~220 lines)
- File: `src/visualization.rs`
- Delete: `html_export` function and related HTML template code
- Keep: `d3_json` and `dot_export` (used by graph_projection)

### 7C: Delete circuit_breaker dead code (~130 lines)
- File: `src/circuit_breaker.rs`
- Delete: `TransactionSnapshot`, `PreWriteSnapshot` structs and impls
- Keep: `CircuitBreaker`, `CircuitState`, `global_circuit_breaker`

---

## Verification Protocol

After EACH phase:
```bash
cargo check
cargo test --lib
```

After ALL phases:
```bash
cargo check
cargo test
cargo clippy
```

---

## Execution Order

| Phase | Lines Cut | Risk |
|-------|-----------|------|
| 5: Delete dead modules | ~2,400 | Low — remove unused code |
| 6: Delete unused deps | ~3 deps | Low — remove unused deps |
| 7: Clean partial dead code | ~600 | Medium — trim existing files |

**Total: ~3,000 lines + 3 deps removed**

---

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Breaking tests | Run tests after each deletion |
| Missing references | Grep for all deleted symbols before removing |
| Dependency conflicts | Check Cargo.lock after removal |
| MCP tool breakage | Verify mcp/tools.rs after each change |
