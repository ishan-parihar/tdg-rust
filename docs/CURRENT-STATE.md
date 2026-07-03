# tdg-rust Current State — July 2026

**Replaces:** `AUDIT_REPORT.md` (July 2026, archived to `docs/archive/AUDIT_REPORT_2026-07.md`) and `upgrade-plan.md` (commit d41c5ad, archived to `docs/archive/upgrade-plan_2026-06-30.md`).

**Purpose:** Accurate description of the v0.5.0 codebase. New contributors should read this instead of the archived audits, which describe bugs that are largely fixed.

---

## Build & Test Status

- `cargo check` — passes, 1 warning (unused fields in `mcp/health.rs::HealthCheckRecord`)
- `cargo check --features onnx` — passes (requires ONNX Runtime 1.20.1 for full build)
- `cargo test` — 491 test functions; suite compiles and runs
- Baseline established 2026-07-03

---

## What's Working

### Graph Engine
- SQLite WAL backend with connection pooling (8 conns default)
- FTS5 external-content search with porter + unicode61 tokenizer
- Hybrid retrieval (FTS5 + cosine similarity on embeddings)
- BFS/DFS pathfinding via petgraph in-memory projection
- Temporal queries (valid_from / valid_to soft-delete)
- Event-sourced temporal reconstruction (JSONL journal via `events` table)
- 12 indexes on nodes/edges/events + 3 on mutation_log

### Data Model
- 21 node types (v1: 13, v4.0: 5, v4.1: 3 "holonic" labels)
- 35 edge types (v1: 24, v4.0: 11)
- 4 dual-pole drives (eros, agape, agency, communion) — each `{positive_pole, negative_pole, availability, blind_spot}`
- 8 developmental stages (Survival → Harvest) with evidence thresholds and age gates
- 7 telos levels (T0 root mission → T6 transcendent)
- 10 catalyst types mapped to node blueprints
- 4 quadrants (UL/UR/LL/LR)

### Drive Propagation
- 3-phase pipeline: emission → stabilization → aggregation
- Quadrant modulators per-drive
- Variance floor (child variance ≥ intrinsic·0.3)
- Max influence per parent = 0.6
- Intrinsic blend ratio: 70% intrinsic + 30% incoming
- Drive values clamped to [-10, 10]
- Diagnoses: Integrated, Addiction, Allergy, BlindSpot, TensionPair
- Shannon entropy computation with health thresholds

### Mind Pipeline
- Consolidation engine (on-demand deep synthesis)
- Reflect engine (clusters observations by shared MENTIONS entities → creates skill nodes; idempotent via SHA256 fingerprint)
- Terrain context (skill discovery from graph density)
- Diagnostic engine (drive distributions, addiction/allergy/blind-spot flags, quadrant imbalance)
- Feeling engine (first-person emotional statements from drive averages)
- Pulse engine (structural-gap detection per node type)
- Embedding (ONNX EmbeddingGemma-300M 768-dim Q4/Q8, fallback MiniLM 384-dim)
- Non-destructive embedding migration (mixed dimensions supported via `dimension` column)

### MCP Server
- 36 tools via rmcp SDK (NOTE: `mcp/mod.rs` doc-comment still says "17 tools" — stale, will be fixed in Phase 0)
- Transports: stdio (default port 3000) + HTTP/SSE (other ports)
- Lean mode (skip expensive operations)
- Trust store with SQLite persistence
- Health monitor with circuit breakers

### Maintenance & Scheduling
- Background scheduler (spawned from `main.rs:518-575`):
  - SelfManager every 6h (configurable via `TDG_MAINTENANCE_INTERVAL_SECS`)
  - Internal health check every 5min (`TDG_HEALTH_CHECK_INTERVAL_SECS`)
- SelfManager cycle: HealthMonitor → Janitor → Enricher → Archiver → HealthMonitor + `telearchy.advance_stage` + `digestion.promote_hypothesis_to_capability`
- Janitor: FTS backfill, orphan detection, drive-state cleanup
- Enricher: embeddings, drives, stages, parents (reachable via `tdg_enrich` tool and `tdg_maintenance action=enrich`)
- Archiver: stale node archival
- Monitor: graph health metrics

### Safety
- Circuit breaker (Closed/Open/HalfOpen) — threshold=3, cooldown=30s
- PreWriteSnapshot for transaction rollback
- WriteGuard file lock per DB (5s timeout)
- Node/edge size limits (100K nodes, 500K edges)
- Type-safe error handling (`TdgResult<T>`)
- Path validation for export/import (blocks `/etc/`, `/var/`, etc.)
- Text length limits (50K chars), node ID length (256), bulk create limit (500)

### Provenance
- Every node has `source: String` field
- Every mutation log row has `agent_id`
- Every event has `agent_id`
- `mutation_log` table provides structured time-travel audit trail (Phase-5 migration)
- `record_mutation` captures old_value/new_value for time-travel

---

## What's Fixed (vs. archived audits)

The July 2026 `AUDIT_REPORT.md` described 5 "critical problems." All are verified fixed in v0.5.0:

| # | Was claimed broken | Current state |
|---|---|---|
| 1 | Adapter explicitly disables digestion (`trigger_digestion: False`) | **FIXED** — `plugins/tdg/__init__.py:326, 379, 407` all set `trigger_digestion: True` |
| 2 | Quadrant data stored in wrong column | **FIXED** — `tdg_observe` writes to BOTH `quadrants_json["primary"]` AND `properties_json["quadrant"]`; `tdg_mind_state` reads `quadrants_json` first with fallback |
| 3 | `enrich` action missing from `tdg_maintenance` | **FIXED** — `mcp/tools.rs:1825-1833` handles `"enrich"` and `"align_data"`; standalone `tdg_enrich` tool exists (line 1960) |
| 4 | Enricher never reachable from MCP | **FIXED** — see #3 |
| 5 | Embeddings never created | **PARTIALLY FIXED** — `add_node`/`update_node` inline-embed when `onnx` feature enabled; Enricher + Janitor also backfill. Requires `--features onnx` at compile time and `libonnxruntime.so.1` at runtime. |

The June 2026 `upgrade-plan.md` (commit d41c5ad) described 10 priority findings. Status:

| Priority | Finding | Current state |
|---|---|---|
| P0 | Test suite doesn't compile (`mcp/tests.rs` imports private params) | **FIXED** — `mcp/tests.rs` exists (758 LOC), `params.rs` is `pub mod` |
| P0 | FTS5 schema structurally wrong (`node_id` vs `id`) | **FIXED** — `db/schema.rs:231-259` FTS_SQL uses `id`; Phase-7 migration drops and rebuilds legacy FTS |
| P0 | Hybrid retriever queries nonexistent `embedding` column | **FIXED** — schema has `vector` column |
| P0 | Entity extraction reports but doesn't wire | **FIXED** — `tdg_observe` calls `upsert_entity_and_connect` for extracted entities |
| P1 | Entity alias APIs use legacy `properties` column | **FIXED** — schema uses `properties_json` everywhere |
| P1 | Embedding backfills store wrong dimension metadata | **FIXED** — `upsert_embedding(conn, node_id, vector, model, dimension)` is centralized; all callers pass dimension |
| P1 | Maintenance MCP contract fragmented (action vs phase) | **FIXED** — `tdg_maintenance` reads `action` first, falls back to `phase` with deprecation warning |
| P1 | Stage coverage treats valid T0 as missing | Needs re-verification |
| P2 | Timestamp format inconsistency | **MOSTLY FIXED** — `events.rs:75-76` uses `now_iso()`; triggers use ISO `strftime` |
| P2 | `mcp/tools.rs` is a god module (2,650 LOC) | **WORSE** — now 3,464 LOC. Phase 0 of the refactor plan addresses this. |

---

## Known Issues (Open)

These are the real open issues, not the stale ones from the archived audits:

### HIGH priority (Phase 0 of refactor)

1. **`src/mcp/tools.rs` is 3,464 LOC** — god module, hard to maintain, bugs recur after partial changes. Split by domain.

2. **Dead diagnostic engine histories** — `src/mind/injector.rs:118` calls `diag_engine.analyze(conn, &[], &[])` with empty arrays for `drive_history` and `quadrant_history`. The persistence-warning, quadrant-repetition, and stuck-pattern features are dead code in production.

3. **Stale doc-comments** — `src/mcp/mod.rs:11, 28` claims 17 MCP tools; there are actually 36.

### MEDIUM priority

4. **Dual `DualPoleDrive` structs** — `src/models.rs:188` (2 fields, unused) vs `src/flow.rs:111` (4 fields, canonical). The 2-field struct is dead code.

5. **`DriveVector` claims "16 drive dimensions"** (`src/models.rs:194`) but only 4 are realized. Aspirational comment from the Python prototype.

6. **MindStateManager claims "dual persistence (JSON + SQLite WAL)"** (`src/state.rs:90`) — only JSON is implemented; WAL is "future: eventsourcing".

7. **`agents_path` only uses `parent_ids[0]`** (`src/db/crud.rs:1509`) — multi-parent holons lose path information for parents 2..N.

8. **`record_mutation` is best-effort** (`src/db/crud.rs:98-117`) — failures logged via `tracing::warn!` but never propagated. Under heavy load, audit trails can have gaps.

### LOW priority

9. **Hardcoded agent name "Sisyphus"** (`src/mind/state.rs:49`) — not parameterised via config.

10. **Unused `HealthCheckRecord` fields** — `service`, `error_message`, `metadata`, `timestamp` are never read (the `cargo check` warning).

---

## Operational Reality vs. README

| README claim | Reality |
|---|---|
| "17 tools" (mcp/mod.rs doc) | Actually 36 tools |
| "511+ tests" | 491 test functions (close enough) |
| "Embedding pipeline" | Conditional on `--features onnx` at compile time AND `libonnxruntime.so.1` at runtime |
| "Hybrid FTS5 + embedding + graph search" | FTS5 works; embedding cosine similarity wired but only contributes when embeddings exist; graph expansion (BFS) is in `tdg_get_related` but not in `tdg_search` |
| "Telearchy engine for evidence collection and reporting" | True — fully implemented and wired into `tdg_audit` + SelfManager |
| "Digestion engine for processing raw observations" | True — creates hypotheses from 3+ observations sharing a source |
| "Two-axis stage-gated telos hierarchy" | True — Stage (8 levels) × TelosLevel (T0-T6) with evidence + age gates |
| "Background maintenance scheduler" | True — 6h SelfManager + 5min health check, env-configurable |
| "Circuit breaker for write operations" | True — global `CircuitBreaker` gates all CRUD writes |

---

## The Hermés Adapter (`plugins/tdg/__init__.py`)

Python `MemoryProvider` plugin that wraps `tdg-rust serve` as a subprocess. Exposes 3 LLM-facing tools (`tdg_memory_search`, `tdg_memory_record`, `tdg_memory_status`).

**Known anti-patterns:**
1. One subprocess per call — no stdio session reuse; every tool call re-spawns the binary. ~3 process startups per session.
2. `sync_turn` truncates `user_message[:200] + assistant_response[:300]` — information loss.
3. Heuristic skip logic — skips turns where `user_len < 20 or asst_len < 30`. Arbitrary thresholds.
4. `on_memory_write` mirrors writes as observations with `trigger_digestion: True` — can flood graph with low-signal observations.
5. No retry/backoff — single 30s MCP timeout fails permanently.
6. No streaming — captures all stdout then parses.

---

## What's Missing (Ontological Gap)

tdg-rust implements the *vocabulary* of holonic science but not the *operatorial core*. The full gap analysis is in `docs/HOLONIC-SCIENCE-AUDIT-AND-REFACTOR-PLAN.md` (in the project download directory — to be committed to the repo in a later phase). Summary:

| Primitive | Status |
|---|---|
| Lesser cycle (M·P·C·E) — the trusted anchor | ABSENT |
| Greater cycle (S·T·G·Ch) | ABSENT |
| Contact boundary | ABSENT |
| Attractor field A(H) = ⟨A_M, A_P, A_G, Γ⟩ | ABSENT |
| G_z (integrative efficiency) | ABSENT |
| P_z (transcendental tension) | ABSENT |
| Resonance R(H1, H2) | ABSENT (only embedding cosine sim) |
| Status ladder (ai-draft → canonical-hypothesis → canonical → superseded) | MISMATCHED (ad-hoc lifecycle_state strings) |
| Type class (e.g. strong-donor-sto) | ABSENT |
| 5-Gate Validation | ABSENT |
| Witnesses vs Sources epistemic distinction | ABSENT |
| Scale codes (S00–S80) | ABSENT |
| Tetra-Axes (4-axis coordinate system) | ABSENT (only single-quadrant label) |
| 8-role load vector (M·P·C·E·S·T·G·Ch) | ABSENT (only 4-drive vector) |
| 22 named archetypes | ABSENT |
| ContextPack (single-call intra+inter+extra) | PARTIAL (`tdg_context` exists but unstructured) |
| Phase-transition / thermodynamic model | PARTIAL (`compute_graph_entropy` exists, passive only) |
| Provenance on every semantic write | EMBODIED |
| Stage codes (8 stages) | EMBODIED |

---

## Next Steps

The refactor plan (`HOLONIC-SCIENCE-AUDIT-AND-REFACTOR-PLAN.md`) and computational design (`TDG-COMPUTATIONAL-DESIGN.md`) specify the path forward in 6 phases:

- **Phase 0 (Hygiene)** — fix the open issues above. No new features.
- **Phase 1 (Holon + Status + Scale)** — scaffolding for holonic computation.
- **Phase 2 (Lesser Cycle)** — the trusted anchor, event-driven metabolism.
- **Phase 3 (Attractor + Health + Resonance)** — operational object for health.
- **Phase 4 (Greater Cycle + Phase Transitions)** — vertical ascent operator.
- **Phase 5 (ContextPack + 5-Gate Validation)** — agent API redesign.
- **Phase 6 (Type System + 22 Archetypes)** — typological classifier.

Each phase is independently shippable, feature-flagged, and backward-compatible.

---

*Last updated 2026-07-03. Maintain this document as the codebase evolves; archive when superseded.*
