# TDG-Rust Refactor Plan: Over-Engineering Cleanup

**Date:** 2026-06-28
**Source:** Ponytail audit (29 findings, ~1,510 lines + 5 deps)
**Goal:** Remove dead code, YAGNI abstractions, duplicated logic, unused dependencies

---

## Phase 1: Delete YAGNI Modules (Day 1) — ~880 lines

### 1A: Delete MetricsEngine (~330 lines)
- File: `src/mind/metrics.rs`
- Remove: `pub mod metrics;` from `src/mind/mod.rs`
- Remove: Any imports in `src/lib.rs`
- Remove: Any references in `src/ops.rs` or other modules

### 1B: Delete ProjectTracker (~210 lines)
- File: `src/mind/project_tracker.rs`
- Remove: `pub mod project_tracker;` from `src/mind/mod.rs`
- Remove: Any imports in `src/lib.rs`

### 1C: Delete FallbackProvider (~85 lines + tests)
- File: `src/llm/fallback.rs`
- Remove: `pub mod fallback;` from `src/llm/mod.rs`
- Remove: Any references in `src/lib.rs`

### 1D: Delete PreWriteSnapshot + TransactionSnapshot (~80 lines)
- File: `src/circuit_breaker.rs` (lines 170-299)
- Keep: CircuitBreaker, CircuitState, global_circuit_breaker
- Remove: PreWriteSnapshot, TransactionSnapshot structs and impls
- Update: Any references in `src/db/write_guard.rs` or `src/db/crud.rs`

### 1E: Delete TurnCaptureWriter (~100 lines)
- File: `src/plugins/turn_capture.rs` (lines 345-427)
- Keep: TurnCapture struct and sync `capture()` method
- Remove: TurnCaptureWriter, async batch writer

---

## Phase 2: Delete Unused Dependencies (Day 1) — ~110K crate size

### 2A: Remove from Cargo.toml
```
linfa = "0.8.1"
linfa-clustering = "0.8.1"
linfa-preprocessing = "0.8.1"
dirs = "5"
futures = "0.3"
```

### 2B: Remove from clustering.rs
- Delete entire file if linfa is removed
- Remove: `pub mod clustering;` from `src/lib.rs`
- Remove: Any references in `src/ops.rs`

### 2C: Remove #![allow(dead_code)]
- File: `src/lib.rs` (line 1)
- Remove the crate-level dead_code allowance
- Fix any remaining dead code warnings

---

## Phase 3: Shrink Duplicated Code (Day 2) — ~410 lines

### 3A: Extract shared STOP_WORDS
- Create: `src/util/stopwords.rs` with unified stop-word set
- Update: `src/plugins/entity_extractor.rs` to import from util
- Update: `src/plugins/hybrid_retriever.rs` to import from util
- Remove: Duplicate definitions (~200 lines saved)

### 3B: Extract shared QUADRANT_KEYWORDS
- Move to `src/util/quadrants.rs` or keep in one file
- Update: `src/plugins/turn_capture.rs` to import
- Update: `src/plugins/preference_extractor.rs` to import
- Remove: Duplicate definitions (~130 lines saved)

### 3C: Extract shared cosine_similarity
- Move to `src/util/math.rs` or `src/hrr.rs`
- Update: All 3 files to import from shared location
- Remove: 2 duplicate definitions (~30 lines saved)

### 3D: Extract infer_quadrant
- Move to shared location
- Update: Both files to import
- Remove: Duplicate logic (~50 lines saved)

### 3E: Extract detect_drive_persistence helper
- Move to `src/mind/util.rs` or keep in one file
- Update: Both diagnostic.rs and feeling.rs to import
- Remove: Duplicate logic (~30 lines saved)

---

## Phase 4: Clean Up Remaining YAGNI (Day 2) — ~200 lines

### 4A: Remove DiagnosticThresholds YAML loading
- File: `src/mind/diagnostic.rs` (lines 94-127)
- Inline defaults directly
- Remove: `serde_yaml` dependency if only used here

### 4B: Remove ReflectEngine::with_config
- File: `src/mind/reflect_engine.rs` (lines 9-57)
- Delete `ReflectConfig` struct and `with_config` method
- Keep: `new()` with defaults

### 4C: Remove ConsolidationEngine::with_lean
- File: `src/mind/consolidation_engine.rs` (lines 40-42)
- Merge lean parameter into `new()`

### 4D: Remove unused HRR methods
- File: `src/hrr.rs`
- Delete: `reason()`, `related()`, `entries()`, `snr_estimate()`, `random_key()`
- Keep: `bind()`, `unbind()`, `bundle()`, `probe()`

### 4E: Remove graph_algorithms::num_communities
- File: `src/graph_algorithms.rs` (lines 93-102)
- Delete the method

### 4F: Remove manual str conversions
- File: `src/mind/diagnostic.rs` — `Severity::as_str`, `DriveLabel::as_str`
- File: `src/mind/pulse.rs` — `PulseSeverity::as_str`
- Use `{:?}` or `Serialize` instead

### 4G: Remove MindStateManager::persist
- File: `src/mind/state.rs` (lines 150-155)
- Delete the method

---

## Phase 5: Shrink MCP Tools (Day 2) — ~120 lines

### 5A: Extract lean_guard to shared helper
- Create: `src/mcp/util.rs` with `fn lean_guard(pool) -> Result<bool>`
- Update: All 31 tools to use shared helper
- Remove: 30 duplicate lean_guard blocks (~90 lines saved)

### 5B: Merge tdg_prefetch into tdg_search
- Add `output_format` parameter to tdg_search
- Remove: tdg_prefetch tool
- Update: HybridRetriever to support format options

### 5C: Extract TrustStore + HealthMonitor boilerplate
- Consider: Single `AppState` struct with derive macros
- Remove: Duplicate Mutex + write-through patterns (~40 lines)

---

## Execution Order

| Phase | Day | Lines Cut | Risk |
|-------|-----|-----------|------|
| 1: Delete YAGNI modules | 1 | ~880 | Low — remove unused code |
| 2: Delete unused deps | 1 | ~110K crate | Low — remove unused deps |
| 3: Shrink duplicated code | 2 | ~410 | Medium — refactor shared code |
| 4: Clean remaining YAGNI | 2 | ~200 | Low — remove unused methods |
| 5: Shrink MCP tools | 2 | ~120 | Low — simplify tool patterns |

**Total: ~1,610 lines + 5 deps removed across 2 days**

---

## Verification

After each phase:
```bash
cargo check
cargo test
```

After all phases:
```bash
cargo check
cargo test
cargo clippy
```

---

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Breaking tests | Run tests after each deletion |
| Missing references | Grep for all deleted symbols before removing |
| Dependency conflicts | Check Cargo.lock after removal |
| Performance regression | Benchmark before/after (optional) |
