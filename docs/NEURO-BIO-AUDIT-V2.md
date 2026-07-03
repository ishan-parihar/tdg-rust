# Neuro-Psycho-Biological Audit + Ponytail/Gap Plan

**Audit date:** 2026-07-04
**TDG-rust commit:** `5c09ca1` (post-Phase 20, neural-plasticity complete)
**Scope:** Full architecture audit against neuro-psycho-biological brain factors
**Tests:** 504 pass (430 lib + 8 integration + 66 E2E), zero warnings

---

## Executive Summary

The TDG now has all six brain-like capabilities (Hebbian learning, synaptogenesis, replay, forgetting, drive adaptation, fragmentation cleanup). But a deep audit against how a real brain works reveals **9 critical functional gaps** and **36 bugs** (4 P0 crash, 5 P1 data corruption, 13 P2 logic errors).

The system is structurally sound but operationally fragile. The most urgent issues are concurrency bugs (worker race conditions, non-transactional multi-row updates) that can corrupt state under load, and missing inhibitory/reward mechanisms that prevent the system from self-regulating.

---

## Part 1: Neuro-Psycho-Biological Gap Analysis

### A. Neurotransmitter System — 4 Critical Gaps

| Brain function | Neurotransmitter | TDG status | Gap |
|---|---|---|---|
| **Inhibitory drive** | GABA | ABSENT — `negative_pole` subtracts from net but doesn't SUPPRESS propagation | **CRITICAL** — nothing prevents runaway excitation |
| **Reward prediction error** | Dopamine RPE | ABSENT — no comparison of expected vs received catalyst | **CRITICAL** — LTP is ungated (strengthens unconditionally) |
| **Global arousal** | Noradrenaline | ABSENT — fixed thresholds, no global gain modulation | HIGH — can't shift between "focused" and "rest" modes |
| **Mood baseline** | Serotonin | ABSENT — no slow tonic baseline | MEDIUM |

### B. Neural Circuit Dynamics — 3 Critical Gaps

| Brain function | TDG status | Gap |
|---|---|---|
| **Global oscillatory state** | ABSENT — each holon ticks independently | **CRITICAL** — no brain waves (gamma/beta/alpha/theta/delta) |
| **Phase synchronization** | ABSENT — catalyst injection is one-way push, no phase-locking | HIGH — holons can't synchronize their lesser cycles |
| **Frequency modulation** | ABSENT — graph_mind injects catalyst but doesn't change tick rate | HIGH — can't shift metabolism speed globally |

### C. Hippocampus-Neocortex System — 3 Critical Gaps

| Brain function | TDG status | Gap |
|---|---|---|
| **Episodic vs semantic distinction** | ABSENT — single uniform `nodes` table | **CRITICAL** — no way to distinguish fresh vs consolidated memories |
| **Transfer with source weakening** | PARTIAL — reflect_engine creates skills but doesn't weaken source observations | HIGH — graph monotonically grows, never transfers |
| **Consolidation marker** | ABSENT — no "consolidated" tag on nodes | HIGH — can't prioritize unconsolidated memories |

### D. Amygdala System (Salience) — 2 Critical Gaps

| Brain function | TDG status | Gap |
|---|---|---|
| **`salience_tag` column** | DEAD — added in Phase 16, never read or written in production | **CRITICAL** — column exists but is functionally inert |
| **High P_z → preferential replay** | ABSENT — P_z only aggregates to mean, doesn't tag individual nodes | **CRITICAL** — emotionally significant memories aren't prioritized |

### E. Prefrontal Cortex (Executive) — 2 Critical Gaps

| Brain function | TDG status | Gap |
|---|---|---|
| **Working-memory-gated retrieval** | ABSENT — MindStateManager is display-only, doesn't bias retrieval | **CRITICAL** — working memory doesn't influence what the agent recalls |
| **Forward planning** | ABSENT — system is purely reactive | **CRITICAL** — no simulation of future states |

---

## Part 2: Ponytail Audit (Redundancies)

### Delete (dead code)

| # | What | Location | Lines saved |
|---|---|---|---|
| D1 | `salience_tag` column (dead — implement or delete) | `db/schema.rs:232` | 3 |
| D2 | `MindState.active_plan` (never set by any code path) | `state.rs:31` | 2 |
| D3 | `MindState.metrics.last_diagnostic` (never written) | `state.rs:84` | 2 |
| D4 | `MindState.metrics.context_utilization` (never written) | `state.rs:82` | 2 |
| D5 | `MindState.trust_score` (set but never influences retrieval/metabolism) | `state.rs:35` | 5 |
| D6 | `IntrinsicSig` struct (duplicates `DualPoleDrive` shape) | `flow.rs:240-245` | 6 |
| D7 | Duplicate table definitions (SCHEMA_SQL + MIGRATE constants) | `db/schema.rs` | ~40 |

### Shrink (same logic, fewer lines)

| # | What | Location | Fix |
|---|---|---|---|
| S1 | 5 near-identical `count_by_type` queries | terrain, consolidation, events, tools, scripts | Extract to `count_active_by_type(conn)` (exists in terrain.rs) |
| S2 | 4 different "active node count" queries with different filters | feeling, tools, scripts | Standardize on `lifecycle_state='active' AND valid_to IS NULL` |
| S3 | 6+ `COUNT(*) FROM edges WHERE edge_type = ?` queries | graph_mind, monitor, scripts, tools, orchestrator | Use existing `crud.rs:1058` helper |
| S4 | `process_resonance_cluster` + `process_cluster` duplicate skill-creation | reflect_engine.rs | Extract `_create_skill_node(cluster, method)` |
| S5 | `check_upward_cascade` + `process_digestion_cycle` duplicate hypothesis-creation | digestion.rs | Consolidate to one method with `by` parameter |
| S6 | `intrinsic_signatures()` 192-line hardcoded table | flow.rs:248-440 | Move to config YAML |
| S7 | Unicode box-drawing lean mode banner (3 lines) | injector.rs:27-30 | One-line "LEAN MODE" |
| S8 | `detect_energy_level` hardcoded thresholds (0/5/20) | feeling.rs:184-192 | Config-driven |

### Yagni (abstraction with one implementation)

| # | What | Location |
|---|---|---|
| Y1 | `MindState.trust_score` — complex trust system, only used for display | state.rs:35 |
| Y2 | `ConsolidationReport.constraint_health.active` — misleading name, counts BLOCKS edges | consolidation_engine.rs:270 |

### Native (should use platform/standard)

| # | What | Location | Fix |
|---|---|---|---|
| N1 | `mcp/tools.rs:277` uses `!= 'archived'` while siblings use `= 'active'` | tools.rs:277 | Standardize to `= 'active'` |
| N2 | `MAX_DRIVE_VALUE=10` vs lesser_cycle `[0,1]` — inconsistent scales | flow.rs:46 vs lesser_cycle.rs:94 | Standardize to [0,1] |
| N3 | `archiver.rs:67` hard-deletes events (destroys audit trail) | archiver.rs:67 | Soft-delete instead |

**Net removable:** ~60 lines dead code + ~100 lines consolidation from shrink + 1 struct + 4 MindState fields

---

## Part 3: Bug Hunt (36 findings)

### P0 — Crash bugs (4)

| # | File:line | Bug | Fix |
|---|---|---|---|
| G10 | `audit.rs:118` | `unwrap()` on `get_mut("chronic")` — panics if key missing | Use `if let Some(arr) = ...` |
| G11 | `state.rs:131,143` | `.expect("mutex poisoned")` — one thread panic kills all future state access | Use `lock().unwrap_or_else(\|e\| e.into_inner())` to recover |
| G12 | `helpers.rs:55` | `.expect("ConnGuard conn already taken")` — double-take panics server | Return `Err` instead |
| G13-14 | `server.rs:30`, `llm/*.rs` | `.expect()` on startup — server refuses to start in restricted environments | Return `Result` |

### P1 — Data corruption (5)

| # | File:line | Bug | Fix |
|---|---|---|---|
| G1 | `worker.rs:706-709` | Drive adaptation `UPDATE` bypasses circuit breaker + write guard | Route through `flow::store_drive_state` |
| G2 | `worker.rs:146-196` | Two workers can claim jobs for same holon simultaneously — last write wins | Add per-holon advisory lock in `claim_job` |
| G28 | `flow.rs:799-855` | `aggregate_upward` loop has no transaction — partial failure leaves graph half-propagated | Wrap in `conn.unchecked_transaction()` |
| G29 | `flow.rs:607-699` | `emit_downward` BFS has no transaction — same issue | Same fix |
| G31 | `injector.rs:319` | Non-atomic write to `tdg-mind-snapshot.json` — crash mid-write corrupts file | Use temp-file + rename (like MindStateManager) |

### P2 — Logic errors (13)

| # | File:line | Bug | Fix |
|---|---|---|---|
| G3 | `worker.rs:638` | Circular parent references cause infinite catalyst cascades | Add visited set in upward pressure loop |
| G4 | `flow.rs:738` | `unwrap_or(0)` silently disables Hebbian learning on query failure | Propagate `Err` |
| G5 | `flow.rs:744` | BLOCKS edges get *stronger* with co-activation (Hebbian formula makes negative rates positive) | Clamp to `max(base_rate, learned_rate)` for negative rates |
| G6 | `flow.rs:557` | Influence weight uses only `eros`, ignores other 3 drives | Use net drive magnitude |
| G8 | `graph_mind.rs:144` | `LIKE '%dormant%'` brittle string matching on JSON | Parse JSON, check `phase == "Dormant"` |
| G15 | `lesser_cycle.rs:388` | Potentiator feedback is 100× attenuated — effectively dead | Increase from 0.01 to 0.1 |
| G16 | `lesser_cycle.rs:409` | Experience monotonically grows across cycles (70% retained forever, no decay) | Add per-cycle decay: `experience_accumulated *= 0.95` |
| G18 | `greater_cycle.rs:376` | `dissolution_ratio` always saturates to 1.0 in early cycles | Use `(shift / 0.3).min(1.0)` instead of `(shift / (old + 0.01))` |
| G19 | `health.rs:117` | G_z collapses to 0 for dormant holons (catalyst=0 → omega_a blows up → a_z→0) | Use `max(c, 0.1)` floor instead of `EPSILON` |
| G20 | `health.rs:125` | Same issue for C_z (experience=0 → c_z→0) | Same fix |
| G21 | `reflect_engine.rs:160` | Observations with MENTIONS edges never archived — stale observation path is dead for MCP-created nodes | Also archive by age, not just by edge absence |
| G25 | `archiver.rs:67` | Hard-deletes events older than 90 days — destroys audit trail | Soft-delete (set `archived_at` column) |
| G36 | `attractor.rs:312` | `edge_count` could be negative → magnitude negative | Add `.max(0.0)` |

---

## Part 4: The Refactor Plan

### Phase 22: Stop-the-Bleeding (bug fixes)

**Priority:** CRITICAL — these can crash the server or corrupt state.

1. Fix G10: `audit.rs:118` — replace `unwrap()` with `if let Some`
2. Fix G11: `state.rs:131,143` — recover from poisoned mutex
3. Fix G12: `helpers.rs:55` — return `Err` instead of panicking
4. Fix G1: `worker.rs:706` — route drive adaptation through `store_drive_state`
5. Fix G2: `worker.rs:146` — add per-holon lock in `claim_job` (skip if another job for same holon is in-progress)
6. Fix G28+G29: wrap `aggregate_upward` and `emit_downward` in transactions
7. Fix G31: use atomic write (temp + rename) in `injector.rs:319`
8. Fix G5: clamp Hebbian learning for negative-rate edges
9. Fix G19+G20: floor catalyst/experience at 0.1 in G_z/C_z computation
10. Fix G16: add per-cycle experience decay (`*= 0.95`)
11. Fix G15: increase potentiometer feedback from 0.01 to 0.1
12. Fix G25: soft-delete events instead of hard-delete

### Phase 23: Wire Up Salience + Episodic/Semantic

**Priority:** HIGH — connects dead infrastructure to live metabolism.

1. Implement `salience_tag`:
   - In `worker.rs::execute_recompute_health`: if `P_z > 50`, set `salience_tag = 'high_salience'`
   - In `worker.rs::execute_lesser_tick`: if `experience_accumulated > 2.0`, set `salience_tag = 'consolidation_target'`
2. Add `memory_stage TEXT DEFAULT 'episodic'` column to nodes
3. In `reflect_engine.rs::run`: after creating a skill, set source observations to `memory_stage = 'transferred'`
4. In the replay pass (`main.rs`): prioritize `salience_tag = 'high_salience'` and `memory_stage = 'episodic'` nodes
5. In the forgetting pass: only forget `memory_stage = 'episodic'` nodes with low confidence; never forget `memory_stage = 'semantic'`

### Phase 24: Inhibitory Drive + Reward Signal

**Priority:** HIGH — enables self-regulation.

1. Add `inhibition` as a 5th drive in `DualPoleDrive`:
   - `inhibition.positive_pole` = active suppression strength
   - `inhibition.negative_pole` = disinhibition (removal of suppression)
2. In `flow.rs::emit_downward`: if `inhibition.positive_pole > threshold`, reduce flow rate to children (the holon is "suppressing" its subordinates)
3. Add `reward_signal` field to `LesserCycleState`:
   - Set when `helpful_count` increases (agent found the memory useful)
   - Gates LTP: `co_activation_count` only increments if `reward_signal > 0`
4. Add `expected_catalyst` field to `LesserCycleState`:
   - Computed from rolling average of past catalyst amounts
   - RPE = `received_catalyst - expected_catalyst`
   - If RPE > 0 (surprise reward): strengthen LTP, set `salience_tag = 'high_salience'`
   - If RPE < 0 (disappointment): weaken LTP, set `salience_tag = 'low_salience'`

### Phase 25: Global Oscillatory State

**Priority:** MEDIUM — enables brain-wave-like global modulation.

1. Add `graph_state` table: `(key TEXT PK, value TEXT)` with key `wave_band`
2. Values: `delta` (deep rest, 1-4 Hz equivalent), `theta` (memory consolidation), `alpha` (relaxed), `beta` (active thinking), `gamma` (focused attention)
3. In `graph_mind.rs::run_integration`: set `wave_band` based on graph state:
   - High mean P_z + low orphans → `gamma` (focused, high-attention)
   - Low mean P_z + high depolarization → `delta` (rest, consolidation mode)
   - Moderate → `beta` (normal operation)
4. In `lesser_cycle.rs::tick`: scale `CycleThresholds` by wave_band:
   - `gamma`: `ingest_threshold *= 0.5` (more sensitive, fires easily)
   - `delta`: `ingest_threshold *= 2.0` (less sensitive, only strong perturbations fire)
5. In the replay schedule: only run when `wave_band = 'theta'` or `wave_band = 'delta'`

### Phase 26: Working Memory → Retrieval + Ponytail Cleanup

**Priority:** MEDIUM — connects executive function to retrieval.

1. In `db/crud.rs::search`: add optional `working_memory_bias: Option<Vec<String>>` parameter
   - If provided, boost relevance of nodes whose name/description contains any WM key
   - Boost factor: `1.2 × (1 + key_match_count)`
2. In `mcp/tools.rs::tdg_search`: read `MindStateManager.working_memory` and pass as bias
3. Execute ponytail D1-D7 (delete dead code)
4. Execute ponytail S1-S8 (shrink duplicate queries)
5. Fix remaining P2/P3 bugs from the bug hunt

---

## Summary: What to Delete, What to Expand

### DELETE (dead code / redundancy)
- `salience_tag` column (if not implementing Phase 23) OR implement it (Phase 23)
- `MindState.active_plan`, `last_diagnostic`, `context_utilization`, `trust_score` (4 unused fields)
- `IntrinsicSig` struct (duplicate of `DualPoleDrive`)
- Duplicate table definitions in schema (SCHEMA_SQL + MIGRATE constants)
- 5 duplicate `count_by_type` queries → use existing helper
- 6 duplicate edge-count queries → use existing helper

### SHRINK (same logic, fewer lines)
- `intrinsic_signatures()` 192-line table → config YAML
- `process_resonance_cluster` + `process_cluster` → one function
- `check_upward_cascade` + `process_digestion_cycle` → one function
- Unicode lean banner → one line
- Hardcoded energy thresholds → config

### EXPAND (implementation gaps to develop)
- **Inhibitory drive** (GABA analogue) — 5th drive that suppresses propagation
- **Reward prediction error** (dopamine analogue) — expected vs received catalyst
- **Global oscillatory state** (brain waves) — graph-level frequency modulation
- **Episodic vs semantic memory** — memory_stage column, transfer with source weakening
- **Salience tagging** — wire up the dead column to P_z and experience
- **Working-memory-gated retrieval** — WM biases search results
- **Forward planning** — simulate future graph states (long-term)

### EVOLVE (refinements for next iteration)
- Phase 24: reward-gated LTP (dopamine gates Hebbian strengthening)
- Phase 25: brain-wave-modulated metabolism (global frequency scaling)
- Phase 26: WM-gated retrieval (PFC → hippocampus projection)
- Future: counterfactual simulation (what-if reasoning)
- Future: goal decomposition (frontopolar cortex analogue)

---

*Audit completed 2026-07-04. Based on exhaustive source-code analysis of tdg-rust commit 5c09ca1. All findings cite exact file:line locations.*
