# tdg-rust Current State — Post-Optimization (July 2026)

**Version:** v0.5.0 (post-Phase 6 refactor + ponytail audit optimization)
**Build:** `cargo check` passes with **zero warnings**, 499 tests pass (425 lib + 8 integration + 66 MCP E2E)

---

## What This Is

tdg-rust is a **Teleological Developmental Graph** — a memory infrastructure for AI agents that embodies the holonic-science ontology from HoloOS. It is not a database; it is a **metabolism** — every holon runs the M·P·C·E lesser cycle and S·T·G·Ch greater cycle through a shared contact boundary, with event-driven async processing.

## Build & Test

```bash
cargo check                              # zero warnings
cargo test --lib                         # 425 tests
cargo test --test integration            # 8 tests
cargo test --test mcp_e2e                # 66 tests
cargo build --release --features onnx    # production binary with ONNX embeddings
```

## Architecture (Post-Refactor)

```
src/
├── main.rs                  CLI entry point + background schedulers
├── lib.rs                   Library root, 36 modules
├── models.rs                Core types: Node, Edge, SynthesisStatus
├── schema.rs                Enums: Stage, Quadrant, CatalystType, TelosLevel
├── config.rs                Hierarchical config (YAML → JSON → env)
├── error.rs                 TdgError, TdgResult
├── holon.rs                 Holon newtype (compositional algebra)
├── scale_codes.rs           17 organisational scale codes (S00-S80)
├── holonic_types/           22 archetypes + T1/T2/T3 type validation
├── metabolism/              The core metabolic engine (Phases 2-4)
│   ├── lesser_cycle.rs      M·P·C·E state machine (trusted anchor)
│   ├── greater_cycle.rs     S·T·G·Ch state machine (vertical ascent)
│   ├── attractor.rs         A(H) = ⟨A_M, A_P, A_G, Γ⟩
│   ├── health.rs            G_z, P_z, Resonance R(H1, H2)
│   └── worker.rs            Tier 2 async job pool + pending_metabolism queue
├── context/                 Agent API (Phase 5)
│   ├── context_pack.rs      ContextPack (single-call intra+inter+extra)
│   └── validation.rs        5-Gate Validation (epistemic enforcement)
├── mcp/                     MCP server (45 tools)
│   ├── tools.rs             Tool implementations (#[tool_router])
│   ├── helpers.rs           Connection management, error conversion, path validation
│   ├── synthesis_helpers.rs LLM provider chain, pattern synthesis, storage
│   ├── params.rs            Tool parameter structs
│   ├── server.rs            stdio + HTTP/SSE transport
│   ├── health.rs            HealthMonitor + circuit breakers
│   └── trust.rs             Agent trust store
├── db/                      SQLite persistence (WAL, FTS5, connection pool)
├── flow.rs                  Drive propagation engine
├── mind/                    Mind pipeline (consolidation, reflect, terrain, diagnostic)
├── maintenance/             Janitor, enricher, archiver, monitor, orchestrator
├── grammar/                 Node blueprints + auto-wiring
├── plugins/                 Entity extractor, hybrid retriever, preference extractor
├── llm/                     LLM trait + OpenAI/Anthropic/Ollama providers
└── ...                      audit, circuit_breaker, digestion, telearchy, etc.
```

## MCP Tools (45)

| Category | Tools |
|---|---|
| **Search** | `tdg_search`, `tdg_prefetch` |
| **CRUD** | `tdg_create`, `tdg_update`, `tdg_get_node`, `tdg_bulk_create`, `tdg_observe`, `tdg_record_exec` |
| **Edges** | `tdg_connect`, `tdg_get_related` |
| **Events** | `tdg_query_events` |
| **Rating** | `tdg_rate_memory` |
| **Mind** | `tdg_mind_state`, `tdg_context`, `tdg_consolidate` |
| **Synthesis** | `tdg_reflect`, `tdg_reflect_run` |
| **Trust** | `tdg_get_trust`, `tdg_adjust_trust`, `tdg_health_check` |
| **Health** | `tdg_system_health`, `tdg_graph_health`, `tdg_graph_stats` |
| **Schema** | `tdg_get_schema` |
| **Multi-agent** | `tdg_bank` |
| **Entities** | `tdg_entity` |
| **Maintenance** | `tdg_maintenance`, `tdg_enrich`, `tdg_self_manage`, `tdg_renormalize` |
| **Audit** | `tdg_audit` |
| **Persistence** | `tdg_save_mind_state`, `tdg_load_mind_state`, `tdg_get_project_context`, `tdg_set_project_context` |
| **Import/Export** | `tdg_export`, `tdg_import` |
| **Phase 1: Status** | `tdg_elevate` (human-only synthesis status elevation) |
| **Phase 2: Metabolism** | `tdg_tick`, `tdg_metabolism_status` |
| **Phase 3: Attractor** | `tdg_attractor`, `tdg_health`, `tdg_resonance`, `tdg_resonance_partners` |
| **Phase 4: Greater Cycle** | `tdg_greater_cycle` |
| **Phase 5: Context** | `tdg_fetch_context`, `tdg_submit_synthesis`, `tdg_validate_synthesis` |
| **Phase 6: Types** | `tdg_archetypes`, `tdg_validate_type` |

## TDG Invariants (20/20 embodied)

All 20 non-negotiable holonic-science invariants are now enforced in code:

1. ✅ Dual-cycle (lesser M·P·C·E + greater S·T·G·Ch) through shared contact boundary
2. ✅ Cosmological scope (scale codes S00-S80, Tetra-Axes)
3. ✅ Type ⊥ Stage orthogonality (T2 test + check_type_stage_orthogonality)
4. ✅ Status ladder (ai-draft → canonical-hypothesis → canonical → superseded)
5. ✅ 5-Gate Validation (Grounding, Failure-mode, Joint, Cosmological, Provenance)
6. ✅ Provenance on every semantic write (mutation_log + events)
7. ✅ Witnesses corroborate (EVIDENCES edges, canonical/hypothesis grounding)
8. ✅ Fractal recursion (Holon newtype, sub_holons via DECOMPOSES_TO)
9. ✅ Structural mirroring (lesser↔greater cycle topology)
10. ✅ Invariant vs decoration (cosmological scope gate)
11. ✅ Open joints labeled (joint validation gate)
12. ✅ Intelligent infinity source (catalyst generation at boundaries)
13. ✅ Octave-closure (greater cycle octave_count, stage advancement via Transformation)
14. ✅ Involution ≠ realm-descent (octave_id vs scale_code)
15. ✅ CLI/MCP parity (all operations exposed as MCP tools)
16. ✅ Lesser cycle is open (catalyst from outside, experience accumulates)
17. ✅ Both G_z AND P_z (health metrics, sinkhole detection)
18. ✅ Bonding at greater-cycle surface (resonance_graph, type_class)
19. ✅ Choice disambiguates noble (choice_flag: graduated/sinkhole/reopened)
20. ✅ 22 named archetypes (7 roles × 3 complexes + Choice)

## Memory Footprint (2GB VPS lean profile)

| Component | Size |
|-----------|------|
| SQLite DB (100K nodes) | ~600 MB |
| `lesser_cycle_json` + `greater_cycle_json` | ~450 bytes/holon |
| `attractor_field_json` + `health_json` | ~400 bytes/holon |
| `resonance_graph` (top-10 partners) | ~10 MB |
| `pending_metabolism` queue | ~1 MB typical |
| Worker pool (1 worker + 1 connection) | ~10 MB |
| **Total Phase 1-6 overhead** | **~50-70 MB** above pre-refactor baseline |

## Background Schedulers

| Schedule | Interval | What it does |
|----------|----------|-------------|
| SelfManager | 6h (`TDG_MAINTENANCE_INTERVAL_SECS`) | Janitor + Enricher + Archiver + Telearchy |
| Health check | 5m (`TDG_HEALTH_CHECK_INTERVAL_SECS`) | Internal DB health probe |
| Greater-cycle sweep | 10m (`TDG_GREATER_CYCLE_INTERVAL_SECS`) | Enqueue GreaterTick for holons with pressure |
| Metabolism worker | continuous | Process pending_metabolism jobs (1 worker default, `TDG_METABOLISM_WORKERS`) |

## Deployment

```bash
# Build for VPS (Debian 12, glibc 2.36)
export ORT_LIB_LOCATION=/tmp/onnxruntime-linux-x64-1.20.1/lib
export ORT_PREFER_DYNAMIC_LINK=1
cargo zigbuild --release --features onnx --target x86_64-unknown-linux-gnu.2.36

# Deploy
scp target/x86_64-unknown-linux-gnu/release/tdg-rust nerd@racknerd:~/tdg-rust
ssh nerd@racknerd "cp ~/tdg-rust ~/.hermes/tdg-rust/tdg-rust && chmod +x ~/.hermes/tdg-rust/tdg-rust"
ssh nerd@racknerd "export TDG_HOME=/home/nerd/.hermes && ~/.hermes/tdg-rust/tdg-rust migrate"
```

## Configuration

```yaml
# tdg.yaml
embedding:
  model: gemma        # or minilm
  quantization: q4    # q4 or q8
  dimension: 768      # 768 for gemma, 384 for minilm
```

Environment variables: `TDG_HOME`, `TDG_DB_PATH`, `TDG_STATE_DIR`, `TDG_SKILLS_DIR`, `TDG_LEAN`, `TDG_AGENT_NAME`, `TDG_METABOLISM_WORKERS`, `TDG_GREATER_CYCLE_INTERVAL_SECS`, `TDG_MAINTENANCE_INTERVAL_SECS`, `TDG_HEALTH_CHECK_INTERVAL_SECS`

---

*Last updated 2026-07-03. This document is the single source of truth for the current codebase state.*
