# REFACTOR-PLAN-V3 — Final Over-Engineering Cleanup

## Context

After the 3-phase refactor (V1: YAGNI modules, V2: Dead code, V3: Dedup/shrink), the codebase is at 26,615 lines across 63 files with 28 dependencies. This plan targets the remaining ~1,584 lines of waste and 3 removable dependencies identified by the final ponytail audit.

## Pre-Execution State

- **Tests**: 353 pass, 0 fail
- **Clippy**: 1 warning (unnecessary parentheses)
- **Git**: Clean (no uncommitted changes)

## Phase 1: Delete Dead Modules (~1,107 lines)

### 1a. Delete `lifecycle.rs` (725 lines)
- **Why**: `SessionLifecycle`, `SessionState`, `LifecycleEvent` — zero callers outside own file. Speculative state machine.
- **Action**: Delete `src/mind/lifecycle.rs`, remove `pub mod lifecycle;` from `src/mind/mod.rs`
- **Verify**: `cargo check` passes, tests still pass

### 1b. Delete `hrr.rs` (261 lines)
- **Why**: `HrrMemoryBank`, `phase_encode`, `bind`, `unbind`, `bundle`, `normalize` — zero callers within crate. `hrr_retriever` was already removed.
- **Action**: Delete `src/hrr.rs`, remove `pub mod hrr;` from `src/lib.rs`, remove `rustfft` from `Cargo.toml`
- **Verify**: `cargo check` passes, tests still pass

### 1c. Delete `visualization.rs` (121 lines)
- **Why**: `d3_json::export`, `dot_export::export` — zero callers from MCP/scripts/main.
- **Action**: Delete `src/visualization.rs`, remove `pub mod visualization;` from `src/lib.rs`
- **Verify**: `cargo check` passes

## Phase 2: Remove Dead Dependencies (2 deps)

### 2a. Remove `leiden-rs` + `rustworkx-core`
- **Why**: Zero imports anywhere in src/.
- **Action**: Remove from `Cargo.toml`
- **Verify**: `cargo check` passes

## Phase 3: Delete Dead Functions (~471 lines)

### 3a. `knowledge.rs` dead functions (~313 lines)
- **Why**: `classify_catalyst`, `link_catalyst_to_structure`, `evaluate_integration_quality`, `process_catalyst_lifecycle`, `reverse_archival` — only called from each other + tests.
- **Action**: Delete functions, remove test references
- **Verify**: Tests still pass

### 3b. `serialize_embedding` + `deserialize_embedding` in `crud.rs` (14 lines)
- **Why**: Duplicate of `serialize_vector` (line 1105).
- **Action**: Delete functions, update test to use `serialize_vector`
- **Verify**: Tests still pass

### 3c. `hard_delete_node` in `crud.rs` (29 lines)
- **Why**: Test-only.
- **Action**: Delete function, remove test
- **Verify**: Tests still pass

### 3d. `export_audit_markdown` in `audit.rs` (~30 lines)
- **Why**: Test-only.
- **Action**: Delete function, remove test
- **Verify**: Tests still pass

### 3e. `get_intrinsic_signature` in `flow.rs` (~85 lines)
- **Why**: Test-only standalone function. `FlowDriveState::intrinsic()` covers same logic.
- **Action**: Delete function, remove test
- **Verify**: Tests still pass

## Phase 4: Fix Clippy Warning

### 4a. Fix unnecessary parentheses in `src/db/crud.rs`
- **Action**: `cargo fix --lib -p tdg-rust`
- **Verify**: Clippy clean

## Execution Strategy

Phases 1-3 execute in parallel via subagents (no overlap). Phase 4 runs after all phases complete.

## Expected Result

| Metric | Before | After | Delta |
|--------|--------|-------|-------|
| Source lines | 26,615 | ~25,031 | -1,584 |
| Dependencies | 28 | 25 | -3 |
| Tests | 353 | ~340 | -13 (dead test cleanup) |
| Clippy warnings | 1 | 0 | -1 |

## Verification

1. `cargo check` — clean compilation
2. `cargo test` — all remaining tests pass
3. `cargo clippy` — no warnings
