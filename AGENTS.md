# AGENTS.md — TDG-Rust Development Guide for AI Agents

> **You are an AI agent working on the TDG-rust codebase.** This document is your operational manual — how to navigate the code, understand the architecture, make changes, and follow the conventions that govern all work here.

---

## Orientation

TDG-rust is a **Teleological Developmental Graph** — a memory infrastructure for AI agents that implements the [HoloOS](https://github.com/ishanparihar/HoloOS) holonic-science ontology. It is not a database; it is a **self-structuring neural memory** that metabolises, learns (Hebbian), grows its own connections (synaptogenesis), consolidates during idle time (sleep replay), and forgets low-value memories.

### What TDG Does

Every memory (holon) runs a **lesser cycle** (M·P·C·E metabolic engine) that processes catalyst, accumulates experience, and diagnoses metabolic inefficiencies. Holons compute their own **attractor field** A(H) = ⟨A_M, A_P, A_G, Γ⟩ and **health metrics** (G_z = integrative efficiency, P_z = transcendental tension). The graph-level **mind** diagnoses patterns and injects catalyst to force integration — a closed loop.

### What TDG Is Not

- Not a key-value store — it's a metabolic graph
- Not a vector database — embeddings are one input, not the whole system
- Not a static knowledge base — it grows, prunes, and reorganizes itself
- Not a chatbot memory — it's the agent's subconscious cognitive infrastructure

---

## Architecture Overview

```
src/
├── main.rs                    CLI entry + 7 background schedules
├── models.rs                  Core types: Node (33 fields), Edge, SynthesisStatus
├── holon.rs                   Holon newtype over &Node (compositional algebra)
├── scale_codes.rs             17 scale codes (S00 Cosmic → S80 Linguistic)
├── metabolism/                THE CORE — the metabolic engine
│   ├── lesser_cycle.rs        M·P·C·E 6-phase state machine (TRUSTED ANCHOR)
│   ├── greater_cycle.rs       S·T·G·Ch 9-phase state machine (vertical ascent)
│   ├── attractor.rs           A(H) = ⟨A_M, A_P, A_G, Γ⟩ computation
│   ├── health.rs              G_z, P_z, Resonance R(H1,H2), ResonanceComponents
│   └── worker.rs              Tier 2 async job pool + Hebbian co-activation tracking
├── context/                   Agent-facing API
│   ├── context_pack.rs        ContextPack builder + 5-min TTL cache
│   └── validation.rs          5-Gate Validation (Grounding, Failure-mode, Joint, Cosmological, Provenance)
├── holonic_types/             Type system
│   ├── archetypes.rs          22 named archetypes (7 roles × 3 complexes + Choice)
│   └── type_validation.rs     T1 (behavioral), T2 (excitation-invariance), T3 (fixed-point persistence)
├── mind/                      Mind pipeline
│   ├── graph_mind.rs          Graph-level mind integration (THE CLOSED LOOP)
│   ├── injector.rs            Prompt assembly + metabolic summary
│   ├── reflect_engine.rs      Skill discovery (entity + resonance clustering)
│   ├── consolidation_engine.rs  Graph health reporting
│   ├── feeling.rs             Drive state → experiential statements
│   ├── diagnostic.rs          Behavioral pattern analysis (legacy — being deprecated)
│   ├── pulse.rs               Structural gap detection
│   ├── terrain.rs             Skill discovery from graph density
│   └── state.rs               MindStateManager (working memory)
├── mcp/                       MCP server (50 tools)
│   ├── tools.rs               Tool implementations (#[tool_router])
│   ├── helpers.rs             ConnGuard, get_conn, validate_file_path, run_blocking
│   ├── synthesis_helpers.rs   LLM provider chain, pattern synthesis
│   ├── params.rs              Tool parameter structs
│   ├── server.rs              stdio + HTTP/SSE transport
│   ├── health.rs              ServiceHealthMonitor + circuit breakers
│   └── trust.rs               Agent trust store
├── db/                        SQLite persistence
│   ├── schema.rs              Schema + 13 migration phases
│   ├── crud.rs                Node/Edge/Embedding CRUD (2053 LOC)
│   ├── pool.rs                Connection pool
│   ├── events.rs              Event recording + trust queries
│   └── write_guard.rs         File-based write serialization
├── flow.rs                    Drive propagation + Hebbian learned rates
├── digestion.rs               Observation → hypothesis cascade
├── telearchy.rs               Stage-gated telos hierarchy (8 stages, 7 telos levels)
├── grammar/                   Node blueprints + auto-wiring
├── maintenance/               Janitor, enricher, archiver, monitor, orchestrator
├── plugins/                   Entity extractor, hybrid retriever, preference extractor
├── llm/                       LLM trait + OpenAI/Anthropic/Ollama providers
└── ...
```

---

## The 3-Tier Computation Model

All TDG operations fall into one of three tiers. **Never mix tiers** — it causes performance regressions.

| Tier | Latency | What runs | Concurrency |
|------|---------|-----------|-------------|
| **Tier 1** (sync) | <10ms | CRUD, provenance, FTS5, 5-Gate validation | Serialized (WriteGuard) |
| **Tier 2** (async) | <100ms/job | Lesser cycle ticks, attractor recompute, health, resonance, drive adaptation | Worker pool (default 1) |
| **Tier 3** (scheduled) | <10s/run | Greater cycle sweep, graph mind, synaptogenesis, LTD, replay, resonance rebuild | Single-threaded |

### The Closed Loop

```
Agent writes (Tier 1) → catalyst enqueued → lesser cycle ticks (Tier 2)
  → experience accumulates → drives adapt (Tier 2)
  → edge co_activation_count incremented (LTP) (Tier 2)
  → attractor field recomputed (Tier 2) → health G_z/P_z (Tier 2)
  → resonance graph updated (Tier 2)
  → graph mind diagnoses patterns (Tier 3) → catalyst injected
  → synaptogenesis creates new edges (Tier 3)
  → LTD decays unused edges (Tier 3)
  → replay re-activates recent memories (Tier 3)
  → forgetting archives low-value nodes (Tier 3)
  → next agent read sees updated state (Tier 1, cached)
```

---

## Key Concepts

### SynthesisStatus Ladder

Every node carries a `synthesis_status`:

```
ai-draft → canonical-hypothesis → canonical → superseded
```

- **All AI-produced content starts at `ai-draft`** — this is hardcoded in `tdg_observe` and `tdg_submit_synthesis`
- **Elevation above `ai-draft` is human-only** — `tdg_elevate` requires a `human_authorization` token
- **The 5-Gate Validation** gates elevation: Grounding, Failure-mode, Joint, Cosmological, Provenance

### Lesser Cycle (M·P·C·E)

The trusted anchor. 6-phase state machine:

```
Dormant → Ingesting → ProcessingSkewed|ProcessingIntegrated → Integrating → Quiescent → Dormant
```

- **Matrix (M)**: current-state organizer (conserved structure)
- **Potentiator (P)**: latent-state generator (possibility space)
- **Catalyst (C)**: incoming perturbation (from edges/agent)
- **Experience (E)**: processed input (accumulated learning)

Shadow diagnosis at Integrating phase:
- `MatrixHyperIngestion` (formerly DarkAddiction) — excess catalyst, rigid
- `MatrixHypoIngestion` (formerly DarkAllergy) — too little catalyst, fragile
- `PotentiatorHyperIngestion` — ungrounded experience floods
- `PotentiatorHypoIngestion` — refuses emergence

### Attractor Field A(H) = ⟨A_M, A_P, A_G, Γ⟩

The unified operational object:
- **A_M**: Matrix attractor (current homeostatic basin)
- **A_P**: Potentiator attractor (latent basin)
- **A_G**: Great-Way attractor (environmental basin, from edge count + transformation pressure)
- **Γ**: Coupling tensor (4 drives on 2-torus: ag/cm anti-correlated, er/agp anti-correlated)
- **π**: polarity disposition (-1 acceptor, 0 sharer, +1 donor, None noble)
- **type_class**: e.g., "strong-donor-sto", "sharer", "noble-graduated", "transient"

### Health Metrics

- **G_z = 100·(A_z/100 · C_z/100 · B_H · B_V)^(1/4)** — rewards balance
- **P_z = 100·∇Ψ·cos(θ_alignment)** — rewards commitment
- **Total = G_z · P_z** — both required (high G_z + low P_z = depolarized, formerly "sinkhole")
- States: Optimal (>70/>50), SubOptimal, Collapse (<30), Depolarized (<10)

### V/C/R/N Coordinate System

Every holon has a 4-axis coordinate (per HoloOS 08.8.x):
- **V** = Verticality ⟨O, D, S⟩ — consciousness-condensation altitude (octave, density, sub_density)
- **C** = Collectivity — "individual" | "collective" | "universal"
- **R** = Realm-Placement — "gross" | "subtle" | "causal" (within-octave dimensional axis)
- **N** = Nesting — Sub(N)↓ / Sup(N)↑ (directional exploration, query parameter)

### Neural Plasticity

| Mechanism | Implementation | Schedule |
|---|---|---|
| **LTP** (strengthening) | `co_activation_count` incremented on catalyst flow; `flow_rate = base + 0.1·ln(1+count)` | Per-tick (Tier 2) |
| **LTD** (weakening) | `co_activation_count /= 2` for edges inactive >7d; prune `weight < 0.3` | 1 hour (Tier 3) |
| **Synaptogenesis** | Create `RESONATES_WITH` edge for `resonance_graph` pairs with R > 0.7 | 30 min (Tier 3) |
| **Replay** | Inject 0.3 catalyst into nodes from last 24h events | 6 hours (Tier 3) |
| **Forgetting** | Archive nodes: `retrieval_count=0 AND confidence<0.3 AND age>30d` | 6 hours (Tier 3) |
| **Drive adaptation** | After cycle completion: `positive_pole += learning_rate`, `negative_pole -= 0.3·learning_rate` | Per-cycle (Tier 2) |

---

## Development Rules

### 1. Never block Tier 1 on Tier 2/3 work

Tier 1 writes (tdg_observe, tdg_connect, tdg_create) must return in <10ms. Enqueue metabolism work via `enqueue_job()` — don't call `lesser_cycle::tick()` synchronously.

### 2. All writes go through the write guard + circuit breaker

```rust
// CORRECT:
crate::flow::store_drive_state_pub(conn, node_id, &drives_json)?;

// WRONG (bypasses safety):
conn.execute("UPDATE nodes SET drives_json = ?1 WHERE id = ?2", params![...])?;
```

### 3. Status ladder is non-negotiable

- AI-produced content is ALWAYS `ai-draft`
- `tdg_submit_synthesis` hardcodes `synthesis_status: "ai-draft"` in the node creation
- `tdg_elevate` requires `human_authorization` — AI agents cannot self-elevate
- The 5-Gate Validation only allows auto-pass from `blocked → passed`, NEVER above `ai-draft`

### 4. Schema changes are additive + idempotent

```rust
// CORRECT: nullable column with default, wrapped in error-swallow
for (table, column, typedef) in &[
    ("nodes", "new_field", "TEXT"),
] {
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {typedef}");
    match conn.execute_batch(&sql) {
        Ok(()) => {}
        Err(rusqlite::Error::ExecuteReturnedResults) => {}
        Err(_) => { /* column already exists */ }
    }
}
```

### 5. Use universal semantics (not D3-human-experiential terms)

Per HoloOS Epistemology doc 6:

| ❌ Don't use | ✅ Use |
|---|---|
| "addiction" | "hyper-ingestion" |
| "allergy" | "hypo-ingestion" |
| "shadow" | "metabolic inefficiency" |
| "sinkhole" | "depolarized" |
| "feeling" | "drive state" |
| "crisis" | "transformation pressure" |

The `Shadow` enum variants use universal names (`MatrixHyperIngestion` etc.) with serde aliases for backward-compatible JSON deserialization of old names.

### 6. Test everything

```bash
cargo test --lib                    # 430 unit tests
cargo test --test integration       # 8 integration tests
cargo test --test mcp_e2e           # 66 MCP E2E tests
cargo test --test e2e_mind_simulation  # 5 full mind-flow tests
```

**509 tests. Zero warnings. Zero regressions.** Don't break this.

### 7. Push after each phase

The project follows a phase-based development model. Each phase is a commit with a descriptive message. Always push to `main` after completing a phase.

---

## Build Requirements

### Local Development

```bash
# Standard build (no ONNX)
cargo build --release

# With ONNX embeddings
cargo build --release --features onnx

# Run tests (ONNX feature set)
cargo test --features onnx
```

### VPS Deployment (Debian 12, glibc 2.36)

**CRITICAL**: Never build on the VPS. Always cross-build locally and deploy.

```bash
# Required tools
cargo install cargo-zigbuild
pacman -S zig  # or equivalent

# Download ONNX Runtime
mkdir -p /tmp/onnxruntime-linux-x64-1.20.1
curl -fsSL "https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-1.20.1.tgz" \
  | tar -xz -C /tmp/onnxruntime-linux-x64-1.20.1 --strip-components=1

# Build
export ORT_LIB_LOCATION=/tmp/onnxruntime-linux-x64-1.20.1/lib
export ORT_PREFER_DYNAMIC_LINK=1
cargo zigbuild --release --features onnx --target x86_64-unknown-linux-gnu.2.36
```

### FTS5 Invariants

The FTS virtual table uses **external content** mode. Critical:

| Item | Correct | Wrong |
|------|---------|-------|
| FTS column for node PK | `id UNINDEXED` | `node_id` |
| Trigger inserts | `INSERT INTO nodes_fts(rowid, id, name, description)` | `... node_id ...` |
| Rebuild | `INSERT INTO nodes_fts(nodes_fts) VALUES('rebuild')` | `DELETE FROM nodes_fts` |

---

## The Hermés Adapter

`plugins/tdg/__init__.py` is a Python `MemoryProvider` for the [Hermes Agent](https://github.com/nousresearch/hermes-agent) framework.

### Interface (must match Hermes `MemoryProvider` ABC)

| Method | What it does |
|---|---|
| `is_available()` | Check if tdg-rust binary exists and is executable |
| `initialize(session_id)` | Spawn MCP client subprocess |
| `system_prompt_block()` | Call `tdg_context` → return markdown prompt |
| `prefetch(query)` | Call `tdg_search` + `tdg_resonance_partners` for top results |
| `sync_turn(user, assistant)` | Call `tdg_observe` with `trigger_digestion=True` |
| `get_tool_schemas()` | Return 3 LLM-facing tool schemas |
| `handle_tool_call(name, args)` | Dispatch to MCP tools |
| `on_memory_write(action, target, content)` | Mirror writes as observations |
| `on_session_end(messages)` | Create session-summary observation |
| `shutdown()` | Kill subprocess |

### Known limitations

1. **One subprocess per call** — no stdio session reuse (each call pays ~200ms startup)
2. **`sync_turn` truncates** to `user[:200] + assistant[:300]` — information loss
3. **No retry/backoff** — single 30s MCP timeout fails permanently

---

## Key Files Reference

| File | What to know |
|---|---|
| `src/models.rs` | `Node` struct has 33 fields. Any schema change requires updating: `add_node` INSERT, `row_to_node` SELECT, `update_node` match arms, ALL SELECT queries (6+ locations), `NewNode` struct |
| `src/db/schema.rs` | Schema + 13 migration phases. New columns go in both `SCHEMA_SQL` (for fresh DBs) AND `run_migrations()` (for existing DBs) |
| `src/flow.rs` | `edge_flow_rate()` is a hardcoded table (192 LOC). `get_flow_rate_for_edge()` adds Hebbian learned rate. Negative base rates (BLOCKS, CONTRADICTS) skip LTP |
| `src/metabolism/worker.rs` | `execute_lesser_tick()` is the hot path — called for every catalyst injection. Contains Hebbian tracking + drive adaptation + upward pressure + attractor recompute enqueue |
| `src/mind/graph_mind.rs` | `run_integration()` is the closed loop — diagnoses 5 graph patterns and injects catalyst |
| `src/context/context_pack.rs` | `build()` has 5-min TTL cache. ContextPack includes realm_placement + collectivity in identity |
| `src/mcp/tools.rs` | 50 tools in one `#[tool_router]` impl block. The rmcp macro requires all `#[tool]` methods in one file |

---

## Phase History

| Phase | What was built |
|-------|---------------|
| 0 | Hygiene: split god module, fix dead diagnostic histories, dead code cleanup |
| 1 | Holon newtype + SynthesisStatus ladder + Scale codes (S00-S80) |
| 2 | Lesser cycle (M·P·C·E) — the trusted anchor, event-driven metabolism |
| 3 | Attractor field A(H) + G_z/P_z + Resonance R(H1,H2) |
| 4 | Greater cycle (S·T·G·Ch) + 4-pillar phase-transition detector |
| 5 | ContextPack + 5-Gate Validation (epistemic enforcement) |
| 6 | 22 archetypes + T1/T2/T3 type validation + Type⊥Stage orthogonality |
| 7-11 | V/C/R/N coordinate system + universal semantics + mind pipeline integration + Hermés adapter + realm-aware catalyst |
| 12-15 | Graph-level mind (closed loop) + ContextPack caching + resonance reflect + dissolution ratio |
| 16 | Hebbian edge-weight learning (LTP/LTD) |
| 17 | Synaptogenesis (grow new edges from resonance) |
| 18 | Memory replay (sleep consolidation) + value-based forgetting |
| 19 | Drive-metabolism unification (drives adapt from experience) |
| 20 | Embedding consistency fix |
| 21 | Fragmentation cleanup (6 critical bugs fixed) |
| 22 | P0/P1/P2 bug fixes + E2E mind simulation test |

---

## Standing Rules

1. **Never build on VPS** — always cross-build with `cargo zigbuild`
2. **Test ONNX features** — `cargo test --features onnx` before release commits
3. **Schema changes** — add migrations in `src/db/schema.rs`; update `add_node`, `row_to_node`, `update_node`, and ALL SELECT queries
4. **New MCP tools** — add params to `params.rs`, tool to `tools.rs`, update doc comment count
5. **Push after each phase** — the project follows phase-based development
6. **Zero warnings** — `cargo check` must pass with zero warnings before commit
7. **509 tests** — don't break existing tests. Add new tests for new features.
8. **Universal semantics** — use holonically-universal terminology, not D3-human-experiential terms

---

## Where to Start

If you're a new AI agent reading this to do work on TDG-rust:

1. **Read `docs/CURRENT-STATE.md`** for the current build/test status
2. **Read `docs/NEURO-BIO-AUDIT-V2.md`** for the latest audit findings and gap analysis
3. **Run `cargo test`** to verify the baseline (509 tests should pass)
4. **Read `src/metabolism/lesser_cycle.rs`** — this is the trusted anchor; everything derives from it
5. **Read `src/mind/graph_mind.rs`** — this is the closed loop; it's how the mind self-regulates
6. **Check `docs/` for audit reports** — they contain the roadmap for future work

---

*Last updated 2026-07-04. This document is the operational guide for AI agents working on TDG-rust.*
