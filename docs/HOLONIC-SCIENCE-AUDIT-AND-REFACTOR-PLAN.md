# tdg-rust Holonomic-Science Audit & Refactor Plan

**Audit target:** `ishan-parihar/tdg-rust` (v0.5.0, ~32.7k LOC Rust, 491 tests, 36 MCP tools)
**Reference ontology:** `ishanparihar/HoloOS` — `_THEORY/01_Epistemology/`, `_THEORY/02_Ontology/` (31 docs), `_THEORY/06_Stage_Theory/`, `_THEORY/09_Thermodynamic_Framework/`, `AGENTS.md`
**Reference implementation:** `HoloOS/_INSTRUMENTS/lib/` (47 Python modules, 21 MCP tools, 5-gate validation)
**Audit date:** 2026-07-03
**Auditor methodology:** Three parallel deep-reads — (a) TDG theory foundations, (b) HoloOS Python reference impl, (c) tdg-rust source. Synthesised into this document.

---

## 0. Executive Summary — Read This First

**tdg-rust is a competent developmental task graph with drive propagation, not a Holon graph.** It implements the *vocabulary* of holonic science (stages, levels, quadrants, drives, catalysts, telos hierarchy, digestion) but lacks the *operatorial core* — the dual metabolic cycle (M·P·C·E + S·T·G·Ch) that IS the trusted anchor of the entire TDG ontology.

### The One-Sentence Finding

> tdg-rust stores holonic vocabulary as data; it does not enact holonic metabolism as computation. Drives are scalar vectors propagated through edges — they are never *generated* at a contact boundary, never *consumed* by a lesser cycle, never *transformed* by a greater cycle. The "mind" pipeline is a retrospective dashboard, not a metabolism.

### What's Working (keep)

- Stage-gated promotion with evidence thresholds AND age gates — genuinely well-designed, prevents premature advancement.
- Dual-pole drives (eros/agape/agency/communion) with quadrant modulators and variance-floor stabilisation — a real (if partial) embodiment of the polarity principle.
- Telearchy validation invariants (max parent-child stage delta = 2, T0 requires T4 parent, bypass risk > 0.5 flagged).
- 5-report audit engine (integrity, polarity, stage progression, persistence, capability) surfaces real pathology.
- Provenance tracked everywhere (`source` field, `agent_id`, `mutation_log` table) — meets a core holonic requirement.
- Production-grade infra: SQLite WAL + FTS5 + connection pool, circuit breaker, write guard, ONNX embeddings, scheduled maintenance (6h SelfManager + 5min health check).
- 491 tests, proptest fuzzing, criterion benchmarks, integration tests for MCP E2E.

### What's Missing (the ontological gap)

| Primitive | Theory status | tdg-rust status |
|---|---|---|
| Lesser cycle (M·P·C·E) | **canonical** (the trusted anchor) | **ABSENT** |
| Greater cycle (S·T·G·Ch) | **canonical** | **ABSENT** |
| Contact boundary (shared by both cycles) | **canonical** | **ABSENT** |
| Attractor field A(H) = ⟨A_M, A_P, A_G, Γ⟩ | **canonical-hypothesis** | **ABSENT** |
| G_z (integrative efficiency) | **canonical** (formula) | **ABSENT** (closest: `integration_score`, unused) |
| P_z (transcendental tension) | **canonical** (formula) | **ABSENT** |
| Resonance R(H1, H2) | **canonical-hypothesis** | **ABSENT** (only embedding cosine sim) |
| Status ladder (ai-draft → canonical-hypothesis → canonical → superseded) | **canonical** | **MISMATCHED** (ad-hoc `lifecycle_state` strings) |
| Type class (e.g. `strong-donor-sto`) | **canonical** | **ABSENT** |
| Type ⊥ Stage orthogonality | **canonical** | **ABSENT** (no type system at all) |
| 5-Gate Validation | **canonical** | **ABSENT** |
| Witnesses vs Sources epistemic distinction | **canonical** | **ABSENT** |
| Scale codes (S00–S80) | **canonical-hypothesis** | **ABSENT** |
| Tetra-Axes (4-axis coordinate system) | **canonical-hypothesis** | **ABSENT** (only single-quadrant label) |
| 8-role load vector (M·P·C·E·S·T·G·Ch) | **canonical-hypothesis** | **ABSENT** (only 4-drive vector) |
| 22 named archetypes | **ai-draft** | **ABSENT** |
| ContextPack (single-call intra+inter+extra) | **canonical** (operational) | **PARTIAL** (`tdg_context` exists but not structured) |
| Phase-transition / thermodynamic model | **ai-draft** | **PARTIAL** (`compute_graph_entropy` exists, passive only) |
| Provenance on every semantic write | **canonical** | **EMBODIED** |
| Stage codes (8 stages) | **canonical** | **EMBODIED** |

### What's Broken (engineering hygiene)

1. **Stale audit docs** — `AUDIT_REPORT.md` (July 2026) describes bugs that are largely fixed. Misleads new contributors.
2. **God module** — `src/mcp/tools.rs` is **3,464 LOC** (was 2,650 in the prior audit — it got *worse*).
3. **Dead code in production** — `DiagnosticEngine::analyze` is called from `injector.rs:118` with `&[]` for both `drive_history` and `quadrant_history`, so the persistence-warning and quadrant-repetition features are inert.
4. **Dual `DualPoleDrive` structs** — `models.rs:188` (2 fields, unused) vs `flow.rs:111` (4 fields, canonical). The 2-field struct is dead.
5. **`DriveVector` claims "16 drive dimensions"** (`models.rs:194`) but only 4 are realised. Aspirational comment from the Python prototype, never reconciled.
6. **MindStateManager claims "dual persistence (JSON + SQLite WAL)"** (`state.rs:90`) — only JSON is implemented; WAL is "future: eventsourcing".
7. **`agents_path` only uses `parent_ids[0]`** (`crud.rs:1509`) — multi-parent holons lose path information for parents 2..N.
8. **Hardcoded agent name "Sisyphus"** (`state.rs:49`) — not parameterised via config.
9. **MCP doc drift** — `mcp/mod.rs` claims 17 tools; there are actually 36.
10. **HoloOS MCP doc drift** — `mcp_server.py` docstring says 16 tools; there are 21.

### The Refactor Plan in One Paragraph

Add the operatorial core in 6 phases without throwing away the existing infrastructure. **Phase 0** cleans the hygiene debt so the codebase is safe to extend. **Phase 1** introduces a first-class `Holon` type and the status ladder (the scaffolding). **Phase 2** implements the lesser cycle (M·P·C·E) as a heartbeat-driven operator on every holon — this is the trusted anchor, the non-negotiable. **Phase 3** adds the attractor field A(H), G_z, P_z, and resonance — the operational object that lets us compute health. **Phase 4** adds the greater cycle (S·T·G·Ch) and phase-transition detection. **Phase 5** redesigns the agent API around the ContextPack and 5-gate validation. **Phase 6** adds the type system (22 archetypes, type_class, Type⊥Stage). Each phase is independently shippable and immediately useful.

---

## 1. Methodology

### 1.1 What was read

**HoloOS theory (Tier 1–3, ~31 docs):**
- `_THEORY/01_Epistemology/` — 5 docs (Method, Grounding Discipline, Red-Team Protocol, Derivation Patterns, Type Validation Protocol)
- `_THEORY/02_Ontology/` — 27 docs (00 master map, 01.1–01.5 primordial architecture, 02.1–02.4 dual metabolism + typology, 03.1–03.2 archetype anatomy, 04.1–04.3 specialization, 05.1–05.3 realms, 06.1–06.3 involution/evolution, 07.1 applied, 08.1–08.7 synthesis tier)
- `_THEORY/06_Stage_Theory/Integrated_Stage_Theory.md` — 13-stage / 7-ray / Integral matrix
- `_THEORY/09_Thermodynamic_Framework/01_Phase_Transition_Model_Synthesis.md` — Prigogine + Chaisson + Kauffman + Landauer
- `AGENTS.md` — operational guide (holon anatomy, attractor field, health metrics, status ladder, ContextPack, R&D process)

**HoloOS reference implementation (Python):**
- `_INSTRUMENTS/lib/` — 21 core modules (holon, ontology, agent_api, attractor, health, archetype, state_machine, validation_gate, frequency, synthesis, graph, edges, transitions, timeline, specialization, cascade, ray_center, infer_state, temporal_query, query_graph)
- `_INSTRUMENTS/schemas/` — JSON Schemas + YAML taxonomies (holon_meta, doc_frontmatter_v2, synthesis_frontmatter, provenance, timeline; taxonomy/_index, dimensions, holon_states, holon_type_placement, edge_types, scale_codes, type_codes, stage_codes, required_anatomy, atman_defenses, developmental_axes, lines_of_development)
- `_INSTRUMENTS/mcp_server.py` — 21 MCP tools

**tdg-rust implementation:**
- `Cargo.toml`, `src/lib.rs`, `src/main.rs`, `src/models.rs`, `src/schema.rs`, `src/config.rs`, `src/error.rs`
- `src/db/` — schema.rs, crud.rs, events.rs, pool.rs, write_guard.rs
- `src/telearchy.rs`, `src/flow.rs`, `src/knowledge.rs`, `src/digestion.rs`, `src/audit.rs`, `src/validation.rs`, `src/circuit_breaker.rs`, `src/graph_projection.rs`
- `src/mind/` — mod, consolidation_engine, reflect_engine, terrain, diagnostic, feeling, pulse, state, sections, injector, embedding, data_loader
- `src/grammar/` — mod, node_grammar, auto_wire
- `src/mcp/` — mod, server, tools, params, trust, health, tests
- `src/plugins/` — entity_extractor, hybrid_retriever, preference_extractor
- `src/maintenance/` — orchestrator, enricher, janitor, archiver, monitor
- `src/util/` — quadrants, math, stopwords
- `plugins/tdg/__init__.py` — the Hermés adapter
- `AUDIT_REPORT.md`, `upgrade-plan.md`, `docs/BUG-FIX-PLAN.md` — prior audits

### 1.2 How the audit was conducted

Three parallel deep-read subagents extracted structured summaries:
- **Subagent 4** — TDG theory: primitives, axioms, invariants, status ladder, health formulas, ContextPack spec, phase-transition model, 15 implementation non-negotiables.
- **Subagent 5b** — HoloOS Python reference: every field on the Holon, every method on the agent_api façade, every MCP tool, all 35 invariants enforced in code, exact G_z / P_z / Resonance formulas, the 5-gate validation logic.
- **Subagent 5** — tdg-rust: every struct, every table, every MCP tool, every invariant, all bugs from prior audits (with current status: fixed / partially fixed / still open), the operational reality vs the marketing.

This document is the synthesis of those three summaries into an actionable refactor plan.

---

## 2. The TDG Theoretical Contract

This section compresses what a TDG implementation **MUST** embody to be holonic-science compliant. It is the contract against which tdg-rust is measured in §3.

### 2.1 The Epistemology (the METHOD that constrains the ontology)

**The 5 Grounding Rules** (`_THEORY/01_Epistemology/1_Grounding_Discipline.md`):
1. **Trusted Anchor** — reduce every question to the single most undistorted articulation available. For TDG, that anchor is the lesser-cycle metabolism (Doc 02.1).
2. **Derive, Don't Borrow** — every structure must be earned from the anchor. Imported formalism is forbidden unless derived, not decorated in.
3. **Witnesses, Not Sources** — external correspondences (chemistry's octet, Law of One, enneagram, Integral stages) corroborate the invariant; they are NEVER the source. Test: if the witness were absent, would the structure still derive from the anchor?
4. **Cosmological Scope** — every ontological claim must hold for atoms AND galaxies. Reject humanistic-only framings.
5. **Modelable Boundary** — the Absolute is not modeled. Every finding is provisional, partial, perspectival.

**The Red-Team Protocol** gates admission of new ontology. 5 failure-modes + 1 cross-cutting:
1. Misplaced invariant
2. Orthogonality violation (deriving one axis from another declared independent)
3. Numerology, not isomorphism
4. Borrowed rigor
5. Unexamined flagship analogy
6. Humanistic reduction (cross-cutting)

**The Status Ladder** — every artifact carries `synthesis_status`:
- `ai-draft` — constructed but not red-teamed (ALL agent outputs start here)
- `canonical-hypothesis` — derived from anchor, joints unvalidated
- `canonical` — derived, red-teamed, joints validated (**human-only elevation**)
- `superseded` — retired tombstone

### 2.2 The Ontology Primitives (the WHAT)

**Holon** — a whole that is also a part. Every holon runs ONE invariant systems architecture with TWO symmetrical but inverted metabolic cycles operating through a SHARED contact-boundary.

**The Lesser Cycle — M·P·C·E** (the **TRUSTED ANCHOR**, canonical):
- **Matrix (M)** — Reservoir A (intra-holonic / what-is): current-state organizer, conserved structure, boundary memory, persistent identity-pattern. Holds the past.
- **Potentiator (P)** — Reservoir B (extra-holonic / what-could-be): latent-state generator, reachable possibility-space, evolutionary invitations. Holds the future.
- **Catalyst (C)** — Currency B→A: boundary-crossing pressure entering from environment, peers, components, or latent states.
- **Experience (E)** — Currency A→B: processed input stored as adaptation, learning, bias, memory.
- **Axiom:** *"What Catalyst is to the Matrix, Experience is to the Potentiator."*
- **The loop:** Matrix processes Catalyst, stores Experience. Potentiator processes Experience, stores Catalyst. The loop is **open, not closed** — it draws Catalyst from outside and accumulates Experience, pressurising ascent.

**The Greater Cycle — S·T·G·Ch** (canonical):
- **Significator (S)** — Reservoir A (intra-holonic, all stages): persistent identity-pattern, continuity reservoir. ≈ Matrix at a higher scale.
- **Great Way (G)** — Reservoir B (extra-holonic, all stages): operating environment, context receiving commitments. ≈ Potentiator at a higher scale.
- **Transformation (T)** — Currency B→A: threshold restructuring event, phase-change pressure.
- **Choice (Ch)** — Currency A→B: directional commitment, polarity/vector emitted into the operating environment.
- **Axiom:** *"What Transformation is to the Significator, Choice is to the Great Way."*
- **Critical distinction:** Potentiator ≠ Great Way. Same extra-holonic structure at different scales; NOT interchangeable.

**The Contact Boundary** — there are TWO contact boundaries (one per cycle), and the four Drives resolve into one orthogonal axis governing each:
- Lesser boundary (Matrix ⇄ Potentiator): **Eros ↔ Agape** (vertical)
- Greater boundary (Significator ⇄ Great Way): **Agency ↔ Communion** (horizontal)
- The contact-boundary (Transformation) is SHARED — the common membrane through which Catalyst and Experience flow on BOTH perspectives.

**The Attractor Field** A(H) = ⟨A_M, A_P, A_G, Γ⟩ (canonical-hypothesis, Doc 08.1):
- **A_M** — Matrix attractor: current homeostatic basin (what state it stabilises around)
- **A_P** — Potentiator attractor: latent basin (what states are reachable)
- **A_G** — Great-Way attractor: environmental basin (what collectives it can bond with)
- **Γ** — Coupling tensor: transmission profile, lives on a **2-torus** (NOT a 4-cube): horizontal drives (Ag, Cm) are anti-correlated; vertical drives (Er, Agp) are anti-correlated.
- The **Significator is implicit** — it is the time-integral of the field, not a fifth component.
- Polarity disposition π = sgn(α_M·A_M + α_P·A_P + α_G·A_G). Maps to type-columns: +1 strong donor (STO), 0 balanced sharer, −1 strong acceptor (STS), closed = noble.

**The 8-Role Load Vector** (M·P·C·E·S·T·G·Ch) — the instantaneous read-out of the attractor field. Two loops:
- **Loop A (Lesser, continuous):** M→C→P→E→M
- **Loop B (Greater, discontinuous):** S→T→G→Ch→S — fires when ℓ_T exceeds threshold

### 2.3 The Health Metrics (canonical formulas)

**G_z (Agape / Integrative Efficiency):**
```
G_z = 100 · (A_z/100 · C_z/100 · B_H · B_V)^(1/4)
```
where:
- A_z = 100·exp(−|ln(Ω_A)|), Ω_A = (M·η_M) / (|C|+ε) — Matrix-side boundary resistance
- C_z = 100·exp(−|ln(σ_C)|), σ_C = (P·η_P) / (|E|+ε) — Potentiator-side field conductance
- B_H = min(A_z, C_z)/max(A_z, C_z) — Agency↔Communion balance
- B_V = min(eros, agape)/max(eros, agape) — Eros↔Agape balance

Geometric mean of 4 factors → any single factor near 0 collapses G_z. **Rewards balance.**
- G_z > 70: optimal; 30–70: sub-optimal; < 30: collapse

**P_z (Eros / Transcendental Tension):**
```
P_z = 100 · ∇Ψ · cos(θ_alignment)
```
where:
- ∇Ψ = |P − M| / (P + M + ε) — structural potential gradient between Matrix and Potentiator
- cos(θ_alignment) — alignment of behavioural output vector with core polar archetype (STO vs STS). θ=0 → aligned; θ=π/2 → neutral (P_z=0, the sinkhole); θ=π → anti-aligned (clamped to 0).

**Rewards commitment, not balance. Neutrality is the pathology.**
- P_z > 50: optimal; 10–50: building; < 10: sinkhole of indifference

**Total health = G_z · P_z.** A holon can be metabolically efficient yet depolarised (the sinkhole). Both are required.

**Resonance R(H_1, H_2)** (canonical-hypothesis, J2):
```
R = register_complementarity · coupling_tensor_compatibility · great_way_intersection
```
- Register complementarity: every open register on H_1 has complementary open register on H_2 (donor↔acceptor, sharer↔sharer)
- Coupling-tensor compatibility: cosine similarity of Γ vectors, clamped ≥0
- Great-Way intersection: A_{G,1} ∩ A_{G,2} ≠ ∅
- R > 0.7: strong bond; 0.3–0.7: moderate; < 0.3: weak

### 2.4 Type ⊥ Stage Orthogonality (canonical)

- **Type** = the Significator's invariant bonding-disposition toward the Great Way. Stable under excitation. Bounded by (d, k) — octave-depth and active-complex count.
- **Stage** = how full/excited the metabolic engine is. Dynamic.
- **Type⊥Stage** — deriving one from the other is red-team failure-mode #2.
- 4 bonding chemistries: ionic (donor→acceptor), covalent (sharer↔sharer), dative (noble lends), metallic (pooled polarity).
- Noble is ambiguous (graduation vs sinkhole); the **Choice** flag χ ∈ {graduated, sinkhole, reopened} disambiguates.

### 2.5 The 5-Gate Validation (canonical)

Every synthesis must pass:
1. **Grounding** — cites ≥1 anchor doc from {06, 07}
2. **Failure-mode** — passes 5 QIM failure-modes + humanistic reduction
3. **Joint validation** — open joints properly labeled; no canonical claim on hypothesis-graded joints
4. **Cosmological scope** — invariant claims cite ≥2 scales (atom AND galaxy)
5. **Provenance completeness** — anchor, witnesses, derivation pattern, agent name all present

### 2.6 The ContextPack (canonical operational object)

Single-call structured object aggregating **intra/inter/extra** context:
- **intra** — attractor_field, health, archetypal_loads, drives_and_shadows, stage
- **inter** — bonds, bridges, top-5 resonances
- **extra** — parent_chain (involution lineage), sub_holons, great_way
- **analogues** — cross-domain type-homologues (max 10)
- **provenance** — last 5 events, evidence_count, open_joints
- **grounding** — anchor_docs, hypothesis_docs, epistemology_status

Token-budget truncation drops cheapest-to-lose first but **NEVER drops** `synthesis_status`, `grounding`, or `type_class` — the epistemological spine.

### 2.7 The 20 Non-Negotiable Invariants

Any implementation claiming "holonic-science compliance" MUST enforce these (full list in `_THEORY/02_Ontology/00.md` and the AGENTS.md standing rules). The most load-bearing:

1. Every holon runs BOTH cycles simultaneously through ONE shared contact boundary.
2. Cosmological scope — every invariant claim must hold for atoms AND galaxies.
3. Type ⊥ Stage orthogonality.
4. Status ladder enforcement — all agent outputs start at `ai-draft`; elevation to `canonical` is human-only.
5. 5-Gate Validation on every synthesis.
6. Provenance on every semantic write.
7. Witnesses corroborate; they are never premises.
8. Fractal recursion — every element of the architecture is itself a holon.
9. Open joints must be labeled (validated / proposed / rejected).
10. Both G_z AND P_z must be computed — never collapse to one.

---

## 3. Current State of tdg-rust

### 3.1 Identity

- Rust edition 2021, ~32.7k LOC, 491 tests, v0.5.0
- Binary: single `tdg-rust` with 15 CLI subcommands (`Serve`, `Migrate`, `Init`, `Backup`, `Stats`, `Audit`, `Check`, `Unify`, `ReconcileConstraints`, `SyncSkills`, `AutoCapture`, `Create`, `MaintenanceCheck`, `RepairOrphans`, `Embed`)
- MCP: 36 tools (stdio default on 3000; HTTP/SSE on other ports) — **doc says 17, stale**
- Embeddings: EmbeddingGemma-300M ONNX, 768-dim, Q4/Q8 — feature-gated under `onnx`
- Maintenance: background scheduler — SelfManager every 6h, health check every 5min
- Deploy targets: Hermés agent (primary), Claude Desktop, Cursor, custom MCP clients

### 3.2 The Data Model (what's actually persisted)

**`nodes` table (21 columns):**
```
id TEXT PK, node_type, name, description, properties_json, quadrants_json,
drives_json, lifecycle_state, teleological_level, developmental_stage INTEGER,
confidence REAL, source, parent_ids, agent_path, created_at, updated_at,
valid_from, valid_to, helpful_count, retrieval_count, agent_id
```

**`edges` table (11 columns):**
```
id TEXT PK, source_id FK, target_id FK, edge_type, weight REAL,
properties_json, valid_from, valid_to, created_at, updated_at, agent_id
```

**`embeddings` table:** `node_id PK FK, vector BLOB, model TEXT, updated_at, dimension INTEGER DEFAULT 384`

**`events` table:** `id PK, event_id, event_action, timestamp, node_id, source_id, target_id, payload, agent_id`

**`mutation_log` table** (Phase-5 migration): `id PK, timestamp, session_id, mutation_type, target_type, target_id, old_value, new_value, agent_id`

**`trust_scores`, `health_checks`, `leases`, `schema_meta`, `nodes_fts`** — supporting tables.

### 3.3 The 21 Node Types

| Generation | Types |
|---|---|
| v1 (13) | observation, telos, skill, capability, action, people, artifact, hypothesis, constraint, discovery, project, trajectory, synthesis |
| v4.0 (5) | being, communication, event, insight, question |
| v4.1 (3) | value, bond, narrative (labelled "Holonic types" but with NO holonic semantics — just 3 more node types) |

### 3.4 The 35 Edge Types

| Generation | Types |
|---|---|
| v1 (24) | DECOMPOSES_TO, OWNS, EXPERIENCES, PURSUES, HAS_CAPABILITY, ENABLES, CONTEXT, BLOCKS, SUPPORTS, CONTRADICTS, EVIDENCES, SYNTHESIZES, DEPENDS_ON, RELATES_TO, REFERENCES, REALIZES, PRECEDES, ALTERNATIVE_TO, OWNED_BY, MEASURED_BY, AFFECTS_QUADRANT, MENTIONS, DIGESTS_TO, PROMOTES_TO |
| v4.0 (11) | SENT, RECEIVED, TRIGGERED, DETECTED, ILLUMINATES, OPENS, SEEKS, CREATES, ADVANCES, APPEALS_TO, REPLIES, CONTINUES |

### 3.5 The 4 Dual-Pole Drives

| Drive | Semantics | Intrinsic blend |
|---|---|---|
| `eros` | Self-assertion / individuation | 70% intrinsic + 30% incoming |
| `agape` | Other-embrace / relational-gift | (same) |
| `agency` | Active potency / doing | (same) |
| `communion` | Passive-potency / being-with | (same) |

Each drive is `DualPoleDrive { positive_pole, negative_pole, availability, blind_spot }` (the canonical `flow.rs` version). The `models.rs:188` 2-field version is dead code.

Drive propagation: 3-phase pipeline (emission → stabilisation → aggregation) with quadrant modulators, variance floor (child variance ≥ intrinsic·0.3), max influence per parent = 0.6.

### 3.6 The 8 Developmental Stages + 7 Telos Levels + 10 Catalyst Types

**Stages** (with evidence thresholds / age gates in days):
```
Survival(1, 0ev, 0d) → Identity(2, 5ev, 0d) → Power(3, 15ev, 3d) →
Heart(4, 30ev, 7d) → Rational(5, 50ev, 14d) → Pluralistic(6, 80ev, 21d) →
Integral(7, 120ev, 45d) → Harvest(8, 200ev, 90d)
```

**Telos levels:** T0 (root mission, highest) → T6 (transcendent). Promotion gates: T0 requires Rational stage, T1 requires Heart, etc.

**Catalyst types:** ExternalSuccess, ExternalFailure, ExternalResponse, InternalCompletion, InternalDiscovery, ConstraintSurfaced, OpportunityDetected, RoutineObservation, SkillMastered, ProjectCreated.

### 3.7 The 36 MCP Tools

Organised by category:

| Category | Tools |
|---|---|
| **Search** | `tdg_search`, `tdg_prefetch` |
| **CRUD** | `tdg_create`, `tdg_update`, `tdg_get_node`, `tdg_bulk_create`, `tdg_observe`, `tdg_record_exec` |
| **Edges** | `tdg_connect`, `tdg_get_related` |
| **Events** | `tdg_query_events` |
| **Rating** | `tdg_rate_memory` |
| **Mind** | `tdg_mind_state`, `tdg_context` (the prompt injector) |
| **Synthesis** | `tdg_reflect` (LLM-powered), `tdg_reflect_run` (clustering) |
| **Trust** | `tdg_get_trust`, `tdg_adjust_trust` |
| **Health** | `tdg_health_check`, `tdg_system_health`, `tdg_graph_health`, `tdg_graph_stats` |
| **Schema** | `tdg_get_schema` |
| **Multi-agent** | `tdg_bank` |
| **Entities** | `tdg_entity` |
| **Maintenance** | `tdg_maintenance`, `tdg_enrich`, `tdg_self_manage`, `tdg_renormalize` |
| **Audit** | `tdg_audit` |
| **Persistence** | `tdg_save_mind_state`, `tdg_load_mind_state`, `tdg_get_project_context`, `tdg_set_project_context` |
| **Import/Export** | `tdg_export`, `tdg_import` |

### 3.8 The "Mind" Pipeline (what it actually does)

| Module | LOC | What it computes | Trigger |
|---|---|---|---|
| `consolidation_engine.rs` | 436 | Graph health snapshot + reflection + constraint analysis + edge-density insights | `tdg_consolidate` (on-demand) |
| `reflect_engine.rs` | 472 | Clusters observations by shared MENTIONS entities (≥3 shared → cluster ≥3 obs); creates `skill:reflect_<sha>` nodes with ENABLES edges back to sources | `tdg_reflect_run` |
| `terrain.rs` | 325 | Discovers skills connected to top-3 densest node types; generates "terrain context" | `tdg_context` |
| `diagnostic.rs` | 807 | Drive distributions, addiction/allergy/blind-spot flags, quadrant imbalance, persistence warnings (DEAD — receives empty histories), ghost nodes, escalation level | `tdg_context` → `generate_prompt()` |
| `feeling.rs` | 365 | First-person emotional statements from drive averages; energy_level bucketed by node count (0=exhausted, <5=low, <20=moderate, else high) | `tdg_context` |
| `pulse.rs` | 469 | Structural-gap detection per node type using `ClosureRule`s | `tdg_context` |
| `state.rs` | 386 | `MindStateManager` with JSON-file persistence; session_id, working_memory, trust_score, MindMetrics | MCP tools |
| `injector.rs` | 624 | Assembles full terrain-first prompt; writes `tdg-mind-snapshot.json` | `tdg_context` |
| `embedding.rs` | 644 | ONNX text→vector (Gemma 768d / MiniLM 384d), mean pool, L2 norm | `add_node`, `update_node`, `Enricher` |
| `data_loader.rs` | 180 | Loads JSON state files with fallback | `injector.rs` |
| `sections.rs` | 452 | Generates prompt sections: pulse, revenue urgency, sensory field, social terrain, wisdom signals | `injector.rs` |

### 3.9 The Hermés Adapter (`plugins/tdg/__init__.py`)

Python `MemoryProvider` plugin that wraps `tdg-rust serve` as a subprocess. Exposes 3 LLM-facing tools (`tdg_memory_search`, `tdg_memory_record`, `tdg_memory_status`).

**Anti-patterns:**
1. **One subprocess per call** — no stdio session reuse; every tool call re-spawns the binary, re-runs PRAGMAs, re-initialises state. ~3 process startups per session.
2. **`sync_turn` truncates** `user_message[:200] + assistant_response[:300]` — information loss.
3. **Heuristic skip logic** — skips turns where `user_len < 20 or asst_len < 30`. Arbitrary thresholds.
4. **`on_memory_write` mirrors writes as observations with `trigger_digestion: True`** — can flood the graph with low-signal observations.
5. **No retry/backoff** — single 30s MCP timeout fails permanently.
6. **No streaming** — captures all stdout then parses.

**Note:** The prior `AUDIT_REPORT.md` claimed the adapter explicitly disabled digestion (`trigger_digestion: False`). **This is STALE** — current code sets `trigger_digestion: True` in all three call sites. The audit was written against an older version.

---

## 4. Gap Analysis — Primitive by Primitive

This is the heart of the audit. For each TDG primitive, we ask: does tdg-rust embody it? If partial, where? If absent, what would it take?

### 4.1 Holon (whole/part) — **PARTIAL**

**Theory:** A holon is simultaneously a whole in itself and a part of a larger whole. Every holon runs the invariant dual-metabolic architecture. Fractal recursion: every element of the architecture is itself a holon.

**tdg-rust:** A `Node` is a typed row in SQLite. It has `parent_ids: Vec<String>` and `DECOMPOSES_TO` edges. There is no first-class Holon type, no part/whole invariant, no compositional algebra.

**Reference (HoloOS):** `Holon` is a thin `(path, meta)` wrapper. All fields are derived on demand from `_meta.yaml` via property accessors. The holon directory structure (per `required_anatomy.yaml`) enforces the part/whole nesting: `_sub/` for sub-holons, `parent` edge for the canonical parent.

**Gap:**
- No compositional algebra — can't ask "what is the whole that this holon is part of?" beyond one level of `parent_ids[0]`.
- `agents_path` only uses `parent_ids[0]` (`crud.rs:1509`) — multi-parent holons lose path information.
- No recursive nesting invariant — a `Node` cannot itself contain `Node`s.

**Fix cost:** Medium. Introduce a `Holon` newtype over `Node` with compositional methods. See Phase 1.

### 4.2 Lesser Cycle (M·P·C·E) — **ABSENT**

**Theory:** The trusted anchor. Two reservoirs (Matrix, Potentiator) + two currencies (Catalyst, Experience). Matrix processes Catalyst, stores Experience. Potentiator processes Experience, stores Catalyst. Open loop, non-equilibrium.

**tdg-rust:** No M·P·C·E operator. The only 4-cycle is the 4 drives (eros/agape/agency/communion), which is a different ontology. Drives are scalar vectors blended by ratio, not derived from a 4-phase metabolic cycle.

**Reference (HoloOS):** `_meta.yaml.archetypal_mind.lesser_cycle` block stores `M, P, C, E` magnitudes + 4 shadows. `state_machine.py` runs a 6-phase lesser cycle: `dormant → ingesting → processing-skewed|processing-integrated → integrating → quiescent → dormant`. Transitions are guarded and atomic.

**Gap:** The single biggest ontological absence. Without the lesser cycle, tdg-rust has no metabolism — it cannot consume, transform, or enact. It can only store and propagate.

**Fix cost:** High. This is the trusted anchor; it must be implemented faithfully. See Phase 2.

### 4.3 Greater Cycle (S·T·G·Ch) — **ABSENT**

**Theory:** Mirrored topology across identity-pattern ⇄ operating-environment. Significator (reservoir A, all stages), Great Way (reservoir B, all stages), Transformation (currency B→A), Choice (currency A→B). Discontinuous/ratcheting — fires when operating-environment push + latent-state pull jointly exceed Significator's threshold.

**tdg-rust:** No S·T·G·Ch macro-cycle. Closest analogue is the 8-stage developmental ladder (Survival→Harvest), which is evolutionary (within-octave ascent), not the greater-cycle operator.

**Reference (HoloOS):** `_meta.yaml.archetypal_mind.greater_cycle` block. `state_machine.py` runs a 9-phase greater cycle: `significator-forming → significator-stable → transformation-pre-crucible → transformation-crucible → transformation-reintegration → great-way-aligned|great-way-friction → choice-polarizing → choice-locked → significator-forming`. Precondition: `transformation-crucible` requires `crucible_intensity ∈ {moderate, acute}`; `choice-locked` requires `crystallization_ratio ≥ 0.7`.

**Gap:** Without the greater cycle, tdg-rust cannot model vertical ascent, phase transitions, or directional commitment. Stages advance by evidence accumulation, not by Transformation events.

**Fix cost:** High. Depends on Phase 2 (lesser cycle) and Phase 3 (attractor field). See Phase 4.

### 4.4 Contact Boundary — **ABSENT**

**Theory:** TWO contact boundaries (lesser: Matrix⇄Potentiator, greater: Significator⇄Great Way). The four Drives ARE the two contact boundaries. The contact-boundary (Transformation) is SHARED between both perspectives. This is where self meets other, where drives are *generated* (not just propagated), where novelty enters.

**tdg-rust:** No primitive for "where self meets other". Drives are scalar vectors propagated through edges via BFS — they are never *born* at a boundary. The graph is a pipe network for pre-existing drive values, not a field of boundary-generated tensions.

**Reference (HoloOS):** The contact boundary is implicit in the state machine — each phase transition occurs at the boundary. The `goldilocks_zone` block (`{contact_boundary_coupling_cc, coupling_threshold_min/max, pathology_state}`) explicitly models boundary coupling. `infer_state.py` derives `in_isolation / approaching_isolation / stable / approaching_confluence / in_confluence` from Cc vs thresholds.

**Gap:** This is why the "mind" pipeline feels like a dashboard: without boundary-generated drives, the system can report on its state but cannot *experience* its becoming.

**Fix cost:** High. Requires Phases 2 + 3. See Phase 2 (contact boundary as first-class primitive on every Holon).

### 4.5 Attractor Field A(H) = ⟨A_M, A_P, A_G, Γ⟩ — **ABSENT**

**Theory:** The unified operational object tying metabolism + typology + bonding together. 4 components: A_M (Matrix attractor), A_P (Potentiator attractor), A_G (Great-Way attractor), Γ (coupling tensor on 2-torus). Significator is implicit (time-integral). Polarity π predicts bonding. Choice flag χ disambiguates noble.

**tdg-rust:** No attractor-field tuple. Closest is `FlowDriveState { eros, agape, agency, communion }` — 4 drives, not 4 attractor components. And `StageEvidence::integration_score()` is the closest thing to G_z but is not named G_z or used as a health gate.

**Reference (HoloOS):** `attractor.py` defines `AttractorField` dataclass with `A_M, A_P, A_G: ReservoirAttractor` (each `{magnitude, sign, polarity}`), `Gamma: CouplingTensor` (Ag, Cm, Er, Agp, each clamped [0,1]), `pi`, `type_class`, `choice_flag`, `archetypal_loads: ArchetypalLoads` (8 floats M·P·C·E·S·T·G·Ch), `stability: StabilityFilter`. Computed from metabolic state, written to `_meta.yaml.attractor_field`, with provenance event on every write.

**Gap:** Without the attractor field, tdg-rust cannot compute resonance, cannot classify type, cannot predict bonding, cannot detect the sinkhole of indifference.

**Fix cost:** Medium. The formula is canonical; the data structures are clear. See Phase 3.

### 4.6 G_z (Integrative Efficiency) — **ABSENT**

**Theory:** `G_z = 100·(A_z/100 · C_z/100 · B_H · B_V)^(1/4)`. Geometric mean of 4 factors. Rewards balance. >70 optimal, <30 collapse.

**tdg-rust:** Not implemented. `StageEvidence::integration_score()` (`telearchy.rs:61`) is the closest: `0.3·child_completion + 0.4·evidence_density + 0.3·cross_quadrant`. Not called G_z, not framed as efficiency, not used as a health gate.

**Reference (HoloOS):** `health.py:compute_G_z(A_z, C_z, B_H, B_V)` — exact formula. Stored in `_meta.yaml.health.G_z`. Thresholds: >70 optimal, 30–70 sub-optimal, <30 collapse.

**Fix cost:** Low (formula is canonical). Depends on Phase 3 (attractor field for A_z, C_z inputs).

### 4.7 P_z (Transcendental Tension) — **ABSENT**

**Theory:** `P_z = 100·∇Ψ·cos(θ_alignment)`. ∇Ψ = |P−M|/(P+M+ε). Rewards commitment, not balance. Neutrality is the pathology. >50 optimal, <10 sinkhole.

**tdg-rust:** No tension metric. Closest is `DriveDiagnosis::TensionPair` (`flow.rs:150`) which fires when `positive_pole>5 AND negative_pole>5` — per-drive, not per-holon.

**Reference (HoloOS):** `health.py:compute_P_z(grad_psi, theta_alignment)`. θ=0 aligned, π/2 neutral (P_z=0 — sinkhole), π anti-aligned (clamped to 0), π/4 partial.

**Fix cost:** Low. Depends on Phase 3.

### 4.8 Resonance R(H1, H2) — **ABSENT**

**Theory:** R = register_complementarity · coupling_tensor_compatibility · great_way_intersection ∈ [0,1]. >0.7 strong bond, <0.3 weak.

**tdg-rust:** No resonance relation. `cosine_similarity` in `util/math.rs` is between embedding vectors only, not between holonic attractor fields.

**Reference (HoloOS):** `attractor.resonance(h1, h2)` — exact 3-factor formula. Transients don't resonate (R=0). Noble holons don't bond (R=0).

**Fix cost:** Low. Depends on Phase 3.

### 4.9 Status Ladder — **MISMATCHED**

**Theory:** `ai-draft → canonical-hypothesis → canonical → superseded`. All agent outputs start at `ai-draft`. Elevation above `ai-draft` is human-only. 5-Gate Validation gates the auto-pass from blocked → passed.

**tdg-rust:** `lifecycle_state` uses ad-hoc strings: `active, archived, emerging, declining, stale, classified, lifecycle_complete, completed`. The `source` field tracks provenance (`mcp_observe`, `reflect_engine`, `digestion_cascade`) but there is NO formal epistemic-status progression.

**Reference (HoloOS):** `synthesis_status` field on every artifact. `submit_synthesis()` hardcodes `synthesis_status: ai-draft` in the header (line 530 — a literal, not a parameter). `can_elevate()` checks `VALID_ELEVATIONS` map. `elevate()` MCP tool returns `note: "Human authorization required for all elevations"`.

**Gap:** Without the status ladder, tdg-rust cannot distinguish AI-fabricated content from human-validated content. An agent can write a `hypothesis` and it's treated the same as a human-confirmed `synthesis`.

**Fix cost:** Low. Add a `synthesis_status` column + enum. See Phase 1.

### 4.10 Type Class + Type ⊥ Stage — **ABSENT**

**Theory:** Type = the Significator's invariant bonding-disposition toward the Great Way. Determined by Derivator(d, k, V, π). Type⊥Stage — Stage is dynamic excitation, Type is invariant shape.

**tdg-rust:** No typological classifier. Node types are flat strings. `validation.rs:28-230` defines per-type `NodeContract`s (required/recommended/auto-wire-on-parent fields) but no donor/receiver/stoichiometric typing.

**Reference (HoloOS):** `attractor._classify_type()` produces type_class strings like `strong-donor-sto`, `weak-acceptor-sts`, `sharer`, `noble-graduated`, `transient`. Type codes T01–T51 in `type_codes.yaml`. Type validation protocol (T1/T2/T3 tests) in `4_Type_Validation_Protocol.md`.

**Fix cost:** Medium. Depends on Phase 3 (attractor field for π, polarity). See Phase 6.

### 4.11 5-Gate Validation — **ABSENT**

**Theory:** Grounding, Failure-mode, Joint validation, Cosmological scope, Provenance completeness. Every synthesis must pass.

**tdg-rust:** No validation gate. `validation.rs` only validates edge-creation patterns, not synthesis submissions.

**Reference (HoloOS):** `validation_gate.py` — 5 gates, blocking on failure. `validate_synthesis()` returns `ValidationReport` with `overall_status ∈ {blocked, passed, failed}` and `can_elevate_to` (always ≤ `ai-draft`).

**Fix cost:** Medium. See Phase 5.

### 4.12 Witnesses vs Sources — **ABSENT**

**Theory:** External correspondences corroborate the invariant; they are NEVER the source. Witnesses are holons; sources are evidence paths within each witness.

**tdg-rust:** No first-class witness concept. `people` is a node type, but there is no Witness-vs-Source epistemic distinction. Observations cite `source` strings.

**Reference (HoloOS):** `_provenance.yaml` sidecar for each synthesis has `witnesses[]` (each with `holon_id`, `scale`, `type_class`, `evidence_paths[]`, `grounding_status`) and `derivation.pattern`. Witnesses are holons; evidence_paths are filesystem paths.

**Fix cost:** Low (data model only). See Phase 5.

### 4.13 Scale Codes — **ABSENT**

**Theory:** `S(Ω, δ, λ) = (T(λ), Σ(Ω, δ, λ))`. S00–S80 organisational scale codes (Cosmic, Galactic, Stellar, Planetary, Biospheric, …, Civilizational_Bloc, Civilization, …, Individual, Sub_Individual, Artifactual, Phenomenal, Conceptual, Linguistic). Plus Tetra-Axes UL/UR/LL/LR each 1–19 = 130,321 scale codes.

**tdg-rust:** No scale codes. `telos_level` (T0–T6) is the only vertical axis and it's developmental, not scalar.

**Reference (HoloOS):** `scale_codes.yaml` defines both organisational S-codes and Tetra-Axes coordinates. Holon ID format: `H.{scale_code}.{type_code}.{slug}` (e.g. `H.S11.T01.india`).

**Fix cost:** Medium. Add a `scale_code` column + taxonomy. See Phase 1.

### 4.14 Tetra-Axes (4-axis coordinate system) — **ABSENT**

**Theory:** 4 co-arising axes × 19 levels = 130,321 scale codes. UL (Interior×Individual), UR (Exterior×Individual), LL (Interior×Collective), LR (Exterior×Collective). Every level serves simultaneously as inter-holonic hierarchy AND intra-holonic fractal template.

**tdg-rust:** The 4 quadrants (UL/UR/LL/LR) are present but as a SINGLE label per node (`quadrants_json["primary"]`), not as a 4-axis coordinate vector.

**Reference (HoloOS):** `tetra_coordinates` block in `_meta.yaml` with `{UL, UR, LL, LR, quadrant}`. Each axis 1–19 with VIBGYOR ray coloring.

**Fix cost:** Low. Extend `quadrants_json` to a 4-axis vector. See Phase 1.

### 4.15 8-Role Load Vector (M·P·C·E·S·T·G·Ch) — **ABSENT**

**Theory:** Instantaneous read-out of the attractor field. 8 floats, each ∈ [0,1]. Two loops: A (M→C→P→E→M, continuous), B (S→T→G→Ch→S, discontinuous).

**tdg-rust:** `FlowDriveState::net_vector()` returns 4 floats `[eros, agape, agency, communion]`, not 8.

**Reference (HoloOS):** `ArchetypalLoads` dataclass with M, P, C, E, S, T, G, Ch fields.

**Fix cost:** Low. Depends on Phases 2 + 4 (lesser + greater cycles). See Phase 3.

### 4.16 22 Named Archetypes — **ABSENT**

**Theory:** 7 functional roles × 3 complexes (Mind, Body, Spirit) + Choice = 22. The 8 functional roles (M·P·C·E·S·T·G·Ch) are the operators; the 22 named archetypes are the operands.

**tdg-rust:** No archetype library.

**Reference (HoloOS):** `03.2_22_Named_Archetypes_Index.md` (ai-draft). `archetype.py` (lib).

**Fix cost:** Low (data model only). Depends on Phase 6 (type system).

### 4.17 Involution Sequence — **ABSENT**

**Theory:** Involution = the chain of 4 previous octaves (NOT realm-descent). Causal/subtle/gross realms are the synchronic dimensional architecture of a single octave; involution is the cross-octave diachronic chain. The 3rd density is the culmination of involution.

**tdg-rust:** No involution model. Closest is `TelosLevel` (T6→T0 promotion), which is *evolutionary*, not involutionary.

**Reference (HoloOS):** `octave_id` field in `_meta.yaml`. Doc 06.1 (canonical-hypothesis) — 3-4 prior octaves, J-INV-1 through J-INV-6 are open joints.

**Fix cost:** Low (data model only). Distinguish `octave_lineage_position` (cross-octave) from `octave_id` + realm structure (within-octave). See Phase 1.

### 4.18 Phase-Transition / Thermodynamic Model — **PARTIAL**

**Theory:** 4 pillars — Prigogine (dissipative structures), Chaisson (energy rate density Φ_m), Kauffman (autocatalytic sets, n·p > 1), Landauer (kT·ln(2) per bit). Phase transition occurs when all 4 conditions are simultaneously met. Bifurcation sequence: build-up → critical slowing down → symmetry breaking → crystallisation → scaling.

**tdg-rust:** `compute_graph_entropy` (`flow.rs:974`) computes Shannon entropy of drive-value distributions (5-bin histogram, normalised to log2(5)), with health thresholds (good > 0.8, warning > 0.5, critical_dilution otherwise). Passive diagnostic, not an active phase-transition operator.

**Reference (HoloOS):** `_THEORY/09_Thermodynamic_Framework/01_Phase_Transition_Model_Synthesis.md` (ai-draft). Not yet implemented in the Python reference.

**Fix cost:** Medium. See Phase 4 (greater cycle + phase transitions).

### 4.19 ContextPack — **PARTIAL**

**Theory:** Single-call structured object aggregating intra/inter/extra context. Token-budget truncation drops cheapest-to-lose first but NEVER drops synthesis_status, grounding, or type_class.

**tdg-rust:** `tdg_context` MCP tool exists and generates a full terrain-first prompt via `injector::generate_prompt()`. But the output is a markdown string, not a structured ContextPack object. No scope parameter, no depth control, no truncation order, no epistemic spine protection.

**Reference (HoloOS):** `agent_api.fetch_context(holon_id, scope, depth, include_analogues, token_budget) → ContextPack`. The capstone of the agent API.

**Fix cost:** Medium. See Phase 5.

### 4.20 Provenance — **EMBODIED**

**Theory:** Every write to a semantic field MUST emit a provenance event. Contaminated sources are tagged, not silently patched. Superseded docs become tombstones.

**tdg-rust:** Every node has `source: String`, every mutation log row has `agent_id`, every event has `agent_id`. `mutation_log` table provides structured time-travel audit trail. `record_mutation()` is best-effort (failures logged, never propagated) — under heavy load, audit trails can have gaps.

**Reference (HoloOS):** `kb.append_event()` on every semantic write. Atomic POSIX line-write. `_kb_events.jsonl` per holon.

**Gap:** tdg-rust is solid here, but `record_mutation` should be made reliable (not best-effort). And there's no `witnesses[]` structure (see 4.12).

### 4.21 Stage Codes — **EMBODIED**

**Theory:** 8 stages with evidence thresholds and age gates. Stage ≡ Density (canonical equivalence). Octave-closure: 8th = 1st of next.

**tdg-rust:** 8 stages (Survival→Harvest) with evidence thresholds (0/5/15/30/50/80/120/200) and age gates (0/0/3/7/14/21/45/90 days). `advance_stage()` wired into SelfManager cycle. This is well-designed.

**Gap:** Stage ≡ Density equivalence not modelled. Octave-closure (8th = 1st of next) not modelled. See Phase 4.

---

## 5. Engineering Hygiene Issues (Open Bugs & Tech Debt)

These must be fixed BEFORE the refactor begins — otherwise the codebase is unsafe to extend.

### 5.1 Stale Audit Documentation — **PRIORITY: HIGH**

**Location:** `AUDIT_REPORT.md` (July 2026), `upgrade-plan.md` (commit d41c5ad)

**Problem:** `AUDIT_REPORT.md` describes 5 "critical problems" that are largely FIXED in the current codebase:
- "Adapter explicitly disables digestion" — **STALE**. Current `plugins/tdg/__init__.py:326, 379, 407` all set `trigger_digestion: True`.
- "Quadrant data stored in wrong column" — **FIXED**. `tdg_observe` (line 1512-1515) writes to BOTH `quadrants_json["primary"]` AND `properties_json["quadrant"]`; `tdg_mind_state` reads `quadrants_json` first with fallback.
- "`enrich` action missing from `tdg_maintenance`" — **FIXED**. `mcp/tools.rs:1825-1833` handles `"enrich"` and `"align_data"`; standalone `tdg_enrich` tool exists (line 1960).
- "Enricher never reachable from MCP" — **FIXED**. See above.
- "Embeddings never created" — **PARTIALLY FIXED**. `add_node`/`update_node` inline-embed when `onnx` feature is enabled.

**Impact:** New contributors read the audit, believe the system is broken, and either (a) attempt to fix already-fixed bugs, or (b) lose trust in the codebase.

**Fix:** Move `AUDIT_REPORT.md` and `upgrade-plan.md` to `docs/archive/`. Replace with a single `docs/CURRENT-STATE.md` that accurately describes the v0.5.0 state.

### 5.2 God Module: `src/mcp/tools.rs` — **PRIORITY: HIGH**

**Problem:** 3,464 LOC in a single file (was 2,650 in the prior audit — it got *worse*). Every MCP tool implementation lives here. Large changes are risky; bugs recur after partial modularisation.

**Fix:** Split by domain:
- `src/mcp/tools/search.rs` — `tdg_search`, `tdg_prefetch`
- `src/mcp/tools/crud.rs` — `tdg_create`, `tdg_update`, `tdg_get_node`, `tdg_bulk_create`, `tdg_record_exec`
- `src/mcp/tools/observe.rs` — `tdg_observe` (the primary write path; deserves its own file)
- `src/mcp/tools/edges.rs` — `tdg_connect`, `tdg_get_related`
- `src/mcp/tools/maintenance.rs` — `tdg_maintenance`, `tdg_enrich`, `tdg_self_manage`, `tdg_renormalize`
- `src/mcp/tools/mind.rs` — `tdg_mind_state`, `tdg_context`
- `src/mcp/tools/reflect.rs` — `tdg_reflect`, `tdg_reflect_run`
- `src/mcp/tools/audit.rs` — `tdg_audit`, `tdg_graph_health`, `tdg_system_health`, `tdg_graph_stats`
- `src/mcp/tools/trust.rs` — `tdg_get_trust`, `tdg_adjust_trust`, `tdg_health_check`
- `src/mcp/tools/schema.rs` — `tdg_get_schema`, `tdg_bank`, `tdg_entity`
- `src/mcp/tools/persistence.rs` — `tdg_save_mind_state`, `tdg_load_mind_state`, `tdg_get_project_context`, `tdg_set_project_context`
- `src/mcp/tools/io.rs` — `tdg_export`, `tdg_import`
- `src/mcp/tools/events.rs` — `tdg_query_events`, `tdg_rate_memory`

Use a `mod.rs` that re-exports. Public MCP response shapes MUST stay stable.

### 5.3 Dead Diagnostic Engine Histories — **PRIORITY: HIGH**

**Location:** `src/mind/injector.rs:118`

**Problem:** `diag_engine.analyze(conn, &[], &[])` — both `drive_history` and `quadrant_history` are always passed as empty arrays. The persistence-warning, quadrant-repetition, and stuck-pattern features of the diagnostic engine are dead code in production.

**Impact:** The diagnostic engine produces reports without the temporal dimension that gives them meaning. "Drive persistence" warnings never fire. "Quadrant imbalance over 4 cycles" never fires.

**Fix:**
1. Persist `drive_history` and `quadrant_history` per holon — add `drive_history_json` and `quadrant_history_json` columns to `nodes` (or a new `holon_history` table).
2. Update `injector.rs:118` to load histories from the DB and pass them through.
3. Add a TTL/rotation policy (keep last N cycles, e.g. 20).

### 5.4 Dual `DualPoleDrive` Structs — **PRIORITY: MEDIUM**

**Location:** `src/models.rs:188` (2 fields: `positive, negative`) vs `src/flow.rs:111` (4 fields: `positive_pole, negative_pole, availability, blind_spot`)

**Problem:** The `models.rs` version is unused. The `flow.rs` version is canonical. The 2-field struct is dead code that misleads.

**Fix:** Delete `DualPoleDrive` from `models.rs`. Move the canonical 4-field struct to `models.rs` and re-import in `flow.rs`. Single source of truth.

### 5.5 `DriveVector` Claims "16 Drive Dimensions" — **PRIORITY: LOW**

**Location:** `src/models.rs:194`

**Problem:** Comment says "16 drive dimensions" but `FlowDriveState` has 4. Aspirational comment from the Python prototype, never reconciled.

**Fix:** Delete the comment or update to "4 drive dimensions: eros, agape, agency, communion".

### 5.6 MindStateManager "Dual Persistence" Claim — **PRIORITY: LOW**

**Location:** `src/state.rs:90`

**Problem:** Docstring claims "dual persistence (JSON + SQLite WAL)" but only JSON is implemented. WAL is "future: eventsourcing".

**Fix:** Either implement SQLite WAL persistence (write `MindState` to a `mind_state` table on every save) or remove the claim from the docstring.

### 5.7 `agents_path` Only Uses `parent_ids[0]` — **PRIORITY: MEDIUM**

**Location:** `src/db/crud.rs:1509-1529` `compute_agent_path`

**Problem:** Multi-parent holons lose path information for parents 2..N.

**Fix:** Either (a) store a full path tree, or (b) document that `agent_path` is the canonical-parent path only (and rename to `canonical_path`).

### 5.8 Hardcoded Agent Name "Sisyphus" — **PRIORITY: LOW**

**Location:** `src/mind/state.rs:49`

**Fix:** Read from config (`config.agent_name`), default to `"tdg-agent"`.

### 5.9 MCP Doc Drift — **PRIORITY: LOW**

**Location:** `src/mcp/mod.rs:11, 28`

**Problem:** Doc-comment table lists 17 tools; there are actually 36.

**Fix:** Update the doc-comment to list all 36 tools, or remove the count and reference `tools.rs` directly.

### 5.10 `record_mutation` Best-Effort — **PRIORITY: MEDIUM**

**Location:** `src/db/crud.rs:98-117`

**Problem:** `record_mutation` failures are logged via `tracing::warn!` but never propagated. Under heavy load, audit trails can have gaps.

**Fix:** Make `record_mutation` return a `TdgResult<()>` and propagate. Add a circuit-breaker trip if mutation logging fails N times in a row (the audit trail is critical for holonic science compliance).

---

## 6. The Refactor Plan

Six phases. Each phase is independently shippable. Each phase delivers immediate value. Phases 0–1 are hygiene + scaffolding; Phases 2–4 are the operatorial core; Phases 5–6 are the agent API + typology.

### Phase 0: Hygiene (1 week)

**Goal:** Make the codebase safe to extend. No new features.

**Tasks:**
1. **Archive stale audits** — move `AUDIT_REPORT.md` and `upgrade-plan.md` to `docs/archive/`. Create `docs/CURRENT-STATE.md` describing v0.5.0 accurately.
2. **Split `src/mcp/tools.rs`** — by domain (see §5.2). Public MCP response shapes MUST stay stable. Add a regression test that snapshots every tool's response shape.
3. **Fix dead diagnostic histories** — add `drive_history_json` and `quadrant_history_json` columns to `nodes`; update `injector.rs:118` to load and pass histories; add TTL/rotation (keep last 20 cycles).
4. **Delete dead `DualPoleDrive`** from `models.rs`. Move canonical 4-field struct to `models.rs`.
5. **Fix `DriveVector` comment** — update to "4 drive dimensions".
6. **Fix MindStateManager docstring** — either implement WAL persistence or remove the claim.
7. **Make `record_mutation` reliable** — propagate errors; trip circuit breaker on N consecutive failures.
8. **Parameterise agent name** — read from config.

**Success criteria:**
- `cargo test` passes with no new warnings.
- `src/mcp/tools.rs` is < 500 LOC (just the `mod.rs` re-exports).
- Diagnostic engine reports include persistence warnings when drive patterns repeat.
- `mutation_log` has zero gaps under load testing.

### Phase 1: Holon Primitive + Status Ladder + Scale Codes (2 weeks)

**Goal:** Introduce the scaffolding for holonic computation. No metabolism yet — just the data structures.

**Tasks:**

#### 1.1 Introduce `Holon` newtype over `Node`

```rust
// src/holon.rs (new file)
pub struct Holon {
    node: Node,
    // Cached computed fields
    attractor_field: Option<AttractorField>,
    health: Option<Health>,
    lesser_cycle_state: Option<LesserCycleState>,
    greater_cycle_state: Option<GreaterCycleState>,
}

impl Holon {
    pub fn from_node(conn: &Connection, node: Node) -> TdgResult<Self>;
    pub fn canonical_parent(&self) -> Option<String> { self.node.parent_ids.first().cloned() }
    pub fn all_parents(&self) -> &[String] { &self.node.parent_ids }
    pub fn children(&self, conn: &Connection) -> TdgResult<Vec<Holon>>;
    pub fn sub_holons(&self, conn: &Connection) -> TdgResult<Vec<Holon>>; // via DECOMPOSES_TO
    pub fn is_whole(&self) -> bool { !self.node.parent_ids.is_empty() }
    pub fn is_part(&self) -> bool { /* has children */ }
    
    // Compositional algebra
    pub fn compose(parent: &Holon, child: &Holon) -> TdgResult<Edge>; // DECOMPOSES_TO
    pub fn decompose(holon: &Holon) -> TdgResult<Vec<Holon>>; // returns sub-holons
}
```

**Migration:** `Holon::from_node` is a zero-cost wrapper. Existing `Node` API stays. New code uses `Holon`; old code migrates incrementally.

#### 1.2 Add `synthesis_status` column + enum

```sql
-- Migration Phase 8
ALTER TABLE nodes ADD COLUMN synthesis_status TEXT DEFAULT 'ai-draft';
-- Values: 'ai-draft', 'canonical-hypothesis', 'canonical', 'superseded'
```

```rust
// src/models.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SynthesisStatus {
    AiDraft,              // all agent outputs start here
    CanonicalHypothesis,  // derived from anchor, joints unvalidated
    Canonical,            // derived, red-teamed, joints validated (human-only elevation)
    Superseded,           // retired tombstone
}

impl SynthesisStatus {
    pub fn can_elevate_to(&self, target: &Self) -> bool {
        matches!((self, target),
            (AiDraft, CanonicalHypothesis) |
            (CanonicalHypothesis, Canonical) |
            (Canonical, Superseded) |
            (AiDraft, Superseded) |
            (CanonicalHypothesis, Superseded))
    }
}
```

**Enforcement:** `tdg_create` and `tdg_observe` set `synthesis_status = AiDraft` by default. Add a `tdg_elevate` MCP tool that checks `can_elevate_to` and requires a `human_authorization` parameter (just a string token for now; real auth in Phase 5).

#### 1.3 Add `scale_code` + Tetra-Axes coordinates

```sql
-- Migration Phase 9
ALTER TABLE nodes ADD COLUMN scale_code TEXT;           -- e.g. "S11" (Civilization)
ALTER TABLE nodes ADD COLUMN tetra_ul INTEGER;          -- 1-19
ALTER TABLE nodes ADD COLUMN tetra_ur INTEGER;          -- 1-19
ALTER TABLE nodes ADD COLUMN tetra_ll INTEGER;          -- 1-19
ALTER TABLE nodes ADD COLUMN tetra_lr INTEGER;          -- 1-19
ALTER TABLE nodes ADD COLUMN octave_id TEXT;            -- "N", "N-1", "N+1", etc.
ALTER TABLE nodes ADD COLUMN octave_lineage_position TEXT; -- cross-octave involution position
```

```rust
// src/scale_codes.rs (new file)
pub const SCALE_CODES: &[(&str, &str)] = &[
    ("S00", "Cosmic"), ("S01", "Galactic"), ("S02", "Stellar"), 
    ("S03", "Planetary"), ("S04", "Biospheric"),
    ("S10", "Civilizational_Bloc"), ("S11", "Civilization"),
    ("S20", "Sub_Civilizational"), ("S30", "Organizational"),
    ("S40", "Individual"), ("S41", "Sub_Individual"),
    ("S50", "Artifactual"), ("S60", "Phenomenal"),
    ("S70", "Conceptual"), ("S80", "Linguistic"),
];

pub fn is_valid_scale(code: &str) -> bool { SCALE_CODES.iter().any(|(c, _)| *c == code) }
```

**Backfill:** Existing nodes get `scale_code = NULL` (unknown). A migration script infers scale from node_type (e.g. `people` → S40, `project` → S30, `observation` → S40).

#### 1.4 Update `tdg_create` and `tdg_observe` to accept scale_code, tetra_coordinates, synthesis_status

Add optional parameters. Default `synthesis_status = AiDraft`. Default `scale_code` inferred from `node_type`.

**Success criteria:**
- `Holon` newtype compiles and passes tests.
- `synthesis_status` column exists; `tdg_elevate` tool works; AI-created nodes are always `AiDraft`.
- `scale_code` and Tetra-Axes columns exist; can be queried.
- Existing tests pass without modification (backward compatible).

### Phase 2: The Lesser Cycle (M·P·C·E) — The Trusted Anchor (3 weeks)

**Goal:** Implement the lesser cycle as a heartbeat-driven operator on every Holon. This is the non-negotiable — without it, tdg-rust is not a TDG implementation.

**Reference:** `_THEORY/02_Ontology/02.1_Microcosmic_Metabolic_Architecture.md` (canonical), `HoloOS/_INSTRUMENTS/lib/state_machine.py`.

#### 2.1 The Lesser Cycle State

```rust
// src/holon/lesser_cycle.rs (new file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LesserCycleState {
    pub phase: LesserPhase,
    pub matrix: ReservoirState,       // M
    pub potentiator: ReservoirState,  // P
    pub catalyst_pending: f64,        // C — incoming pressure not yet processed
    pub experience_accumulated: f64,  // E — processed input stored
    pub last_transition_at: String,   // RFC3339
    pub transition_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LesserPhase {
    Dormant,
    Ingesting,
    ProcessingSkewed,
    ProcessingIntegrated,
    Integrating,
    Quiescent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservoirState {
    pub magnitude: f64,    // [0, 1] — basin depth
    pub sign: i8,          // -1, 0, +1 (acceptor/balanced/donor)
    pub eta: f64,          // η_M or η_P — boundary resistance / conductance
    pub shadow: Option<Shadow>, // addiction/allergy diagnosis
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Shadow {
    DarkAddiction,      // Matrix hyper-ingestion
    DarkAllergy,        // Matrix hypo-ingestion
    GoldenAddiction,    // Potentiator hyper-ingestion
    GoldenAllergy,      // Potentiator hypo-ingestion
}
```

**Storage:** Add `lesser_cycle_json TEXT` column to `nodes`. Serialise `LesserCycleState` as JSON.

#### 2.2 The Lesser Cycle Operator

```rust
// src/holon/leser_cycle.rs
impl LesserCycleState {
    /// The 4-step loop: receive → process → store → generate → send.
    /// Matrix processes Catalyst, stores Experience.
    /// Potentiator processes Experience, stores Catalyst.
    pub fn tick(&mut self, incoming_catalyst: f64, dt: Duration) -> TdgResult<Vec<Event>> {
        let mut events = Vec::new();
        
        // Phase transition logic (guarded, per HoloOS state_machine.py)
        let next = self.next_phase();
        if self.can_transition_to(&next)? {
            self.transition_to(next)?;
            events.push(Event::phase_transition("lesser", &self.phase));
        }
        
        // The metabolic step
        match self.phase {
            LesserPhase::Ingesting => {
                // Matrix receives Catalyst
                self.catalyst_pending += incoming_catalyst;
                if self.catalyst_pending > self.matrix.eta * THRESHOLD {
                    self.transition_to(LesserPhase::ProcessingSkewed)?;
                }
            }
            LesserPhase::ProcessingSkewed | LesserPhase::ProcessingIntegrated => {
                // Matrix processes Catalyst → stores Experience
                let processed = self.catalyst_pending.min(self.matrix.eta * dt.as_secs_f64());
                self.catalyst_pending -= processed;
                self.experience_accumulated += processed * self.matrix.magnitude;
                
                // Potentiator processes Experience → stores Catalyst (latent)
                let latent = self.experience_accumulated * self.potentiator.eta;
                self.experience_accumulated -= latent;
                // latent goes to Potentiator's stored Catalyst (pressurising ascent)
                
                if self.catalyst_pending < THRESHOLD * 0.1 {
                    self.transition_to(LesserPhase::Integrating)?;
                }
            }
            LesserPhase::Integrating => {
                // Shadow surfacing — diagnose addiction/allergy
                self.diagnose_shadows();
                self.transition_to(LesserPhase::Quiescent)?;
            }
            LesserPhase::Quiescent => {
                // Reset for next cycle
                self.transition_to(LesserPhase::Dormant)?;
            }
            LesserPhase::Dormant => {
                if incoming_catalyst > THRESHOLD {
                    self.transition_to(LesserPhase::Ingesting)?;
                }
            }
        }
        
        Ok(events)
    }
    
    fn diagnose_shadows(&mut self) {
        // Dark-Addiction: Matrix hyper-ingestion (catalyst_pending consistently high)
        // Dark-Allergy: Matrix hypo-ingestion (catalyst_pending consistently low)
        // Golden-Addiction: Potentiator hyper-ingestion (experience_accumulated flooding)
        // Golden-Allergy: Potentiator hypo-ingestion (experience_accumulated starving)
        // ... thresholds from config/diagnostic_thresholds.yaml
    }
}
```

#### 2.3 The Heartbeat

```rust
// src/heartbeat.rs (new file)
pub struct Heartbeat {
    interval: Duration,  // default 60s, configurable
    ticker: tokio::time::Interval,
}

impl Heartbeat {
    pub async fn run(self, pool: DbPool) {
        loop {
            self.ticker.tick().await;
            let conn = get_conn(&pool).unwrap();
            
            // Tick the lesser cycle on every active holon
            for holon in Holon::all_active(&conn).unwrap() {
                let incoming_catalyst = holon.incoming_catalyst(&conn).unwrap_or(0.0);
                let mut state = holon.lesser_cycle_state(&conn).unwrap_or_default();
                let events = state.tick(incoming_catalyst, self.interval).unwrap();
                holon.save_lesser_cycle_state(&conn, &state).unwrap();
                
                // Emit events
                for event in events {
                    record_event(&conn, &event).unwrap();
                }
            }
        }
    }
}
```

**Integration:** Spawn the heartbeat from `main.rs` alongside the existing SelfManager scheduler. Make it configurable (`TDG_HEARTBEAT_INTERVAL_SECS`, default 60).

#### 2.4 Catalyst Generation at Contact Boundaries

This is the key insight — drives must be *generated* at boundaries, not just propagated.

```rust
// src/holon/contact_boundary.rs (new file)
pub struct ContactBoundary {
    pub holon_id: String,
    pub boundary_type: BoundaryType, // Lesser (Matrix⇄Potentiator) or Greater (Significator⇄Great Way)
}

impl ContactBoundary {
    /// When two holons interact (via an edge), generate catalyst at the boundary.
    /// This is where novelty enters the system.
    pub fn generate_catalyst(&self, conn: &Connection, other: &Holon, edge_type: &str) -> f64 {
        // Catalyst magnitude = f(edge_type, drive_complementarity, resonance)
        let edge_weight = edge_type_weight(edge_type);
        let drive_complementarity = self.drive_complementarity(other);
        let resonance = self.resonance(other); // Phase 3
        
        edge_weight * drive_complementarity * resonance
    }
}
```

**Integration with `tdg_connect`:** When an edge is created, fire `ContactBoundary::generate_catalyst` on both endpoints. The catalyst feeds into the next heartbeat tick of the lesser cycle.

**Success criteria:**
- Every active holon has a `LesserCycleState` that ticks on the heartbeat.
- Phase transitions are logged as events.
- Shadows are diagnosed and stored.
- Catalyst is generated at edges and feeds into the lesser cycle.
- The graph *metabolises* — observations are consumed, experiences accumulated, ascent pressurised.

### Phase 3: Attractor Field + G_z + P_z + Resonance (2 weeks)

**Goal:** Implement the operational object that lets us compute health, classify type, and predict bonding.

**Reference:** `_THEORY/02_Ontology/08.1_Attractor_Field_Model.md` (canonical-hypothesis), `HoloOS/_INSTRUMENTS/lib/attractor.py`, `HoloOS/_INSTRUMENTS/lib/health.py`.

#### 3.1 The Attractor Field

```rust
// src/holon/attractor_field.rs (new file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttractorField {
    pub a_m: ReservoirAttractor,  // Matrix attractor
    pub a_p: ReservoirAttractor,  // Potentiator attractor
    pub a_g: ReservoirAttractor,  // Great-Way attractor
    pub gamma: CouplingTensor,    // Γ — on 2-torus
    pub pi: Option<f64>,          // polarity disposition [-1, +1], None = noble
    pub type_class: String,       // "strong-donor-sto", "sharer", "noble-graduated", etc.
    pub choice_flag: Option<ChoiceFlag>,
    pub archetypal_loads: ArchetypalLoads, // 8 floats M·P·C·E·S·T·G·Ch
    pub stability: StabilityFilter,
    pub computed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservoirAttractor {
    pub magnitude: f64,  // [0, 1]
    pub sign: i8,        // -1, 0, +1
    pub polarity: Option<String>, // only A_G: "STO" / "STS" / "neutral"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouplingTensor {
    pub ag: f64,   // Agency [0, 1]
    pub cm: f64,   // Communion [0, 1]
    pub er: f64,   // Eros [0, 1]
    pub agp: f64,  // Agape [0, 1]
}

impl CouplingTensor {
    /// Γ lives on a 2-torus: horizontal drives (Ag, Cm) anti-correlated;
    /// vertical drives (Er, Agp) anti-correlated.
    pub fn enforce_torus_constraints(&mut self) {
        // If Ag + Cm > 1.0, scale down proportionally
        // If Er + Agp > 1.0, scale down proportionally
        let h_sum = self.ag + self.cm;
        if h_sum > 1.0 { let scale = 1.0 / h_sum; self.ag *= scale; self.cm *= scale; }
        let v_sum = self.er + self.agp;
        if v_sum > 1.0 { let scale = 1.0 / v_sum; self.er *= scale; self.agp *= scale; }
    }
    
    pub fn horizontal_balance(&self) -> f64 { self.ag - self.cm + 0.5 }
    pub fn balance_multiplier(&self) -> f64 {
        let devs = [self.ag, self.cm, self.er, self.agp].iter()
            .map(|d| (d - 0.5).abs()).collect::<Vec<_>>();
        1.0 - (devs.iter().sum::<f64>() / 4.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchetypalLoads {
    pub m: f64, pub p: f64, pub c: f64, pub e: f64,  // Loop A
    pub s: f64, pub t: f64, pub g: f64, pub ch: f64, // Loop B
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityFilter {
    pub self_consistent: bool,  // ℓ_T < θ_T (0.7) AND π stable
    pub bondable: bool,         // A_G.magnitude > 0.1
    pub persistent: bool,       // |C − E| < 0.5
}

impl StabilityFilter {
    pub fn is_stable_type(&self) -> bool {
        self.self_consistent && self.bondable && self.persistent
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChoiceFlag { Graduated, Sinkhole, Reopened }
```

**Computation** (`AttractorField::compute(holon)`):
- **A_M** magnitude from lesser-cycle Matrix state; sign from shadow diagnosis (addiction → +1 donor, allergy → −1 acceptor).
- **A_P** magnitude from lesser-cycle Potentiator state; sign from golden shadow diagnosis.
- **A_G** magnitude from coupling_breadth (number of edges weighted by resonance); polarity from P_z sign.
- **Γ** read from `drives_json` (eros/agape/agency/communion positive_pole values), normalised to [0,1], torus-constrained.
- **π** = `(A_M.sign + A_P.sign + A_G_polarity_sign) / 3.0`. If all reservoirs balanced AND |P_z| < 0.05 → π = None (noble).
- **type_class** via `_classify_type()`: prefix from π, suffix from A_G polarity, "transient" if not stable, "noble-*" if π is None.

#### 3.2 Health Metrics

```rust
// src/holon/health.rs (new file)
pub struct Health {
    pub g_z: f64,  // [0, 100]
    pub p_z: f64,  // [0, 100]
    pub a_z: f64,  // [0, 100] — Matrix-side boundary resistance
    pub c_z: f64,  // [0, 100] — Potentiator-side field conductance
    pub b_h: f64,  // horizontal balance
    pub b_v: f64,  // vertical balance
    pub grad_psi: f64,  // ∇Ψ
    pub theta_alignment: f64, // radians
    pub total: f64, // G_z * P_z
    pub state: HealthState,
}

pub enum HealthState { Optimal, SubOptimal, Collapse, Sinkhole }

impl Health {
    pub fn compute(holon: &Holon, af: &AttractorField) -> Self {
        let m = holon.lesser_cycle.matrix.magnitude;
        let p = holon.lesser_cycle.potentiator.magnitude;
        let eta_m = holon.lesser_cycle.matrix.eta;
        let eta_p = holon.lesser_cycle.potentiator.eta;
        let c = holon.lesser_cycle.catalyst_pending;
        let e = holon.lesser_cycle.experience_accumulated;
        
        let omega_a = (m * eta_m) / (c.abs() + EPSILON);
        let a_z = if omega_a <= 0.0 { 0.0 } else { 100.0 * (-omega_a.ln().abs()).exp() };
        
        let sigma_c = (p * eta_p) / (e.abs() + EPSILON);
        let c_z = if sigma_c <= 0.0 { 0.0 } else { 100.0 * (-sigma_c.ln().abs()).exp() };
        
        let b_h = if a_z > c_z { c_z / a_z } else { a_z / c_z };
        let eros = af.gamma.er;
        let agape = af.gamma.agp;
        let b_v = if eros > agape { agape / eros } else { eros / agape };
        
        let product = (a_z / 100.0) * (c_z / 100.0) * b_h * b_v;
        let g_z = if product <= 0.0 { 0.0 } else { 100.0 * product.powf(0.25) };
        
        let grad_psi = (p - m).abs() / (p + m + EPSILON);
        let theta_alignment = compute_theta_alignment(holon, af);
        let p_z = 100.0 * grad_psi * theta_alignment.cos();
        let p_z = p_z.max(0.0).min(100.0);
        
        // ... state classification
        Self { g_z, p_z, a_z, c_z, b_h, b_v, grad_psi, theta_alignment, total: g_z * p_z, state }
    }
}
```

#### 3.3 Resonance

```rust
// src/holon/resonance.rs (new file)
pub fn resonance(h1: &AttractorField, h2: &AttractorField) -> f64 {
    if !h1.stability.is_stable_type() || !h2.stability.is_stable_type() {
        return 0.0; // transients don't resonate
    }
    
    // 1. Register complementarity
    let comp = match (h1.pi, h2.pi) {
        (None, _) | (_, None) => 0.0, // noble doesn't bond
        (Some(p1), Some(p2)) if (p1 > 0.0 && p2 < 0.0) || (p1 < 0.0 && p2 > 0.0) => {
            p1.abs().min(p2.abs()) // donor↔acceptor
        }
        (Some(p1), Some(p2)) if (p1 - p2).abs() < 0.2 => 1.0 - (p1 - p2).abs(), // sharer↔sharer
        _ => 0.3,
    };
    
    // 2. Coupling-tensor cosine similarity
    let g1 = (h1.gamma.ag, h1.gamma.cm, h1.gamma.er, h1.gamma.agp);
    let g2 = (h2.gamma.ag, h2.gamma.cm, h2.gamma.er, h2.gamma.agp);
    let dot = g1.0*g2.0 + g1.1*g2.1 + g1.2*g2.2 + g1.3*g2.3;
    let norm1 = (g1.0.powi(2) + g1.1.powi(2) + g1.2.powi(2) + g1.3.powi(2)).sqrt();
    let norm2 = (g2.0.powi(2) + g2.1.powi(2) + g2.2.powi(2) + g2.3.powi(2)).sqrt();
    let gamma_compat = if norm1 == 0.0 || norm2 == 0.0 { 0.0 } 
                       else { (dot / (norm1 * norm2)).max(0.0) };
    
    // 3. Great-Way intersection
    let gw = match (h1.a_g.polarity.as_deref(), h2.a_g.polarity.as_deref()) {
        (Some(p1), Some(p2)) if p1 == p2 => 1.0,
        (p1, p2) if p1 == Some("neutral") || p2 == Some("neutral") => 0.6,
        (Some("STO"), Some("STS")) | (Some("STS"), Some("STO")) => 0.2,
        _ => 0.3,
    };
    
    (comp * gamma_compat * gw * 10_000.0).round() / 10_000.0
}
```

#### 3.4 MCP Tools

- `tdg_attractor` — returns A(H) for a holon (cached or computed)
- `tdg_health` — returns G_z, P_z, total health, state classification
- `tdg_resonance` — takes two holon_ids, returns R(H1, H2)
- Update `tdg_audit` to include G_z/P_z distributions across the graph

**Success criteria:**
- Every holon has a computed `AttractorField` and `Health`.
- `tdg_health <id>` returns G_z, P_z, and state classification.
- `tdg_resonance <id1> <id2>` returns R ∈ [0, 1].
- The sinkhole-of-indifference (high G_z + low P_z) is detectable and flagged in `tdg_audit`.
- Attractor field is recomputed on every lesser-cycle tick (or on demand).

### Phase 4: The Greater Cycle + Phase Transitions (3 weeks)

**Goal:** Implement the vertical ascent operator. Stages advance by Transformation events, not just evidence accumulation.

**Reference:** `_THEORY/02_Ontology/02.2_Macrocosmic_Metabolic_Architecture.md` (canonical), `_THEORY/09_Thermodynamic_Framework/01_Phase_Transition_Model_Synthesis.md` (ai-draft), `HoloOS/_INSTRUMENTS/lib/state_machine.py`.

#### 4.1 The Greater Cycle State

```rust
// src/holon/greater_cycle.rs (new file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreaterCycleState {
    pub phase: GreaterPhase,
    pub significator: ReservoirState,  // S
    pub great_way: ReservoirState,     // G
    pub transformation_pressure: f64,  // T — accumulated
    pub choice_committed: f64,         // Ch — directional commitment
    pub crucible_intensity: CrucibleIntensity,
    pub crystallization_ratio: f64,    // [0, 1]
    pub last_transition_at: String,
    pub transition_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GreaterPhase {
    SignificatorForming,
    SignificatorStable,
    TransformationPreCrucible,
    TransformationCrucible,
    TransformationReintegration,
    GreatWayAligned,
    GreatWayFriction,
    ChoicePolarizing,
    ChoiceLocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CrucibleIntensity { Moderate, Acute, None }
```

**Preconditions** (from HoloOS `state_machine.py`):
- `TransformationCrucible` requires `crucible_intensity ∈ {Moderate, Acute}`
- `ChoiceLocked` requires `crystallization_ratio ≥ 0.7`

#### 4.2 The Greater Cycle Operator

The greater cycle is **discontinuous/ratcheting** — it fires when:
1. Great-Way push (environmental pressure) + Potentiator pull (latent potential) jointly exceed Significator's threshold
2. All 4 thermodynamic conditions are met (Prigogine + Chaisson + Kauffman + Landauer)

```rust
impl GreaterCycleState {
    pub fn tick(&mut self, lesser: &LesserCycleState, health: &Health, dt: Duration) -> TdgResult<Vec<Event>> {
        // Transformation pressure accumulates from lesser-cycle Experience
        self.transformation_pressure += lesser.experience_accumulated * TRANSFORMATION_RATE;
        
        // Check if threshold exceeded
        let threshold = self.significator.magnitude * SIGNIFICATOR_THRESHOLD;
        if self.transformation_pressure > threshold && self.phase == SignificatorStable {
            // Phase transition: critical slowing down → symmetry breaking
            self.crucible_intensity = if self.transformation_pressure > threshold * 2.0 {
                CrucibleIntensity::Acute
            } else {
                CrucibleIntensity::Moderate
            };
            self.transition_to(TransformationPreCrucible)?;
        }
        
        // ... full state machine logic per HoloOS state_machine.py
        
        // On ChoiceLocked: commit the choice, reset transformation_pressure,
        // advance the stage (this is where stage promotion happens via Transformation,
        // not just evidence accumulation)
        if self.phase == ChoiceLocked && self.can_advance_stage() {
            self.commit_choice()?;
            // Trigger stage advance via telearchy.advance_stage
        }
        
        Ok(events)
    }
}
```

#### 4.3 Phase Transition Detection (Thermodynamic Model)

```rust
// src/holon/phase_transition.rs (new file)
pub struct PhaseTransitionDetector {
    // 4 pillars
    prigogine: PrigogineMetric,    // distance from equilibrium
    chaisson: ChaissonMetric,      // energy rate density Φ_m
    kauffman: KauffmanMetric,      // n·p catalytic closure
    landauer: LandauerMetric,      // informational budget
}

impl PhaseTransitionDetector {
    pub fn readiness(&self, holon: &Holon) -> f64 {
        // R_total = w1·R_prigogine + w2·R_chaisson + w3·R_kauffman + w4·R_landauer
        // When R_total → 1.0, system is at bifurcation point
        let weights = [0.25, 0.25, 0.25, 0.25];
        let readiness = [
            self.prigogine.readiness(holon),
            self.chaisson.readiness(holon),
            self.kauffman.readiness(holon),
            self.landauer.readiness(holon),
        ];
        weights.iter().zip(readiness.iter()).map(|(w, r)| w * r).sum()
    }
    
    pub fn detect_bifurcation(&self, holon: &Holon) -> Option<BifurcationEvent> {
        if self.readiness(holon) > 0.8 {
            // Critical slowing down: anomalously long-lived fluctuations
            // Symmetry breaking: a fluctuation amplified to macroscopic scale
            Some(BifurcationEvent::new(holon.id.clone()))
        } else {
            None
        }
    }
}
```

**Integration with `telearchy.rs`:** Stage advancement now requires EITHER:
- (a) Evidence threshold + age gate (current logic), OR
- (b) A Transformation event from the greater cycle

Both paths valid; (b) is the holonic-science path.

**Success criteria:**
- Every holon has a `GreaterCycleState` that ticks on the heartbeat (slower than lesser cycle — e.g. every 5 minutes).
- Phase transitions are logged; `TransformationCrucible` and `ChoiceLocked` require preconditions.
- Stage advancement can occur via Transformation events, not just evidence accumulation.
- The bifurcation detector flags holons approaching phase transitions.
- `tdg_audit` includes greater-cycle phase distribution and bifurcation warnings.

### Phase 5: ContextPack + 5-Gate Validation (2 weeks)

**Goal:** Redesign the agent API around the ContextPack and 5-Gate Validation. This is what makes tdg-rust safe for AI agents to use without corrupting the canonical layer.

**Reference:** `HoloOS/_INSTRUMENTS/lib/agent_api.py`, `HoloOS/_INSTRUMENTS/lib/validation_gate.py`, `_THEORY/01_Epistemology/`.

#### 5.1 The ContextPack

```rust
// src/mcp/context_pack.rs (new file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPack {
    pub holon_id: String,
    pub scale_code: String,
    pub type_code: String,
    pub type_class: String,
    pub quadrant: String,
    pub synthesis_status: SynthesisStatus,
    
    pub intra: IntraContext,     // attractor_field, health, archetypal_loads, drives_and_shadows, stage
    pub inter: InterContext,     // bonds, bridges, top-5 resonances
    pub extra: ExtraContext,     // parent_chain, sub_holons, great_way
    pub analogues: Vec<Analogue>, // cross-domain type-homologues (max 10)
    pub provenance: ProvenanceSummary, // last 5 events, evidence_count, open_joints
    pub grounding: Grounding,    // anchor_docs, hypothesis_docs, epistemology_status
}

impl ContextPack {
    pub fn to_prompt_block(&self, token_budget: usize) -> String {
        // Render as markdown with [status: {status}] tags on every claim.
        // Truncation order (cheapest-to-lose first):
        //   analogues → provenance beyond top 3 → scale_neighbors →
        //   resonances beyond top 5 → archetypal_loads detail
        // NEVER drop: synthesis_status, grounding, or type_class
        todo!()
    }
}

// New MCP tool: tdg_fetch_context
// Replaces tdg_context (which returns an unstructured markdown prompt)
// tdg_fetch_context takes: holon_id, scope ("intra"|"inter"|"extra"|"intra+inter"|"intra+inter+extra"|"analogues"), depth (0-4), token_budget
// Returns: ContextPack (structured) or markdown prompt block
```

#### 5.2 The 5-Gate Validation

```rust
// src/validation_gate.rs (rewritten)
pub struct ValidationGate;

impl ValidationGate {
    pub fn validate(synthesis_id: &str, provenance: &Provenance, text: &str) -> ValidationReport {
        let gates = vec![
            Self::gate1_grounding(provenance),       // cites ≥1 anchor doc
            Self::gate2_failure_mode(provenance, text), // 5 QIM failure-modes + humanistic reduction
            Self::gate3_joint_validation(provenance),   // open joints labeled
            Self::gate4_cosmological_scope(provenance), // invariant claims cite ≥2 scales
            Self::gate5_provenance_completeness(provenance), // required fields present
        ];
        
        let blocked = gates.iter().any(|g| g.blocked);
        let overall = if blocked { "blocked" }
                      else if gates.iter().all(|g| g.passed) { "passed" }
                      else { "failed" };
        
        ValidationReport {
            synthesis_id: synthesis_id.to_string(),
            gates,
            overall_status: overall.to_string(),
            can_elevate_to: SynthesisStatus::AiDraft, // auto-pass only
            validated_at: now_iso(),
        }
    }
}
```

#### 5.3 The Synthesis Submission Flow

```rust
// New MCP tool: tdg_submit_synthesis
// 1. Write synthesis to a separate location (NOT the canonical layer)
// 2. Set synthesis_status = AiDraft (hardcoded, not a parameter)
// 3. Run 5-gate validation
// 4. Return validation report
// 5. Append provenance event

// New MCP tool: tdg_elevate (human-only)
// 1. Check can_elevate_to
// 2. If target == Canonical, require validation_report.overall_status == "passed"
// 3. Require human_authorization token
// 4. Update synthesis_status
// 5. Append provenance event
```

**Success criteria:**
- `tdg_fetch_context <id> --scope intra+inter+extra --depth 2` returns a structured ContextPack.
- AI-submitted syntheses ALWAYS land at `AiDraft` status (hardcoded).
- The 5-gate validation runs on every submission; blocked syntheses cannot be elevated.
- Elevation above `AiDraft` requires a human_authorization token.
- The ContextPack markdown render includes `[status: {status}]` tags on every claim.

### Phase 6: Type System + 22 Archetypes (2 weeks)

**Goal:** Implement the typological classifier. Holons get a `type_class` derived from their attractor field, validated by T1/T2/T3 tests.

**Reference:** `_THEORY/02_Ontology/02.3_Holonic_Typology_Derivator.md` (canonical), `_THEORY/02_Ontology/03.2_22_Named_Archetypes_Index.md` (ai-draft), `_THEORY/01_Epistemology/4_Type_Validation_Protocol.md`.

#### 6.1 Type Classification

```rust
// src/holon/type_system.rs (new file)
pub fn classify_type(af: &AttractorField) -> String {
    if !af.stability.is_stable_type() {
        return "transient".to_string();
    }
    
    if af.pi.is_none() {
        return match af.choice_flag {
            Some(ChoiceFlag::Graduated) => "noble-graduated",
            Some(ChoiceFlag::Sinkhole) => "noble-sinkhole",
            Some(ChoiceFlag::Reopened) => "noble-reopened",
            None => "noble-ambiguous",
        }.to_string();
    }
    
    let pi = af.pi.unwrap();
    let prefix = if pi > 0.5 { "strong-donor" }
                 else if pi > 0.1 { "weak-donor" }
                 else if pi < -0.5 { "strong-acceptor" }
                 else if pi < -0.1 { "weak-acceptor" }
                 else { "sharer" };
    
    let suffix = match af.a_g.polarity.as_deref() {
        Some("STO") => "-sto",
        Some("STS") => "-sts",
        _ => "",
    };
    
    format!("{}{}", prefix, suffix)
}
```

#### 6.2 Type Validation (T1/T2/T3)

```rust
// src/holon/type_validation.rs (new file)
pub struct TypeValidator;

impl TypeValidator {
    /// T1 — Behavioral match: does observed bonding match the signature's prediction?
    pub fn t1_behavioral_match(holon: &Holon, af: &AttractorField) -> bool {
        // Check actual edges against predicted bonding disposition
        // donor should have outgoing edges; acceptor should have incoming; etc.
        todo!()
    }
    
    /// T2 — Excitation-invariance: does 𝒱 stay fixed as Stage changes?
    pub fn t2_excitation_invariance(holon: &Holon) -> bool {
        // Check that type_class hasn't changed across stage transitions
        todo!()
    }
    
    /// T3 — Fixed-point persistence: does 𝒱 persist across metabolic cycles?
    pub fn t3_fixed_point_persistence(holon: &Holon) -> bool {
        // Check that type_class hasn't changed across lesser-cycle ticks
        todo!()
    }
    
    pub fn validate(holon: &Holon, af: &AttractorField) -> TypeValidationResult {
        let t1 = Self::t1_behavioral_match(holon, af);
        let t2 = Self::t2_excitation_invariance(holon);
        let t3 = Self::t3_fixed_point_persistence(holon);
        TypeValidationResult { t1, t2, t3, valid: t1 && t2 && t3 }
    }
}
```

#### 6.3 The 22 Named Archetypes

```rust
// src/holon/archetypes.rs (new file)
pub const ARCHETYPES: &[Archetype] = &[
    // Mind complex (7)
    Archetype::new(1, "Matrix of the Mind", Complex::Mind, Role::M),
    Archetype::new(2, "Potentiator of the Mind", Complex::Mind, Role::P),
    Archetype::new(3, "Catalyst of the Mind", Complex::Mind, Role::C),
    Archetype::new(4, "Experience of the Mind", Complex::Mind, Role::E),
    Archetype::new(5, "Significator of the Mind", Complex::Mind, Role::S),
    Archetype::new(6, "Transformation of the Mind", Complex::Mind, Role::T),
    Archetype::new(7, "Great Way of the Mind", Complex::Mind, Role::G),
    // Body complex (7) — analogous
    // Spirit complex (7) — analogous
    // Choice (1) — the meta-pivot
    Archetype::new(22, "Choice", Complex::Pivot, Role::Ch),
];
```

**Success criteria:**
- Every holon has a `type_class` derived from its attractor field.
- T1/T2/T3 validation runs on type assignment; failed validations flag the holon as "transient".
- The 22 archetypes are defined and queryable.
- Type⊥Stage orthogonality is enforced (T2 test catches violations).

---

## 7. Migration Safety

The refactor must not break the existing graph. Principles:

1. **All new columns are nullable with defaults.** Existing nodes work without modification.
2. **All new MCP tools are additive.** Existing tools keep their response shapes.
3. **The `Holon` newtype is zero-cost.** Existing `Node` API stays; new code uses `Holon`.
4. **Lesser/greater cycle states default to `Dormant` / `SignificatorForming`.** Existing nodes start with inert cycles and begin ticking on the next heartbeat.
5. **`synthesis_status` defaults to `AiDraft`.** Existing nodes are treated as AI-draft (the safe default).
6. **Scale codes are inferred from `node_type`** during migration (e.g. `people` → S40, `project` → S30, `observation` → S40).
7. **Stage advancement via evidence accumulation still works.** The greater-cycle Transformation path is ADDITIVE — stages can advance via either path.
8. **The existing `tdg_context` tool is deprecated, not removed.** It's replaced by `tdg_fetch_context` but kept for backward compat with the Hermés adapter.
9. **Each phase ships behind a feature flag** (`tdg-holonic-v1`, `tdg-holonic-v2`, etc.) so it can be enabled incrementally.

### Migration Order

```
Phase 0 (hygiene) → Phase 1 (scaffolding) → Phase 2 (lesser cycle) →
Phase 3 (attractor field) → Phase 4 (greater cycle) →
Phase 5 (agent API) → Phase 6 (type system)
```

Each phase is a minor version bump (v0.6.0, v0.7.0, …). Each phase has a migration script that runs on `tdg-rust migrate`.

---

## 8. Mind Pipeline Redesign — From Dashboard to Metabolism

The current mind pipeline (`src/mind/`) is a retrospective dashboard. It reports state but doesn't drive action. The refactor turns it into a metabolism.

### Current State (Dashboard)

| Module | What it does | What it should do |
|---|---|---|
| `consolidation_engine.rs` | Snapshot + reflection + insights (returned to caller, no mutation) | Run on heartbeat; close the loop by feeding insights back into lesser cycles |
| `reflect_engine.rs` | Clusters observations, creates skill nodes (the ONE module that mutates) | Already good; integrate with attractor field — skills should inherit type_class from observations |
| `terrain.rs` | Discovers skills by graph density | Add resonance-based discovery (find skills with high R to current holon) |
| `diagnostic.rs` | Drive distributions, addiction/allergy flags (HISTORIES DEAD) | Fix histories; feed diagnoses into lesser-cycle shadow diagnosis |
| `feeling.rs` | First-person statements from drive averages; energy by node count | Replace node-count bucket with thermodynamic energy function (G_z, P_z, Φ_m) |
| `pulse.rs` | Structural-gap detection per node type | Add attractor-field gap detection (holons with no resonance partners) |
| `state.rs` | MindStateManager JSON persistence | Add SQLite WAL persistence (the "dual persistence" claim) |
| `injector.rs` | Assembles full terrain-first prompt | Replace with ContextPack-based prompt assembly |

### Target State (Metabolism)

The mind pipeline becomes the **integrator** of the lesser cycle across the whole graph:

1. **Heartbeat** ticks every holon's lesser cycle (Phase 2).
2. **DiagnosticEngine** reads lesser-cycle states across the graph, diagnoses graph-level shadows (e.g. "the graph as a whole is in dark-addiction — too many observations, not enough integration").
3. **FeelingEngine** computes first-person statements from G_z, P_z, and attractor-field state (not from drive averages).
4. **PulseEngine** detects structural gaps in the attractor-field topology (holons with no resonance partners, sinkholes of indifference).
5. **ConsolidationEngine** closes the loop — feeds graph-level diagnoses back into individual holons' lesser cycles as catalyst.
6. **Injector** assembles the ContextPack-based prompt (Phase 5).

The key change: **the mind pipeline no longer just reports — it feeds back.** Graph-level diagnoses become catalyst for individual holons. This is the metabolism.

---

## 9. Success Metrics

How to know the refactor worked:

| Metric | Current (v0.5.0) | Target (v0.8.0) |
|---|---|---|
| Holonic primitives embodied | 4 / 20 | 18 / 20 |
| Lesser cycle ticking | No | Yes, every 60s |
| Greater cycle ticking | No | Yes, every 5min |
| Attractor field computed | No | Yes, on every holon |
| G_z / P_z computed | No | Yes, on every holon |
| Resonance R(H1, H2) | No (only embedding cosine) | Yes, 3-factor formula |
| Status ladder enforced | No (ad-hoc lifecycle_state) | Yes (ai-draft → canonical) |
| 5-Gate Validation | No | Yes, on every synthesis |
| ContextPack | No (unstructured tdg_context) | Yes (structured tdg_fetch_context) |
| Type classification | No | Yes, with T1/T2/T3 validation |
| Sinkhole-of-indifference detection | No | Yes, flagged in tdg_audit |
| Phase-transition detection | No (passive entropy) | Yes (4-pillar readiness index) |
| Diagnostic engine histories | Dead (&[]) | Live (persisted per holon) |
| `tools.rs` LOC | 3,464 | < 500 (mod.rs only) |
| Stale audit docs | Yes | Archived; replaced with CURRENT-STATE.md |
| Health score | ~0.50 | > 0.85 |
| Orphan ratio | < 15% (post-Phase 0 fix) | < 5% (lesser cycle generates catalyst at edges, reducing orphans) |

---

## 10. Open Questions & Risks

### 10.1 Open Questions

1. **Heartbeat frequency** — what's the right interval for the lesser cycle? 60s? 10s? Configurable per holon type? (HoloOS doesn't specify; it's event-driven, not time-driven.)
2. **Catalyst magnitude** — how do we quantify "incoming catalyst" from an edge? The theory says it's "boundary-crossing pressure" but doesn't give a formula. The HoloOS reference doesn't compute it either — it's an input to the lesser cycle, not an output.
3. **Type validation persistence** — T2 (excitation-invariance) requires tracking type_class across stage transitions. How long to keep history? Forever? Last N transitions?
4. **Greater cycle and the existing stage system** — the greater cycle's Transformation event advances the stage. But the existing stage system has evidence thresholds and age gates. Do both paths coexist? Which takes precedence?
5. **The 22 archetypes** — the theory doc is `ai-draft`. Should we implement them as a fixed library, or derive them from the type system?
6. **Scale code inference** — the backfill from `node_type` to `scale_code` is heuristic. Should we require explicit scale_code on creation?

### 10.2 Risks

1. **Performance** — ticking the lesser cycle on every active holon every 60s could be expensive at scale (100K nodes). Mitigation: only tick holons that have been touched recently (last 24h) or have pending catalyst.
2. **Complexity creep** — the refactor adds 6 new modules, 4 new tables, 8 new MCP tools. Risk of the codebase becoming harder to maintain. Mitigation: Phase 0 splits the god module first; each phase ships behind a feature flag.
3. **Theory drift** — the HoloOS theory is still evolving (some docs are `ai-draft`, some `canonical-hypothesis`). Implementing an `ai-draft` doc could mean rework when it's elevated. Mitigation: only implement `canonical` and `canonical-hypothesis` docs; mark `ai-draft` implementations as experimental.
4. **Migration irreversibility** — once `synthesis_status` is added and AI-created nodes are marked `AiDraft`, there's no going back. Mitigation: the column is nullable; the default is `AiDraft` but existing nodes can be backfilled to `CanonicalHypothesis` if needed.
5. **Agent disruption** — the Hermés adapter expects `tdg_context` to return a markdown string. Replacing it with `tdg_fetch_context` (structured) could break the adapter. Mitigation: keep `tdg_context` as a deprecated wrapper that calls `tdg_fetch_context` and renders to markdown.

---

## 11. Reference Document Index

For each phase, the HoloOS docs to consult:

| Phase | Primary reference | Secondary reference |
|---|---|---|
| Phase 0 | (none — internal hygiene) | `tdg-rust/AUDIT_REPORT.md`, `tdg-rust/upgrade-plan.md` (to archive) |
| Phase 1 | `_THEORY/02_Ontology/00.md` (master map), `_THEORY/02_Ontology/01.2_Tetra_Axes_Coordinate_System.md`, `_THEORY/02_Ontology/01.4_Scalar_Metric.md` | `HoloOS/AGENTS.md` (holon anatomy), `HoloOS/_INSTRUMENTS/schemas/taxonomy/scale_codes.yaml` |
| Phase 2 | `_THEORY/02_Ontology/02.1_Microcosmic_Metabolic_Architecture.md` (**the trusted anchor**) | `_THEORY/01_Epistemology/1_Grounding_Discipline.md` (Rule 1), `HoloOS/_INSTRUMENTS/lib/state_machine.py` |
| Phase 3 | `_THEORY/02_Ontology/08.1_Attractor_Field_Model.md`, `_THEORY/02_Ontology/08.3_Gz_Pz_Deepened_Articulation.md` | `HoloOS/_INSTRUMENTS/lib/attractor.py`, `HoloOS/_INSTRUMENTS/lib/health.py` |
| Phase 4 | `_THEORY/02_Ontology/02.2_Macrocosmic_Metabolic_Architecture.md`, `_THEORY/09_Thermodynamic_Framework/01_Phase_Transition_Model_Synthesis.md` | `HoloOS/_INSTRUMENTS/lib/state_machine.py` (greater cycle), `_THEORY/02_Ontology/06.3_Universal_Evolutionary_Protocol.md` |
| Phase 5 | `_THEORY/01_Epistemology/0_Method_of_Holonic_Inquiry.md`, `HoloOS/AGENTS.md` (ContextPack, status ladder) | `HoloOS/_INSTRUMENTS/lib/agent_api.py`, `HoloOS/_INSTRUMENTS/lib/validation_gate.py`, `HoloOS/_INSTRUMENTS/mcp_server.py` |
| Phase 6 | `_THEORY/02_Ontology/02.3_Holonic_Typology_Derivator.md`, `_THEORY/02_Ontology/02.4_Significator_Valence_and_Type.md`, `_THEORY/01_Epistemology/4_Type_Validation_Protocol.md` | `_THEORY/02_Ontology/03.1_Universal_Archetype_Anatomy.md`, `_THEORY/02_Ontology/03.2_22_Named_Archetypes_Index.md`, `HoloOS/_INSTRUMENTS/lib/archetype.py` |

---

## 12. Summary — The Path Forward

tdg-rust is not broken — it's incomplete. It has production-grade infrastructure (SQLite WAL, MCP server, circuit breaker, ONNX embeddings, scheduled maintenance) that the HoloOS Python reference lacks. But it lacks the operatorial core that makes a system holonic rather than merely developmental.

The refactor adds that core in 6 phases, each independently shippable, each behind a feature flag, each backward-compatible with the existing graph. The result is a TDG implementation that:

- **Metabolises** — every holon runs the lesser cycle on a heartbeat, consuming catalyst, accumulating experience, pressurising ascent.
- **Computes health** — G_z and P_z on every holon; the sinkhole-of-indifference is detectable.
- **Predicts bonding** — resonance R(H1, H2) based on attractor-field overlap, not just embedding cosine similarity.
- **Ascends via transformation** — stages advance via greater-cycle Transformation events, not just evidence accumulation.
- **Enforces epistemology** — 5-gate validation; AI outputs always start at `ai-draft`; human-only elevation.
- **Serves agents safely** — ContextPack aggregates intra/inter/extra context in one call; the epistemic spine (synthesis_status, grounding, type_class) is never dropped.

The mind that actually works is not a dashboard — it's a metabolism. This refactor builds it.

---

*Audit completed 2026-07-03. Source repositories: `ishan-parihar/tdg-rust` (v0.5.0), `ishanparihar/HoloOS`. This document is the working refactor plan; update it as phases ship.*
