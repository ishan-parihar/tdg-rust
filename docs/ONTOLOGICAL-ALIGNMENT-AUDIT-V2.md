# Second-Pass Ontological-Alignment & Memory-Operational-Efficacy Audit

**Audit date:** 2026-07-03 (second pass)
**TDG-rust commit:** `d8ae992` (post-Phase 7-11 alignment)
**HoloOS commit:** `a6a9b0f83` (latest pull — includes 08.8.21, density naming fix, schema v6.0, coordinate integration tests)
**Previous audit score:** 5.7/10 → **Current score:** 7.7/10 → **This audit target:** identify remaining gaps

---

## Executive Summary

The first-pass alignment (Phases 7-11) closed the 3 critical gaps (coordinate system, semantics, mind pipeline integration) and raised the alignment score from 5.7 to 7.7. This second pass identifies what remains.

HoloOS has advanced further since the last pull, adding:
1. **Schema v6.0** — formalizes the coordinate object with octave/density/sub_density/collectivity/realm
2. **Density naming correction** — VIBGYOR is now octave-agnostic (not D1=Red, but D1=first density of ANY octave)
3. **R&D Utility Audit (08.8.21)** — identifies 5 R&D-capacity gaps (R1-R5)
4. **Coordinate integration tests** — formal test suite for V/C/R/N

**The finding:** TDG-rust is now **structurally aligned** with HoloOS for memory infrastructure purposes. The remaining gaps are R&D-utility features (transmutation cycles, sub-density content, witness commands) that are primarily relevant to HoloOS's knowledge-base R&D, not to tdg-rust's role as agent memory. However, 3 operational efficacy gaps remain that directly impact the agent's mind.

---

## Part 1: Ontological Alignment Status

### 1.1 Fields Now Aligned ✅

| HoloOS schema field | TDG implementation | Status |
|---|---|---|
| `synthesis_status` | `Node.synthesis_status` + `SynthesisStatus` enum | ✅ Full |
| `holon_id` | `Node.id` | ✅ Full |
| `holon_type` | `Node.node_type` | ✅ Full |
| `name` | `Node.name` | ✅ Full |
| `scale_code` / `stage_of_complexity` | `Node.scale_code` | ✅ Full |
| `octave_id` | `Node.octave_id` | ✅ Full |
| `coordinate.octave` | `Node.verticality_json` (parsed) | ✅ Full (Phase 7) |
| `coordinate.density` | `Node.verticality_json` (parsed) | ✅ Full (Phase 7) |
| `coordinate.sub_density` | `Node.verticality_json` (parsed) | ✅ Full (Phase 7) |
| `coordinate.collectivity` | `Node.collectivity` | ✅ Full (Phase 7) |
| `coordinate.realm` | `Node.realm_placement` | ✅ Full (Phase 7) |
| `nesting_sub` / `nesting_sup` | `Node.nesting_sub/sup` | ✅ Full (Phase 7) |
| `attractor_field` | `attractor_field_json` column + `AttractorField` struct | ✅ Full |
| `health_score` | `health_json` column + `Health` struct (G_z, P_z) | ✅ Full |
| `metabolic_dynamics` | `lesser_cycle_json` + `GreaterCycleState` | ✅ Full |
| `archetypal_mind` | `ArchetypalLoads` in `AttractorField` | ✅ Full |
| `drives_and_shadows` | `drives_json` + `Shadow` enum (universal semantics) | ✅ Full (Phase 8) |
| `provenance` | `mutation_log` + `events` + `source` field | ✅ Full |
| `edges` | `edges` table + `Edge` struct | ✅ Full |
| `tetra_coordinates` | `tetra_ul/ur/ll/lr` (deprecated, kept for compat) | ✅ Deprecated |

### 1.2 Fields Missing from TDG (12 fields)

| HoloOS schema field | TDG status | Impact | Priority |
|---|---|---|---|
| `substrate_layer_law` | Absent | The substrate-layer ↔ Law correspondence (D1→Free Will, D2→Love, D3→Light) is not stored. Low impact for agent memory — this is an R&D annotation. | LOW |
| `veil_status` | Absent | The "Veil of Forgetting" status (D3-specific involution→evolution membrane). Not needed for agent memory. | LOW |
| `involution_ground` | Absent | Cross-octave involution lineage block. Stored in `octave_id` but not as a structured block. | LOW |
| `quantum_isomorphism` | Absent | The QIM boundary-depth/symmetry/relational-alignment fields. Not needed for agent memory. | LOW |
| `goldilocks_zone` | Absent | Contact-boundary coupling thresholds. Partially captured in lesser cycle `eta` values. | LOW |
| `components` | Absent | The 3-complex (Mind/Body/Spirit) component structure. Not needed for agent memory. | LOW |
| `framework_plugins` | Absent | Plugin system metadata. Not applicable to tdg-rust. | N/A |
| `cumulative_consciousness_index` | Absent | A composite metric. Could be computed from G_z·P_z but not stored. | LOW |
| `holonic_integrity` | Absent | A structural-integrity metric. Partially captured by `StabilityFilter`. | LOW |
| `ray` | Absent | The VIBGYOR ray assignment. Could be derived from `developmental_stage`. | LOW |
| `line` | Absent | The line of development. Not needed for agent memory. | LOW |
| `substrate` | Absent | The substrate label (physical, energetic, informational). Partially captured by `realm_placement`. | LOW |

**Verdict:** All 12 missing fields are LOW priority for agent memory infrastructure. They are HoloOS R&D annotations, not memory-operational requirements. TDG-rust correctly focuses on the metabolic and epistemic primitives, not the R&D-content fields.

### 1.3 Semantics Alignment ✅

| HoloOS requirement | TDG status |
|---|---|
| Universal terminology (Epistemology doc 6) | ✅ Shadow variants renamed to MatrixHyperIngestion etc. (Phase 8) |
| `HealthState::Depolarized` (not "Sinkhole") | ✅ Renamed (Phase 8) |
| Backward-compatible JSON aliases | ✅ serde aliases preserve old names |
| D3-human-experiential terms in output strings | ⚠️ `feeling.rs` still uses "compulsive", "conflict", "exhaustion" — needs Phase 8 cleanup |

### 1.4 Coordinate System Alignment ✅

| HoloOS requirement | TDG status |
|---|---|
| V = ⟨O, D, S⟩ verticality | ✅ `verticality_json` stores {octave, density, sub_density} (Phase 7) |
| C = collectivity | ✅ `collectivity` column (Phase 7) |
| R = realm_placement | ✅ `realm_placement` column with backfill (Phase 7) |
| N = nesting (query param) | ✅ `nesting_sub/sup` columns (Phase 7) |
| Density naming (octave-agnostic VIBGYOR) | ✅ Not hardcoded — TDG uses numeric density (1-7) |
| Coordinate object in ContextPack | ⚠️ `ContextPack` identity doesn't include realm/verticality yet |

---

## Part 2: Memory-Operational-Efficacy Status

### 2.1 What's Working Well ✅

| Capability | Status | Evidence |
|---|---|---|
| Metabolic summary in agent prompt | ✅ Phase 9 | `generate_metabolic_summary()` in injector.rs — agent sees G_z/P_z averages + health distribution |
| Resonance-aware prefetch | ✅ Phase 10 | Adapter calls `tdg_resonance_partners` for top search results |
| Metabolism status in agent status | ✅ Phase 10 | `tdg_memory_status` includes queue depth |
| Realm-aware catalyst | ✅ Phase 11 | `generate_catalyst_realm_aware()` with distance multiplier |
| Realm-diversity validation gate | ✅ Phase 11 | Gate 4 checks ≥2 realms in addition to ≥2 scales |
| Universal semantics | ✅ Phase 8 | Shadow variants + HealthState renamed |

### 2.2 Remaining Efficacy Gaps (3 operational, 2 R&D-utility)

#### Gap E1 — ContextPack Caching (OPERATIONAL, MEDIUM)

**Problem:** `tdg_fetch_context` computes the ContextPack on every call. With 5 active holons, this is 5 × ~50ms = 250ms per turn. The audit plan (Phase 11) specified a `context_cache` table with 5-min TTL, but it was not implemented.

**Impact:** Agent reads are slower than necessary. On a 2GB VPS, this adds ~200ms latency per turn.

**Fix:** Add a `context_cache` table (keyed by holon_id + scope + depth, 5-min TTL). Invalidate on any Tier 1 write to the holon or its 1-hop neighbors.

#### Gap E2 — Graph-Level Mind Integration Pass (OPERATIONAL, HIGH)

**Problem:** The computational design doc specified a Tier 3 "mind pipeline integration pass" that reads graph-level health aggregates, diagnoses graph-level patterns (e.g., "GoldenAllergy: 80 observations, 0 hypotheses"), and injects catalyst into specific holons to force integration. This is the "closed loop" that turns TDG from a dashboard into a metabolism.

**Current state:** The `generate_metabolic_summary()` (Phase 9) REPORTS graph-level state but doesn't ACT on it. The greater-cycle sweep (Tier 3) enqueues GreaterTick jobs but doesn't diagnose graph-level patterns.

**Impact:** The system metabolises per-holon but doesn't integrate at the graph level. The "mind" is per-holon, not graph-level. This is the single biggest remaining efficacy gap.

**Fix:** Add a Tier 3 `mind_integration` schedule (every 15-30 min) that:
1. Queries graph-level health aggregates (mean G_z, mean P_z, depolarized count)
2. Diagnoses graph-level patterns (DarkAllergy = too few observations, GoldenAllergy = too few hypotheses)
3. Enqueues `CatalystInjection` jobs for target holons to force integration
4. This closes the loop: graph state → diagnosis → catalyst injection → per-holon metabolism → updated graph state

#### Gap E3 — Reflect by Resonance (OPERATIONAL, MEDIUM)

**Problem:** The reflect engine clusters observations by shared MENTIONS entities (entity co-occurrence). It does NOT use the attractor field or resonance graph to cluster by structural similarity.

**Impact:** Skill discovery is entity-based, not holonic-resonance-based. The agent gets skills from "observations that mention the same entity" instead of "observations that resonate structurally."

**Fix:** Add a `reflect_by_resonance()` method that loads attractor fields for recent observations, groups by type_class (donor observations cluster with acceptor observations), and creates skill nodes from resonance clusters.

#### Gap R1 — R×N Composition Engine (R&D-UTILITY, LOW for TDG)

**HoloOS gap:** The `holos holograph --realm <R>` flag is not implemented in HoloOS. The R × N composition matrix (9 exploration modes) cannot be exercised.

**TDG relevance:** LOW. TDG is agent memory, not an R&D exploration tool. The `nesting_sub/sup` columns exist but TDG doesn't need to implement the 9-mode exploration matrix. This is a HoloOS CLI concern.

#### Gap R4 — Significator-Liminality Detection (R&D-UTILITY, LOW for TDG)

**HoloOS gap:** `holos health` doesn't detect the Significator-Liminal state (phase-transition signature).

**TDG relevance:** LOW. TDG's greater cycle already models the transformation crucible (which IS the Significator-Liminal state). The `GreaterPhase::TransformationCrucible` captures this structurally. The HoloOS gap is about a CLI flag, not a structural absence in TDG.

---

## Part 3: Updated Alignment Score

| Category | Previous (1st audit) | Current (2nd audit) | Change |
|----------|---------------------|---------------------|--------|
| Core metabolic engine | 10/10 | 10/10 | — |
| Attractor field + health | 9/10 | 10/10 | +1 (universal semantics) |
| Status ladder + 5-Gate validation | 8/10 | 9/10 | +1 (realm diversity gate) |
| ContextPack | 7/10 | 7/10 | — (needs realm in identity + caching) |
| Coordinate system | 4/10 | 8/10 | +4 (V/C/R/N added) |
| Semantics | 5/10 | 8/10 | +3 (Shadow + HealthState renamed; feeling.rs still has D3 terms) |
| Mind pipeline integration | 3/10 | 7/10 | +4 (metabolic summary in prompt) |
| Hermés adapter integration | 2/10 | 6/10 | +4 (resonance prefetch + metabolism status) |
| Graph-level mind (closed loop) | 3/10 | 3/10 | — (still no graph-level integration pass) |
| **Overall alignment** | **5.7/10** | **7.6/10** | **+1.9** |

---

## Part 4: The Audit Plan

### Phase 12: Graph-Level Mind Integration (CLOSED LOOP)

**Priority:** HIGH — the single biggest remaining efficacy gap.

1. Add a `mind_integration` Tier 3 schedule to `main.rs` (every 15 min, configurable via `TDG_MIND_INTEGRATION_INTERVAL_SECS`)
2. Implement `run_mind_integration(conn)` in `src/mind/mod.rs`:
   - Query `SELECT COUNT(*) FROM nodes WHERE node_type = 'observation'` and `SELECT COUNT(*) FROM nodes WHERE node_type = 'hypothesis'`
   - Diagnose: if observations/hypotheses > 10 → GoldenAllergy (no emergence); if < 0.1 → GoldenHyperIngestion (speculation)
   - Diagnose: if mean G_z < 30 → graph-level collapse; if mean P_z < 10 → graph-level depolarization
   - For GoldenAllergy: enqueue `CatalystInjection` jobs for the top 3 observation clusters (force digestion cascade)
   - For depolarization: enqueue `CatalystInjection` for the most-connected holon (force transformation pressure)
3. Log diagnoses + injections as events

### Phase 13: ContextPack Caching + Realm in Identity

**Priority:** MEDIUM — improves read latency + ontological completeness.

1. Add `context_cache` table (holon_id, scope, depth, token_budget, context_json, rendered_markdown, computed_at, expires_at)
2. Implement cache check in `build_context_pack()` — return cached if fresh (< 5 min)
3. Invalidate on any write to the holon or its 1-hop neighbors
4. Add `realm_placement` and `verticality` to `ContextPack` identity section
5. Add `realm_placement` to `ContextPack.to_prompt_block()` output

### Phase 14: Reflect by Resonance + Feeling.rs Semantics Cleanup

**Priority:** MEDIUM — improves skill discovery + completes semantics alignment.

1. Add `reflect_by_resonance()` method to `ReflectEngine`:
   - Load attractor fields for recent observations
   - Group by type_class (donor + acceptor pairs cluster together)
   - Create skill nodes from resonance clusters
2. Update `feeling.rs` output strings to use universal terminology:
   - "compulsive expression" → "hyper-ingestion pattern"
   - "compulsive avoidance" → "hypo-ingestion pattern"
   - "internal conflict" → "tension-pair pattern"
   - "exhaustion" → "resource depletion"
   - "needs attention" → "dormant — awaiting catalyst"

### Phase 15: Dissolution Ratio + Resonance Graph Rebuild

**Priority:** LOW — improves greater-cycle precision + resonance graph accuracy.

1. Add `dissolution_ratio` field to `GreaterCycleState` (tracks how much of the old Significator has dissolved during the crucible)
2. Update `TransformationReintegration` phase to rebuild based on dissolution ratio
3. Add a Tier 3 hourly `resonance_graph` full rebuild (corrects incremental drift)
4. Add `attractor_lazy` config option (compute attractor on first query, not on every lesser cycle completion)

---

## Summary

The second-pass audit confirms that the Phase 7-11 alignment was effective — the score rose from 5.7 to 7.6. The remaining gaps are:

1. **Graph-level mind integration (HIGH)** — the system metabolises per-holon but doesn't close the loop at the graph level. This is the last critical efficacy gap.
2. **ContextPack caching (MEDIUM)** — 200ms/turn latency that could be 5ms with caching.
3. **Reflect by resonance (MEDIUM)** — skill discovery is entity-based, not holonic-resonance-based.
4. **feeling.rs semantics (MEDIUM)** — last remaining D3-human-experiential terms.
5. **Dissolution ratio + resonance rebuild (LOW)** — precision improvements.

The 12 missing HoloOS schema fields are all LOW priority — they are R&D annotations, not memory-operational requirements. TDG-rust correctly focuses on the metabolic and epistemic primitives.

**Recommended execution order:** Phase 12 (graph-level mind) → Phase 13 (caching + realm in ContextPack) → Phase 14 (reflect by resonance + feeling.rs cleanup) → Phase 15 (dissolution + rebuild)

Phase 12 is highest priority because it closes the loop — without it, the system is a per-holon metabolism, not a graph-level mind.

---

*Second-pass audit completed 2026-07-03. HoloOS pulled to latest (commit a6a9b0f83). TDG-rust at post-Phase 7-11 state (commit d8ae992).*
