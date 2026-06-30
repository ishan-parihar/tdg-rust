# TDG-Rust Bug Fix Plan

**Date**: 2026-06-30
**Source**: TDG-RUST-FRAGMENTATION-REPORT.md (verified against codebase)

## Verified Bugs

### C1+C2: tdg_mind_state reads wrong columns (CRITICAL)
- **File**: `src/mcp/tools.rs:1358,1418`
- **Bug**: Quadrant distribution query reads `properties_json` — should read `quadrants_json`. Drive scores query reads `properties_json` — should read `drives_json`.
- **Fix**: Replace column names in SQL queries.

### C3: enricher writes string to INTEGER column (CRITICAL)
- **File**: `src/maintenance/enricher.rs:73-85`
- **Bug**: `stage_by_type()` returns `"T2"`, `"T3"`, etc. but `developmental_stage` is `INTEGER`.
- **Fix**: Return numeric values (0-4) instead of string labels.

### H1: event query ignores after/before (HIGH)
- **File**: `src/mcp/tools.rs:877-921`
- **Bug**: `QueryEventsParams` declares `after`/`before` fields but SQL never uses them.
- **Fix**: Add `AND timestamp >= ?` and `AND timestamp <= ?` clauses.

### H2: timestamp format mismatch (HIGH)
- **Files**: `src/db/schema.rs:288-361`, `src/db/crud.rs:64-66`
- **Bug**: Triggers use `datetime('now')` → `"2026-06-30 14:30:00"`. Code uses `now_iso()` → `"2026-06-30T14:30:00Z"`. String comparisons break.
- **Fix**: Standardize triggers to `strftime('%Y-%m-%dT%H:%M:%SZ', 'now')`.

### H3: drives column typo (HIGH)
- **File**: `src/mcp/tools.rs:2728`
- **Bug**: SQL references `drives` — column is `drives_json`.
- **Fix**: Replace `drives` with `drives_json`.

## Fix Priority Order

1. C1+C2 (data corruption — reads wrong data)
2. C3 (data corruption — writes wrong type)
3. H3 (query failure — column doesn't exist)
4. H1 (functional — filters ignored)
5. H2 (functional — timestamp comparison broken)

## Verification

After all fixes: `cargo build --release` must succeed. If tests exist, `cargo test`.
