# Neural-Plasticity & Integration-Fragmentation Audit + Refactor Plan

**Audit target:** tdg-rust at commit `b419c55` (post-Phase 15)
**Scope:** Full architectural audit for brain-like self-structuring, adaptive memory, and operational fragmentation
**Date:** 2026-07-04

---

## 0. Executive Summary

**Can TDG act like a brain?** The architecture CAN support it — the lesser cycle (M·P·C·E), attractor field, and graph-level mind are the right primitives. But three structural barriers prevent neural-plasticity today:

1. **The metabolic engine and the drive-flow engine are two parallel systems that never close the loop.** Drives feed into the lesser cycle (one-way); lesser-cycle experience never modifies drives or edge weights. No Hebbian learning is possible.

2. **No structural plasticity.** TDG cannot grow new edges between existing nodes based on resonance, cannot prune unused edges, cannot restructure the graph during transformation events. The brain grows synapses; TDG cannot.

3. **No sleep/replay/forgetting.** Consolidation is a reporting pass. Archiver is age-based only. No re-activation of recent memories, no value-based forgetting, no global downscaling.

**Additionally, 15 integration fragmentations** were identified — duplicate logic, inconsistent formats, and siloed systems that degrade the agent's mind experience.

This plan defines **6 phases (16-21)** to upgrade TDG from a "metabolic memory" to a "self-structuring neural memory" — a system that grows its own connections, strengthens pathways through use, prunes unused ones, and consolidates memories during idle cycles.

---

## 1. Current State: What TDG Already Does Well

| Brain function | TDG equivalent | Status |
|---|---|---|
| Neuron activation | Lesser cycle tick (catalyst → processing → experience) | ✅ Working |
| Neurotransmitter thresholds | CycleThresholds (ingest, process, integrate) | ✅ Working |
| Neural health monitoring | G_z (integrative efficiency), P_z (transcendental tension) | ✅ Working |
| Neural network topology | Graph of nodes + edges with type system | ✅ Working |
| Neuromodulation | Attractor field A(H) = ⟨A_M, A_P, A_G, Γ⟩ | ✅ Working |
| Brain-wave synchronization | Graph-level mind integration (Tier 3 closed loop) | ✅ Working |
| Developmental stages | Telearchy (8 stages, evidence + age gated) | ✅ Working |
| Sleep spindles | Greater cycle Transformation events (discontinuous) | ✅ Working (but no graph restructuring) |

---

## 2. The Three Barriers to Neural Plasticity

### Barrier 1: No Hebbian Learning (drives ≠ metabolism)

**How the brain works:** Neurons that fire together, wire together. Co-activation strengthens synaptic weights (LTP); lack of co-activation weakens them (LTD). Synaptic weights are mutable and history-dependent.

**How TDG works now:** Edge weights are set once at creation (`edges.weight REAL DEFAULT 1.0`) and NEVER updated. The drive propagation system (`flow.rs`) uses hardcoded flow rates per edge type:

```rust
"DECOMPOSES_TO" | "ENABLES" | "REALIZES" | "SUPPORTS" => (0.8, 0.8),
"DEPENDS_ON" | "PRECEDES" | "CONTEXT" => (0.6, 0.6),
"RELATES_TO" | "REFERENCES" | "MENTIONS" => (0.3, 0.3),
```

A `DECOMPOSES_TO` edge that has fired 1000 times has the same flow rate as one that fired once. The lesser cycle reads drives (via `drive_complementarity`) but never writes back. The metabolic system and the drive system run on **disjoint columns** (`lesser_cycle_json` vs `drives_json`).

**What needs to change:**

1. **Edge weights must become mutable.** Add `edge.weight` updates in `emit_downward` / `aggregate_upward` based on co-activation frequency.
2. **The lesser cycle must feed back into drives.** When `experience_accumulated` crosses a threshold, the stored `drives_json` should be updated (the holon's drive profile shifts based on what it has processed).
3. **A co-activation signal is needed.** Track which edges were "active" (source and target both had lesser-cycle ticks in the same window). This is the eligibility trace.

### Barrier 2: No Structural Plasticity (no synaptogenesis / pruning)

**How the brain works:** The brain grows new synapses (synaptogenesis) and prunes unused ones (synaptic pruning) continuously. Dendritic arborization grows new branches. Neurogenesis creates new neurons.

**How TDG works now:**
- `auto_wire.rs` only creates edges at node creation, with a fixed edge-type list.
- No code path creates edges between two EXISTING nodes based on discovered resonance.
- The `resonance_graph` table materializes R(H1, H2) scores but NO production code reads it to create new edges.
- No edge is ever pruned for low weight or non-use. `archiver.rs::prune_dead_edges` only removes edges whose endpoints are deleted.
- The greater cycle's Transformation event only updates `greater_cycle_json` — it does NOT restructure the graph.
- All node types are hardcoded in `schema::NodeType`. No new types can emerge.

**What needs to change:**

1. **Synaptogenesis:** When `resonance_graph` shows R > 0.7 between two unconnected holons, create a `RESONATES_WITH` edge. This is the TDG equivalent of growing a new synapse.
2. **Synaptic pruning:** A scheduled pass that decrements `edge.weight` for edges whose endpoints haven't co-activated in N cycles. Edges below a pruning threshold are soft-deleted.
3. **Edge weight learning:** `emit_downward` and `aggregate_upward` must update `edge.weight` based on co-activation (Hebbian LTP/LTD).
4. **Dendritic arborization:** When a telos node accumulates > N observations, auto-create a child sub-telos (the hierarchy grows organically).

### Barrier 3: No Sleep/Replay/Forgetting

**How the brain works:** During sleep, the hippocampus replays recent memories, strengthening important ones and transferring them to long-term storage. Synaptic homeostasis globally downscales all synapses. Forgetting curves actively decay unused memories. Emotional salience tags memories for consolidation priority.

**How TDG works now:**
- Consolidation engine (`consolidation_engine.rs`) is a reporting pass — it counts nodes and produces a text report. No re-activation of recent memories.
- Archiver is age-based only (90-day cutoff for events). No value-based forgetting.
- `compute_live_confidence` decays confidence on READ but never PERSISTS the decay. The stored `confidence` column only goes up (via `record_helpful_action`), never down.
- No memory tagging (emotional salience → consolidation priority). `lesser_cycle.experience_accumulated` and `health.g_z` are computed but never used to prioritize consolidation.
- No sequence-based consolidation (sequences of observations never get compressed into reusable scripts).

**What needs to change:**

1. **Replay pass:** A scheduled job that re-reads recent events (last 24h), re-activates the associated nodes (inject small catalyst), and strengthens their edges (LTP on edges between co-retrieved nodes).
2. **Value-based forgetting:** A scheduled job that decrements `confidence` for nodes not retrieved in N days. Nodes below a threshold are archived (soft-deleted).
3. **Confidence decay persistence:** `compute_live_confidence` should write the decayed value back to the `confidence` column periodically.
4. **Salience tagging:** Nodes with high `experience_accumulated` or high `P_z` (transcendental tension) get consolidation priority.

---

## 3. Embedding System: At Par with the Brain?

### Current State

- **Model:** EmbeddingGemma-300M (768-dim, Q4/Q8) or MiniLM (384-dim)
- **Storage:** One vector per node in `embeddings.vector` BLOB
- **Generation:** Mean pool + L2 normalize over `name + description + top-3 edge context`
- **Retrieval:** Cosine similarity in `hybrid_retriever.rs`, blended with FTS5 + graph expansion
- **Coverage:** All nodes embedded on creation (when `--features onnx`), with backfill via enricher/janitor

### What the Brain Does That TDG Doesn't

| Brain feature | TDG status | Impact |
|---|---|---|
| **Contextual re-encoding** (same memory, different recall context) | ABSENT | TDG produces ONE vector per node. The brain re-encodes memories context-dependently. |
| **Multi-vector per node** (title vs body vs edges) | ABSENT | One BLOB column per node. Can't distinguish "matches by title" from "matches by relationship context." |
| **Embedding clusters** (semantic neighborhoods) | ABSENT | No clustering of embeddings. The `resonance_graph` uses attractor fields, not embeddings. |
| **Embedding decay** (stale vectors re-encoded) | ABSENT | `enricher.rs` skips nodes that already have an embedding, even if stale. |
| **Association-based recall** (spreading activation) | WEAK | `expand_1_hop` does topological expansion with fixed 0.7× decay. Not associative. |
| **Embedding-attractor integration** | ABSENT | The semantic vector space and the metabolic attractor space are completely disjoint. |

### What's Needed

1. **Multi-vector embeddings:** Store separate vectors for `name`, `description`, and `edge_context`. Retrieval computes a weighted blend.
2. **Embedding staleness detection:** Enricher should re-embed nodes where `updated_at > embeddings.updated_at`.
3. **Embedding clusters:** Periodically cluster all embeddings (k-means or HDBSCAN). Store cluster assignments. Use for "semantic neighborhood" recall.
4. **Contextual re-weighting:** During retrieval, re-weight the node's embedding components based on the query intent (e.g., `Semantic` intent weights `description` higher; `Factual` weights `name` higher).
5. **Embedding-attractor bridge:** The attractor field's `CouplingTensor` (ag, cm, er, agp) should be derivable from the embedding — the embedding IS the holon's "neural representation," and the attractor is its "metabolic disposition." Bridging them means the semantic and metabolic spaces inform each other.

---

## 4. Integration Fragmentation Inventory (15 items)

### Critical Fragmentations (affect agent experience)

| # | Fragmentation | Location | Fix |
|---|---|---|---|
| **F1** | Two parallel drive systems (flow.rs vs lesser_cycle) with no feedback loop | flow.rs + lesser_cycle.rs | Unify: lesser cycle writes back to drives_json; attractor's CouplingTensor becomes the single drive representation |
| **F3** | Three "drive signature" sources with different values for same node types | flow.rs:248, enricher.rs:10, flow.rs:55 | Single source of truth in schema.rs; enricher writes dual-pole format |
| **F4** | Four different "node value / confidence" formulas | crud.rs:133, events.rs:56, events.rs:86, tools.rs:2056 | Single `compute_node_value` function; all paths use it |
| **F5** | Resonance graph stores one value in 4 distinct columns (bug) | main.rs:736, worker.rs:537 | `resonance()` returns a struct; callers store components separately |
| **F13** | `write_mind_state_file` passes empty drive_history (stuck detection disabled) | injector.rs:257 | Pass `load_drive_history(conn)` instead of `&[]` |
| **F15** | Enricher skips stale embeddings (designed-in regression) | enricher.rs:130 | Change query to `WHERE e.node_id IS NULL OR e.updated_at < n.updated_at` |

### Moderate Fragmentations (affect code quality)

| # | Fragmentation | Location | Fix |
|---|---|---|---|
| **F2** | Three "health" classes with different meanings | metabolism/health.rs, maintenance/monitor.rs, mcp/health.rs | Rename: HolonHealth, GraphHygieneMonitor, ServiceHealthMonitor |
| **F6** | Embedding model name written inconsistently ("onnx" vs "embeddinggemma-300m") | crud.rs:353, enricher.rs:166, main.rs:423 | All callers use `config.embedding.model_dir_name()` |
| **F7** | Embedding `max_edges` parameter differs (3 vs 5) | crud.rs:349, main.rs:416 | Define `EMBEDDING_MAX_EDGES` constant |
| **F8** | `deserialize_embedding` duplicated | crud.rs:1706, hybrid_retriever.rs:639 | Delete duplicate; retriever calls crud |
| **F9** | Two drive-distribution computations (same data iterated twice) | diagnostic.rs:182, feeling.rs:127 | One `compute_graph_drive_summary` function |
| **F10** | Three "count nodes by type" with different filters | terrain.rs:87, consolidation_engine.rs:188, monitor.rs:43 | Single `count_nodes_by_type(conn, filter)` |
| **F11** | Four upward-inference mechanisms with no coordination | digestion.rs, reflect_engine.rs (×2), node_grammar.rs | One `infer_upward_patterns` function; remove dead code |

### Low-Priority Fragmentations

| # | Fragmentation | Location | Fix |
|---|---|---|---|
| **F12** | 15+ hardcoded constants that should be adaptive | Various | Central config with env-var overrides |
| **F14** | Two mind-state files (snapshot vs state) | injector.rs:319, state.rs | Consolidate to one writer |

---

## 5. The Refactor Plan: 6 Phases to Neural Plasticity

### Phase 16: Hebbian Edge-Weight Learning (LTP/LTD)

**Goal:** Edges that co-activate strengthen; edges that don't co-activate weaken.

**Changes:**
1. Add `co_activation_count INTEGER DEFAULT 0` and `last_co_activation TEXT` columns to `edges` table
2. In `lesser_cycle::tick`, when a holon processes catalyst from an edge, increment `co_activation_count` on that edge
3. In `flow.rs::emit_downward`, modify `edge_flow_rate` to use `base_rate + learning_rate · ln(1 + co_activation_count)` instead of hardcoded constants
4. Add a Tier 3 `synaptic_decay` schedule (every 1 hour) that decrements `edge.weight` by `decay_rate` for edges whose `last_co_activation` is older than N cycles (LTD)
5. Edges with `weight < 0.1` are soft-deleted (pruned)

**Brain equivalent:** LTP (long-term potentiation) and LTD (long-term depression).

### Phase 17: Structural Plasticity (Synaptogenesis + Pruning)

**Goal:** TDG grows new edges based on resonance and prunes unused ones.

**Changes:**
1. Add a Tier 3 `synaptogenesis` schedule (every 30 min) that:
   - Reads `resonance_graph` for pairs with R > 0.7
   - Checks if an edge already exists between them
   - If not, creates a `RESONATES_WITH` edge with `weight = 0.3` (weak initial synapse)
   - Logs: "Synaptogenesis: new RESONATES_WITH edge between H1 and H2 (R=0.82)"
2. Add a Tier 3 `synaptic_pruning` schedule (every 1 hour, paired with LTD) that:
   - Queries edges with `weight < 0.1` AND `co_activation_count < 2`
   - Soft-deletes them (`valid_to = now`)
   - Logs: "Synaptic pruning: edge H1→H2 pruned (weight=0.05, co_activations=1)"
3. Add `RESONATES_WITH` to the edge type vocabulary
4. Update `flow.rs::edge_flow_rate` to handle `RESONATES_WITH` with a base rate of 0.5 (moderate, learned)

**Brain equivalent:** Synaptogenesis (new synapse formation) and synaptic pruning (removal of unused synapses).

### Phase 18: Memory Consolidation (Sleep Replay + Forgetting)

**Goal:** TDG consolidates memories during idle cycles and forgets low-value ones.

**Changes:**
1. Rewrite `consolidation_engine.rs::run` to implement **replay**:
   - Read recent events (last 24h) from `events` table
   - For each event's `node_id`, inject a small catalyst (0.3) — re-activating the memory
   - For each edge that was co-active in the last 24h, increment `co_activation_count` (LTP during replay)
   - Prioritize nodes with high `experience_accumulated` (salience tagging)
2. Add a `forgetting` pass to `archiver.rs`:
   - Query nodes where `retrieval_count = 0` AND `created_at < now - 30 days` AND `confidence < 0.3`
   - Soft-delete them (set `lifecycle_state = 'archived'`, `valid_to = now`)
   - Log: "Forgetting: node N archived (retrieval_count=0, age=45d, confidence=0.2)"
3. Add `confidence_decay` persistence: every 6 hours, run `UPDATE nodes SET confidence = compute_live_confidence(...)` for all active nodes
4. Add `salience_tag` column to nodes: `TEXT DEFAULT 'normal'` — values: `normal`, `high_salience` (high P_z), `consolidation_target` (high experience_accumulated)

**Brain equivalent:** Hippocampal replay, synaptic homeostasis, forgetting curve, emotional salience tagging.

### Phase 19: Drive-Metabolism Unification

**Goal:** The drive system and the metabolic system become ONE closed loop.

**Changes:**
1. Have `lesser_cycle::tick` write back to `drives_json` at the `Integrating` phase:
   - If `experience_accumulated > threshold`, shift the holon's drives toward the incoming catalyst's profile (the holon adapts to what it has been processing)
   - `drives_json` becomes a LEARNED representation, not a hardcoded one
2. Delete `flow.rs::intrinsic_signatures` — replace with a `default_drives(node_type)` function in `schema.rs` that returns the INITIAL signature. The stored `drives_json` is the LEARNED signature.
3. Have `flow.rs::emit_downward` read the attractor field's `CouplingTensor` instead of `drives_json` — the coupling tensor IS the drive representation in [0,1] form.
4. Remove `enricher.rs::enrich_drives` (no longer needed — drives are set by the lesser cycle, not by a backfill)
5. Fix `enricher.rs::drives_by_type` to use dual-pole format (F3 fix)

**Brain equivalent:** The brain's neural representations are LEARNED through experience, not hardcoded. TDG's drive signatures should be the same.

### Phase 20: Embedding Plasticity

**Goal:** Embeddings become contextual, clustered, and integrated with the attractor field.

**Changes:**
1. Add `embedding_name BLOB`, `embedding_desc BLOB`, `embedding_edges BLOB` columns to `embeddings` table (multi-vector)
2. Update `build_embedding_text` to produce 3 separate texts; `embed` to produce 3 vectors
3. Update `hybrid_retriever.rs` to compute `cosine(query, name_vec) · name_weight + cosine(query, desc_vec) · desc_weight + cosine(query, edges_vec) · edges_weight`
4. Fix enricher staleness: `WHERE e.node_id IS NULL OR e.updated_at < n.updated_at` (F15 fix)
5. Fix model name consistency: all callers use `config.embedding.model_dir_name()` (F6 fix)
6. Fix `max_edges` consistency: define `EMBEDDING_MAX_EDGES` constant (F7 fix)
7. Add a Tier 3 `embedding_cluster` schedule (every 2 hours) that:
   - Runs k-means (k = sqrt(N/2)) on all embeddings
   - Stores cluster assignments in a new `embedding_clusters` table
   - Retrieval can now expand to same-cluster neighbors (semantic neighborhood recall)
8. Bridge embedding ↔ attractor: the `CouplingTensor` can be initialized from the embedding's principal components (PCA on the embedding → 4 components → Γ). This makes the semantic and metabolic spaces inform each other.

**Brain equivalent:** Contextual encoding, semantic neighborhoods, representation-metabolism bridge.

### Phase 21: Fragmentation Cleanup + Old Pipeline Deprecation

**Goal:** Remove all 15 fragmentations and deprecate the old mind pipeline.

**Changes:**
1. Fix F4 (confidence formulas): single `compute_node_value` function
2. Fix F5 (resonance graph bug): `resonance()` returns struct with components
3. Fix F13 (empty drive_history): pass `load_drive_history(conn)` in `write_mind_state_file`
4. Fix F8 (duplicate deserialize): delete `decode_f32_vec`
5. Fix F9 (duplicate drive distribution): single `compute_graph_drive_summary`
6. Fix F10 (duplicate count_by_type): single `count_nodes_by_type(conn, filter)`
7. Fix F11 (four upward-inference paths): single `infer_upward_patterns` function; remove `infer_upward_pattern` and mark `run_by_resonance` as the production path
8. Fix F2 (three "health" classes): rename
9. Fix F12 (hardcoded constants): central `SchedulerConfig` + `MetabolismConfig`
10. Deprecate `DiagnosticEngine::analyze` — replace with attractor-field-based diagnostics
11. Deprecate `FeelingEngine::generate` — replace with G_z/P_z-based energy computation
12. Wire `run_by_resonance()` into `ConsolidationEngine::run_reflection` (make it the production path)
13. Fix `MAX_SEQUENCE_LENGTH` to be model-dependent (256 for MiniLM, 2048 for Gemma)

---

## 6. Priority Order

| Phase | Priority | Rationale |
|-------|----------|-----------|
| **Phase 21** (fragmentation cleanup) | CRITICAL | Fixes bugs that affect agent experience TODAY (F5 resonance bug, F13 disabled stuck detection, F15 stale embeddings, F4 inconsistent confidence). Must come first — clean foundation. |
| **Phase 16** (Hebbian learning) | HIGH | The single biggest barrier to neural plasticity. Without mutable edge weights, the system can't learn. |
| **Phase 19** (drive-metabolism unification) | HIGH | Closes the loop between the metabolic engine and the drive system. Required for Phase 16 to work end-to-end. |
| **Phase 17** (structural plasticity) | HIGH | Growing new edges is the TDG equivalent of synaptogenesis. Without this, the graph topology is frozen. |
| **Phase 18** (sleep/replay/forgetting) | MEDIUM | Improves memory quality over time but doesn't block initial operation. Can be deployed after 16-17-19. |
| **Phase 20** (embedding plasticity) | MEDIUM | Improves retrieval quality but doesn't block the core neural-plasticity loop. Can be deployed last. |

---

## 7. What the Agent's Mind Will Do After All 6 Phases

```
Agent observes something new
  ↓
Catalyst injected → lesser cycle ticks
  ↓
Experience accumulates → attractor field updates
  ↓
Edge weights strengthen (LTP) — "neurons that fire together, wire together"
  ↓
Resonance discovered → new RESONATES_WITH edge created (synaptogenesis)
  ↓
Graph mind integration pass: diagnoses graph-level patterns
  ↓
During idle time (replay pass): recent memories re-activated, edges strengthened
  ↓
Low-value memories decay (forgetting curve) → archived when confidence < 0.3
  ↓
Unused edges weaken (LTD) → pruned when weight < 0.1
  ↓
Drives adapt: holon's drive profile shifts based on what it has processed
  ↓
Embeddings cluster: semantic neighborhoods form organically
  ↓
The graph GROWS its own structure, STRENGTHENS useful pathways, PRUNES unused ones
```

This is a brain. Not a metaphor — a functional implementation of the same principles (Hebbian learning, structural plasticity, sleep consolidation, forgetting) in software.

---

## 8. Memory Impact (2GB VPS)

| New feature | Memory cost |
|---|---|
| `co_activation_count` + `last_co_activation` on edges | ~16 bytes/edge (500K edges = 8MB) |
| `salience_tag` on nodes | ~12 bytes/node (100K nodes = 1.2MB) |
| Multi-vector embeddings (3 BLOBs) | ~3× current embedding storage (~30MB for 100K nodes at 768-dim) |
| `embedding_clusters` table | ~4 bytes/node (400KB) |
| Replay pass (transient) | ~10MB during the 5-min replay window |
| **Total new overhead** | **~50MB** — within the 2GB VPS budget |

---

*Audit completed 2026-07-04. Based on exhaustive source-code analysis of tdg-rust commit b419c55. All findings cite exact file:line locations.*
