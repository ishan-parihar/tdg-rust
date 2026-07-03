# Ontological-Alignment & Memory-Operational-Efficacy Audit

**Audit date:** 2026-07-03
**TDG-rust commit:** `a72292b` (post-Phase 6 + ponytail optimization)
**HoloOS commit:** `8ee564874` (latest pull — includes 08.8.17-08.8.20, Epistemology doc 6)
**Scope:** Full ontological alignment check + memory operational efficacy assessment

---

## Executive Summary

tdg-rust has successfully embodied the **core** holonic-science ontology (20/20 Phase 1-6 invariants). However, HoloOS has evolved significantly since the initial audit, adding:

1. **A redesigned coordinate system** (08.8.x series): V/C/R/N (Verticality, Collectivity, Realm-Placement, Nesting) — replacing the old Tetra-Axes UL/UR/LL/LR
2. **The Primal Distortion Genesis Theorem** (08.8.7): Three Laws (Free Will, Love, Light) formalized as octave-progression foundations
3. **The Holonically-Universal Semantics Protocol** (Epistemology doc 6 + 08.8.17): Eliminating D3-human-experiential terminology
4. **The KB Architecture Audit** (08.8.20): Identifying the `_CREATION/` vs `_UNIVERSE/` inconsistency and 12 ontological gaps

The audit identifies **3 critical alignment gaps**, **4 operational efficacy gaps**, and **5 optimization opportunities**. None are regressions — they are evolution gaps where HoloOS theory has advanced beyond what tdg-rust currently implements.

---

## Part 1: Ontological Alignment Audit

### 1.1 Coordinate System Mismatch (CRITICAL)

**HoloOS (08.8.x series):** The coordinate system has been redesigned to:
```
Holon Coordinate = ⟨ V, C, R, N ⟩

  V = Verticality     = ⟨ O, D, S ⟩ — consciousness-condensation altitude
  C = Collectivity    = Individual ↔ Collective gradient
  R = Realm-Placement = Gross | Subtle | Causal (within-octave dimensional axis)
  N = Nesting         = Sub(N) ↓ / Sup(N) ↑ (directional exploration)
```

**tdg-rust (current):** Uses the OLD Tetra-Axes system:
```
Node fields: tetra_ul, tetra_ur, tetra_ll, tetra_lr (1-19 each)
```

**Gap:** The Tetra-Axes (UL/UR/LL/LR from Wilber AQAL) have been superseded by the V/C/R/N system in HoloOS. The old system conflated within-octave dimensional architecture (realms) with cross-octave verticality (nesting) — HoloOS revision 2 explicitly corrected this conflation.

**Impact:** tdg-rust's `tetra_*` columns are ontologically stale. The `scale_codes` module is still valid (S00-S80 organisational codes) but the 4-axis coordinate is wrong.

**Fix:** Add `verticality_json`, `collectivity`, `realm_placement`, `nesting_sub`, `nesting_sup` columns. Deprecate (but don't delete) `tetra_*` columns. Update `Holon` newtype to expose the new coordinate system. Update `ContextPack` to include realm/verticality in the identity section.

### 1.2 Realm-Placement Absent (CRITICAL)

**HoloOS (08.8.3):** Every holon has a Realm-Placement (Gross/Subtle/Causal) that determines where its Experience operates. This is a within-octave dimensional axis, NOT cross-octave.

- **Causal** = Free Will (First Distortion — formless potentiality)
- **Subtle** = Light (Third Distortion — archetypal form)
- **Gross** = physical manifestation (downstream of Light — spacetime, matter)

**tdg-rust:** No realm placement field. No concept of Gross/Subtle/Causal.

**Impact:** The metabolism doesn't know which dimensional layer a holon's Experience operates in. A "Gross" observation (physical event) and a "Causal" observation (formless principle) are treated identically — they should generate different catalyst types and process through different metabolic pathways.

**Fix:** Add `realm_placement` column (`TEXT`, values: "gross" | "subtle" | "causal"). Update catalyst generation to consider realm (cross-realm catalyst generates more pressure than same-realm). Update the 5-Gate cosmological scope check to verify realm diversity, not just scale diversity.

### 1.3 D3-Human-Experiential Terminology (HIGH)

**HoloOS (08.8.17 + Epistemology doc 6):** The Holonically-Universal Semantics Protocol requires eliminating D3-human-experiential terms. The audit identifies terms like "addiction," "allergy," "shadow," "feeling," "crisis" as D3-human projections that limit ontological accuracy.

**tdg-rust (current):** Uses D3-human-experiential terms extensively:
- `Shadow::DarkAddiction`, `Shadow::DarkAllergy`, `Shadow::GoldenAddiction`, `Shadow::GoldenAllergy`
- `feeling.rs` — "compulsive expression detected", "internal conflict detected", "exhaustion"
- `diagnostic.rs` — "addiction", "allergy", "blind spot", "pathological"
- `health.rs` — `HealthState::Sinkhole` (humanistic metaphor)

**Impact:** The terminology is not scale-invariant. A galaxy doesn't "feel exhausted"; a quantum system doesn't have an "addiction." The underlying metabolic phenomena (hyper-ingestion, hypo-ingestion, depolarization) are real at every scale, but the labels are D3-projections.

**Fix:** Add universal-equivalent aliases to the enum variants (keeping backward-compatible serialization):
- `DarkAddiction` → `MatrixHyperIngestion` (serialization: `"matrix-hyper-ingestion"`)
- `DarkAllergy` → `MatrixHypoIngestion` (serialization: `"matrix-hypo-ingestion"`)
- `GoldenAddiction` → `PotentiatorHyperIngestion`
- `GoldenAllergy` → `PotentiatorHypoIngestion`
- `Sinkhole` → `Depolarized` (the actual phenomenon: P_z → 0, no tension)
- Update `feeling.rs` to use universal terminology in output strings

### 1.4 Primal Distortion Genesis Theorem (MEDIUM)

**HoloOS (08.8.7):** The Three Laws (Free Will, Love, Light) are formalized as the foundational distortions that structure each octave. Each Law required a complete previous octave to establish.

**tdg-rust:** No concept of the Three Primal Distortions. The `octave_id` field exists but has no semantic connection to the Laws.

**Impact:** Low for memory infrastructure — the Primal Distortions are primarily an R&D concern for HoloOS's knowledge base, not for agent memory. However, the attractor field's polarity (STO/STS) derives from the First Distortion (Free Will = the principle of choice), and this connection is not surfaced.

**Fix:** Document the connection in `attractor.rs` comments. No code change needed for memory infrastructure — this is an ontological annotation, not a functional gap.

### 1.5 Phase-Transition Disorientation Theorem (LOW)

**HoloOS (08.8.14):** Phase transitions at octave boundaries produce a temporary "disorientation" (now renamed to "Significator-dissolution" per the semantics protocol) — the holon's identity-pattern dissolves before reforming at the new level.

**tdg-rust:** The greater cycle's `TransformationCrucible` phase captures this structurally (the Significator is restructured), but the "dissolution" aspect (temporary loss of identity before reformation) is not explicitly modeled.

**Impact:** Low — the crucible phase already captures the restructuring event. The dissolution is implicit in the pressure consumption (50% of pressure consumed, significator shifted).

**Fix:** Add a `dissolution_ratio` field to `GreaterCycleState` that tracks how much of the old Significator has dissolved during the crucible. Update the `TransformationReintegration` phase to rebuild based on the dissolution ratio.

---

## Part 2: Memory-Operational-Efficacy Audit

### 2.1 Mind Pipeline Not Integrated with Metabolism (CRITICAL)

**The problem:** The `src/mind/` pipeline (diagnostic, feeling, pulse, consolidation, reflect, terrain, injector) is the **old pre-Phase-2 system**. It uses:
- `compute_drive_distribution()` — old drive-averaging heuristic
- `label_drive_patterns()` — old addiction/allergy labeling
- `compute_graph_entropy()` — old Shannon entropy
- `feeling.rs` — old node-count-based energy bucketing

None of these use the **new** metabolism (lesser cycle, attractor field, G_z/P_z, greater cycle). The mind pipeline and the metabolism are **two parallel systems that don't talk to each other**.

**Impact:** The agent's context prompt (`tdg_context` → `injector::generate_prompt()`) uses old heuristics, NOT the new metabolic health. The agent sees "drive distribution: eros=2.1" instead of "G_z=45.2, P_z=12.3, state=sinkhole". The metabolism computes rich health data that the mind pipeline doesn't surface.

**Fix:** Rewrite `injector.rs` to call `tdg_fetch_context` (the ContextPack) instead of the old heuristic sections. The ContextPack already includes attractor field, health, lesser/greater cycle state. The old `diagnostic.rs` and `feeling.rs` can be deprecated (or kept as fallback when metabolism hasn't run yet).

### 2.2 Hermés Adapter Not Updated (CRITICAL)

**The problem:** The `plugins/tdg/__init__.py` adapter still calls:
- `tdg_context` (old unstructured prompt) instead of `tdg_fetch_context` (ContextPack)
- `tdg_observe` (without metabolism awareness) instead of also calling `tdg_submit_synthesis`
- `tdg_search` (FTS5 only) instead of also using `tdg_resonance_partners` for bonding-aware recall

**Impact:** The agent never sees the ContextPack's `[status: ai-draft]` tags. The agent never gets resonance-based recall suggestions. The agent's syntheses are never validated through the 5-Gate. The new Phase 1-6 tools are **completely invisible to the agent**.

**Fix:** Update the adapter:
1. Replace `tdg_context` calls with `tdg_fetch_context` (format="markdown")
2. Add `tdg_resonance_partners` to the prefetch path (after search, find resonance partners for top results)
3. Wire `on_memory_write` to call `tdg_submit_synthesis` for any substantial memory write
4. Add `tdg_metabolism_status` to the `tdg_memory_status` tool (so the agent can see queue depth)

### 2.3 Reflect Engine Not Using Metabolism (HIGH)

**The problem:** The `src/mind/reflect_engine.rs` clusters observations by shared MENTIONS entities and creates skill nodes. It does NOT use:
- The attractor field (it could cluster by type_class, not just entity overlap)
- The resonance graph (it could find bonding candidates, not just entity co-occurrence)
- The lesser cycle state (it could prioritize observations with high transformation_pressure)

**Impact:** Skill discovery is entity-co-occurrence-based, not holonic-resonance-based. The agent gets skills from "observations that mention the same entity" instead of "observations that resonate structurally."

**Fix:** Add a `reflect_by_resonance()` method that:
1. Loads attractor fields for recent observations
2. Groups by type_class (donor observations cluster with acceptor observations)
3. Creates skill nodes from resonance clusters, not just entity clusters
4. Prioritizes observations with high transformation_pressure (they're metabolically active)

### 2.4 Catalyst Generation Doesn't Consider Realm (MEDIUM)

**The problem:** `generate_catalyst()` in `lesser_cycle.rs` computes catalyst from edge type × weight × drive complementarity. It does NOT consider:
- Realm placement (cross-realm catalyst should generate more pressure — a Causal principle manifesting in Gross reality is a bigger perturbation than a Gross-to-Gross interaction)
- Verticality (cross-density catalyst — a D7 holon interacting with a D3 holon generates more pressure than D3-to-D3)
- Collectivity (Individual-to-Collective interactions have different dynamics than Individual-to-Individual)

**Impact:** All catalyst is treated equally regardless of dimensional scope. The metabolism doesn't distinguish between "a trivial same-realm interaction" and "a profound cross-realm breakthrough."

**Fix:** Extend `generate_catalyst()` to take optional realm/verticality/collectivity parameters and multiply the catalyst by a dimensional-distance factor. Cross-realm interactions get 2x catalyst; cross-density gets 1.5x; same-realm-same-density gets 1.0x.

### 2.5 5-Gate Cosmological Scope Only Checks Scale (MEDIUM)

**The problem:** Gate 4 (cosmological scope) checks that invariant claims cite ≥2 different `scale_code` values. It does NOT check:
- Realm diversity (a claim spanning Gross + Subtle + Causal is more cosmologically grounded than one spanning only Gross)
- Verticality diversity (a claim spanning D1 + D7 is more cosmologically grounded than D3 + D4)

**Impact:** A synthesis citing two Gross-scale observations (S40 + S50) passes the gate, but it's not cosmologically universal — it's only grounded in one realm at one density band.

**Fix:** Extend Gate 4 to check realm diversity (≥2 realms) AND scale diversity (≥2 scales). Update the SQL query to join on `realm_placement` (once the column exists).

### 2.6 No Graph-Level Mind Pipeline (MEDIUM)

**The problem:** The computational design doc specifies a Tier 3 mind pipeline that reads graph-level health aggregates and feeds catalyst back to specific holons. This is the "closed loop" that turns TDG from a dashboard into a metabolism.

**tdg-rust (current):** The Tier 3 greater-cycle sweep exists (every 10 min), but it only enqueues `GreaterTick` jobs. There is no "mind pipeline integration pass" that:
1. Reads `AVG(g_z)`, `AVG(p_z)` across all holons
2. Diagnoses graph-level shadows (e.g., "GoldenAllergy: 80 observations, 0 hypotheses")
3. Injects catalyst into specific holons to force integration

**Impact:** The system metabolises per-holon (lesser cycle + greater cycle) but doesn't integrate at the graph level. The "mind" is per-holon, not graph-level.

**Fix:** Add a Tier 3 `mind_integration` schedule (every 15-30 min) that:
1. Queries `holon_summary` (or computes aggregates from `health_json`)
2. Diagnoses graph-level patterns
3. Enqueues `CatalystInjection` jobs for target holons
4. This is Phase 7 of the computational design doc — it was planned but not implemented.

---

## Part 3: Optimization Opportunities

### 3.1 ContextPack Caching (MEDIUM)

The `tdg_fetch_context` tool computes the ContextPack on every call. With 5 active holons, this is 5 × ~50ms = 250ms per turn. A simple `context_cache` table (keyed by holon_id + scope + depth, 5-min TTL) would reduce this to <5ms for cached reads.

### 3.2 Resonance Graph Full Rebuild (LOW)

The `resonance_graph` is updated incrementally when attractor fields change, but there's no periodic full rebuild. Over time, incremental updates drift (deleted holons leave orphan entries, type_class changes don't always trigger updates). A Tier 3 hourly full rebuild would correct this.

### 3.3 Metabolism Worker Coalescing (LOW)

When multiple catalyst injections arrive for the same holon within a short window, each enqueues a separate `CatalystInjection` job. A coalescing step (sum catalyst amounts for the same holon before ticking) would reduce redundant ticks.

### 3.4 Attractor Field Lazy Computation (LOW)

The `RecomputeAttractor` job runs whenever the lesser cycle reaches Integrating phase. But many holons never get queried — their attractor fields are computed but never read. A lazy approach (compute on first `tdg_attractor` query, not on every lesser cycle completion) would save ~40% of metabolism CPU.

### 3.5 Hermés Adapter Session Reuse (LOW)

The adapter spawns a new `tdg-rust serve` subprocess for every tool call. A persistent stdio session (one subprocess per agent session, not per call) would eliminate ~200ms of process startup per call.

---

## Part 4: Alignment Score

| Category | Score | Notes |
|----------|-------|-------|
| Core metabolic engine (M·P·C·E + S·T·G·Ch) | 10/10 | Fully aligned |
| Attractor field + health metrics | 9/10 | Formulas correct, terminology needs universal semantics update |
| Status ladder + 5-Gate validation | 8/10 | Functional, but Gate 4 only checks scale diversity (needs realm) |
| ContextPack | 7/10 | Structurally correct, but missing realm/verticality in identity |
| Coordinate system | 4/10 | Uses old Tetra-Axes, not new V/C/R/N |
| Semantics | 5/10 | Uses D3-human-experiential terms (addiction, allergy, shadow, feeling) |
| Mind pipeline integration | 3/10 | Old heuristics, NOT integrated with metabolism |
| Hermés adapter integration | 2/10 | Still uses old tools, doesn't use ContextPack or submit_synthesis |
| Graph-level mind (closed loop) | 3/10 | Tier 3 sweep exists but no graph-level integration pass |
| **Overall alignment** | **5.7/10** | Core is solid; integration layers need work |

---

## Part 5: The Audit Plan

### Phase 7: Coordinate System Migration (V/C/R/N)

**Priority:** CRITICAL — everything else depends on having the right coordinate system.

1. Add `realm_placement` column (`TEXT`, values: "gross" | "subtle" | "causal")
2. Add `verticality_json` column (`TEXT`, stores `{"octave": N, "density": D, "sub_density": S}`)
3. Add `collectivity` column (`TEXT`, values: "individual" | "collective" | "universal")
4. Add `nesting_sub` and `nesting_sup` columns (`INTEGER`, query parameters for exploration depth)
5. Deprecate `tetra_ul/ur/ll/lr` (keep for backward compat, don't use in new code)
6. Update `Holon` newtype to expose V/C/R/N
7. Update `ContextPack` identity section to include realm + verticality
8. Update `scale_codes.rs` to add realm inference from node_type

### Phase 8: Semantics Protocol Alignment

**Priority:** HIGH — required for ontological accuracy.

1. Rename `Shadow` enum variants to universal equivalents:
   - `DarkAddiction` → `MatrixHyperIngestion`
   - `DarkAllergy` → `MatrixHypoIngestion`
   - `GoldenAddiction` → `PotentiatorHyperIngestion`
   - `GoldenAllergy` → `PotentiatorHypoIngestion`
2. Rename `HealthState::Sinkhole` → `HealthState::Depolarized`
3. Update `feeling.rs` output strings to use universal terminology
4. Update `diagnostic.rs` labels to use universal terminology
5. Keep old names as aliases for backward-compatible JSON serialization

### Phase 9: Mind Pipeline Integration

**Priority:** CRITICAL — the metabolism is invisible to the agent without this.

1. Rewrite `injector.rs` to call `build_context_pack()` instead of old heuristic sections
2. Deprecate `diagnostic.rs` and `feeling.rs` (keep as fallback when metabolism hasn't run)
3. Update `reflect_engine.rs` to cluster by resonance, not just entity co-occurrence
4. Add `reflect_by_resonance()` method
5. Add graph-level mind integration pass (Tier 3 schedule, every 15-30 min)

### Phase 10: Hermés Adapter Update

**Priority:** CRITICAL — the agent can't use the new features without this.

1. Replace `tdg_context` with `tdg_fetch_context` (format="markdown")
2. Add `tdg_resonance_partners` to prefetch
3. Wire `on_memory_write` to `tdg_submit_synthesis`
4. Add `tdg_metabolism_status` to `tdg_memory_status`
5. Implement persistent stdio session (one subprocess per session)

### Phase 11: Catalyst + Gate Enhancements

**Priority:** MEDIUM — improves metabolic precision.

1. Extend `generate_catalyst()` with realm/verticality distance factor
2. Extend Gate 4 to check realm diversity (≥2 realms) in addition to scale diversity
3. Add `dissolution_ratio` to greater cycle crucible phase
4. Add ContextPack caching (5-min TTL)
5. Add hourly resonance_graph full rebuild

---

## Summary

The core holonic-science engine (Phases 1-6) is solid — all 20 invariants are embodied, the metabolism works, health is computed, resonance predicts bonding. But HoloOS has evolved, and tdg-rust's integration layers haven't kept up:

1. **The coordinate system is stale** (old Tetra-Axes, not new V/C/R/N)
2. **The terminology is D3-human-experiential** (addiction/allergy/shadow, not hyper/hypo-ingestion)
3. **The mind pipeline doesn't use the metabolism** (old heuristics, not G_z/P_z)
4. **The Hermés adapter doesn't use the new tools** (tdg_context, not tdg_fetch_context)

The fix is 5 phases (7-11) that migrate the coordinate system, align the semantics, integrate the mind pipeline with the metabolism, update the adapter, and enhance the catalyst and validation gates. Each phase is independently shippable.

**Priority order:** Phase 9 (mind pipeline) → Phase 10 (adapter) → Phase 7 (coordinate system) → Phase 8 (semantics) → Phase 11 (enhancements)

Phase 9 and 10 are highest priority because they make the existing metabolism **visible to the agent** — without them, the agent can't benefit from the Phase 1-6 work.

---

*Audit completed 2026-07-03. HoloOS pulled to latest (commit 8ee564874). TDG-rust at post-optimization state (commit a72292b).*
