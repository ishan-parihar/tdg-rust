# TDG-Rust Infrastructure — Complete Fragmentation Report

**Date:** 2026-06-30
**Binary Version:** v0.4.0 (ONNX enabled)
**Database:** 59.7 MB, 1,274 nodes, 44,777 edges, 98,097 events
**Health Score:** 1.02 (post-FTS fix)

**Methodology:** 4-way parallel audit — direct analysis + 3 subagents covering DB/maintenance, mind/MCP tools, and flow/grammar/plugins layers. 63 raw findings deduplicated to **35 unique issues**.

---

## Executive Summary

The TDG-Rust codebase has **35 fragmentation/implementation gaps** across 7 layers. The core problem is not individual bugs — it's that **the end-to-end lifecycle pipeline is broken at every link**:

```
Observe → Extract → Create → Wire → Flow → Advance
   ✅        ❌        ✅       ✅      ❌      ❌
```

The observe and create steps work. The wiring step works. But entity extraction, flow propagation, and telearchy advancement never fire. This means the graph accumulates nodes but never processes them into meaningful structure.

Additionally, **3 CRITICAL data corruption bugs** silently destroy data on every enrichment cycle.

---

## CRITICAL Gaps (Data Corruption / Always-Wrong Results)

### C1. Mind State Reads Drives from Wrong Column
- **File:** `src/mcp/tools.rs:1367-1395`
- **Issue:** `tdg_mind_state` detail mode queries `properties_json` for drive data (`eros_positive_pole`, etc.), but the enricher writes drives to `drives_json`.
- **Impact:** Drive scores **always zero**. The enricher populates `drives_json` on 514+ nodes, but the reader never sees them.
- **Fix:** ~10 lines — change `properties_json` to `drives_json` and parse the correct structure.

### C2. Mind State Reads Quadrants from Wrong Column
- **File:** `src/mcp/tools.rs:1340-1360`
- **Issue:** `tdg_mind_state` default mode queries `properties_json` for `primary` key, but quadrant data lives in `quadrants_json`.
- **Impact:** Quadrant distribution **always `{UL:0, UR:0, LL:0, LR:0}`**.
- **Fix:** ~5 lines — change to read from `quadrants_json`.

### C3. Enricher Writes STRING "T2" to INTEGER Column (Active Data Corruption)
- **File:** `src/maintenance/enricher.rs:73-85, 247-249`
- **Issue:** `stage_by_type()` returns `&str` values like `"T0"`, `"T2"`. The `enrich_stages()` function writes these to `developmental_stage INTEGER`. SQLite silently coerces `"T2"` → `0`, destroying existing stage data and re-triggering enrichment in an infinite loop.
- **Impact:** Every enrichment cycle **overwrites all developmental stages to 0**, destroying the telearchy hierarchy.
- **Fix:** Change `stage_by_type()` to return `i32` values (`0, 2, 3, 4`).

### C4. ONNX Feature Gate Missing in Janitor + Enricher
- **Files:** `src/maintenance/janitor.rs:291-298`, `src/maintenance/enricher.rs:152-167`
- **Issue:** Both call `crate::mind::embedding::embed()` unconditionally. On non-ONNX builds, this calls the stub that always returns `Err`. The janitor/enricher burn CPU on error paths for every unembedded node on every cycle.
- **Impact:** Wasted cycles + misleading error logs on every maintenance run.
- **Fix:** Wrap in `#[cfg(feature = "onnx")]` with skip-logging else branch.

---

## HIGH Gaps (Broken Features / Silent Failures)

### H1. Event Query `after`/`before` Parameters Never Wired
- **File:** `src/mcp/tools.rs:878-921`
- **Issue:** `QueryEventsParams` declares `after`/`before` fields, but the SQL never uses them.
- **Fix:** ~8 lines — add `AND timestamp >= ?` / `AND timestamp <= ?` clauses.

### H2. Event Trigger Timestamps Incompatible with Code Timestamps
- **File:** `src/db/schema.rs:291-361` vs `src/db/crud.rs:805`
- **Issue:** Triggers use `datetime('now')` → `"2026-06-30 14:30:00"`. Code uses `now_iso()` → `"2026-06-30T14:30:00+00:00"`. Lexicographic comparison of these formats is incorrect.
- **Fix:** Standardize all timestamps to ISO 8601.

### H3. `pattern_synthesis` Queries Nonexistent Column
- **File:** `src/mcp/tools.rs:2728`
- **Issue:** SQL references `SELECT drives FROM nodes` — column is `drives_json`. Query silently returns empty.
- **Fix:** Change `drives` to `drives_json`.

### H4. FTS Not Auto-Synced on Node Changes
- **Files:** `src/maintenance/janitor.rs:56-84`
- **Issue:** No triggers keep FTS in sync. New nodes missed until next `self_manage`. Soft-deleted nodes bloat FTS index.
- **Fix:** Add FTS insert in `tdg_observe`/`tdg_create` + janitor cleanup for stale entries.

### H5. FTS External Content Bloat (Pre-Fix State)
- **File:** `src/db/schema.rs:210-227`
- **Issue:** Old `content='nodes'` FTS mode had broken sync. Janitor adds entries but never removes stale ones for soft-deleted nodes.
- **Status:** We fixed by switching to standalone FTS, but the janitor's FTS code still references the old pattern.

### H6. Plugin Layer: 3 of 4 Plugins Dead Code
- **Files:** `src/plugins/turn_capture.rs`, `entity_extractor.rs`, `preference_extractor.rs`
- **Issue:** Fully implemented but zero production callers. Not wired into MCP server or `main.rs`.
- **Impact:** Turn capture, NER, and preference extraction completely non-functional.

### H7. ConsolidationEngine Never Called
- **File:** `src/mind/consolidation_engine.rs`
- **Issue:** Declared and exported in `lib.rs` but has no MCP tool, no cron, no trigger.
- **Impact:** No memory consolidation, pattern detection, or constraint health analysis.

### H8. ReflectEngine Never Runs
- **File:** `src/mind/reflect_engine.rs`
- **Issue:** Called only from ConsolidationEngine (which is dead code). Observation clustering and hypothesis creation never happens.

### H9. Entire Lifecycle Pipeline Broken
- **File:** Multiple
- **Issue:** The intended chain Observe→Extract→Create→Wire→Flow→Advance has 5 broken links:
  1. Extract: Entity extraction in turn_capture only adds MENTIONS edges
  2. Create: No hypothesis/constraint creation for patterns
  3. Flow: `emit_downward()` never called after auto-wiring
  4. Advance: Telearchy never checks after flow propagation
  5. Consolidation: Never runs
- **Impact:** Graph accumulates nodes but never processes them.

---

## MEDIUM Gaps (Design Debt / Disconnected Systems)

### M1. Enricher Uses Static Drive Values Per Node Type
- **File:** `src/maintenance/enricher.rs:20-65`
- **Issue:** All observations get identical drive signatures from hardcoded maps. Flow engine propagation never runs on these values.

### M2. Flow Engine Never Called After Node Creation
- **File:** `src/flow.rs`
- **Issue:** Full propagation pipeline implemented but never invoked from any MCP tool or maintenance cycle.

### M3. `tdg_reflect` Results Not Persisted
- **File:** `src/mcp/tools.rs:1830-2088`
- **Issue:** LLM synthesis returned as text string, never stored as graph node.

### M4. Three Unused Database Tables
- `mutation_log` (0 rows), `leases` (0 rows), `health_checks` (0 rows) — created but never written to.

### M5. Flow ↔ Telearchy Never Connected
- **File:** `src/flow.rs`, `src/telearchy.rs`
- **Issue:** After drive state changes, no telearchy stage check. After stage advancement, no flow re-emission.

### M6. Validation Layer Bypassed by Grammar Engine
- **File:** `src/grammar/node_grammar.rs`, `auto_wire.rs`
- **Issue:** Grammar creates nodes/edges without calling `validate_node_creation()` or `validate_edge_creation()`.

### M7. Edge Noise Formula Inverted in Health Score
- **File:** `src/mcp/tools.rs:828`
- **Issue:** `(1.0 - edge_noise) * 0.15` penalizes having MORE structured edges.

### M8. Context Generator Reads Python-Era JSON Files
- **File:** `src/mind/injector.rs:26-27`
- **Issue:** Reads `hermes-working-memory.json` from Python TDG plugin. If absent, all sections return defaults.

### M9. Intrinsic Signatures vs Node Contracts Dual Definition
- **Files:** `src/flow.rs`, `src/validation.rs`
- **Issue:** Two overlapping but non-identical node type registries with no shared source of truth.

### M10. `QUADRANT_MODULATORS` Defined but Never Used
- **File:** `src/flow.rs:37-84`
- **Issue:** Detailed per-quadrant drive multipliers exist but are never applied in any production code path.

### M11. Reflect Engine Minimum Thresholds Too High
- **File:** `src/mind/reflect_engine.rs:55-88`
- **Issue:** Requires 5 recent observations sharing 2+ entities. Silently skips for sparse graphs.

### M12. Revenue Pulse Uses Hardcoded Stale Dates
- **File:** `src/mind/sections.rs:22-56`
- **Issue:** `checkpoint_date: "2026-05-20"` hardcoded as default.

### M13. `compute_agent_path` Only Uses First Parent
- **File:** `src/db/crud.rs:1274-1294`
- **Issue:** Loses hierarchy for multi-parent nodes.

---

## LOW Gaps (Minor Debt)

### L1. Lean Mode Disables All MCP Tools
### L2. `#![allow(dead_code)]` Masks Unused Code
### L3. `tdg_bank` Returns Empty — No Banks
### L4. Dual Persistence (JSON + SQLite) for Mind State
### L5. `rate_node` Missing `valid_to IS NULL` Filter
### L6. `agent_id` Column Never Populated by Triggers
### L7. Parent Backfill Duplicated in Janitor + Enricher
### L8. `schema_meta.version` Never Bumped

---

## Priority Fix Order

| Priority | Gaps | Lines | Impact |
|----------|------|-------|--------|
| **P0** | C1 + C2 + C3 + C4 | ~25 | Fix data corruption (drives, quadrants, stages, ONNX gate) |
| **P1** | H1 + H2 + H3 | ~20 | Fix broken queries (events, pattern_synthesis, timestamps) |
| **P2** | H4 + H5 + M7 | ~15 | Fix FTS sync + health score formula |
| **P3** | H6 + H7 + H8 | ~100 | Wire dead plugins + consolidation + reflection |
| **P4** | H9 + M1-M6 + M9-M10 | ~200+ | Reconnect the lifecycle pipeline (design decision) |
| **P5** | L1-L8, M8, M11-M13 | ~50 | Minor cleanup |

---

## Quick-Win: P0 Patch (25 lines, fixes 4 critical bugs)

```rust
// 1. C1: Fix drives — tools.rs ~line 1367
// Change: SELECT properties_json FROM nodes WHERE ... properties_json NOT IN ('{}', '')
// To:     SELECT id, drives_json FROM nodes WHERE ... drives_json != '{}'
// Then parse drives_json structure: {"eros": {"positive_pole": 7.5, ...}}

// 2. C2: Fix quadrants — tools.rs ~line 1340
// Change: SELECT properties_json FROM nodes WHERE ... properties_json NOT IN ('{}', '')
// To:     SELECT quadrants_json FROM nodes WHERE ... quadrants_json != '{}'
// Then read primary key from quadrants_json

// 3. C3: Fix stages — enricher.rs stage_by_type()
// Change: "observation" → "T2"  (string)
// To:     "observation" → 2      (integer)
// Change all 6 entries from "T0"/"T2"/"T3"/"T4" to 0/2/3/4

// 4. C4: Fix ONNX gate — janitor.rs + enricher.rs
// Wrap embed() calls in #[cfg(feature = "onnx")] { ... } else { log skip }
```
