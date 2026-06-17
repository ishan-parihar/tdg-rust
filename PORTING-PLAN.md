# TDG-Rust Porting Plan

## Scope

**Complete standalone Rust implementation of TDG.** Everything from the Python codebase gets ported. No hybrid architecture. No Python dependency. The Rust binary replaces the Python system entirely.

**Full port includes:**
- Core CRUD (nodes, edges, batch operations)
- FTS5 full-text search
- Vector similarity search (cosine)
- MCP HTTP/SSE transport layer
- Event store (temporal reconstruction)
- Connection pooling
- Input validation
- Configuration management
- Statistics and rating
- HRR compositional algebra (1024-dim phase vectors — replace numpy with `ndarray`)
- Holonic self-model and traversal
- 16-cell drive matrix
- Flow engine (drive state propagation, polarity, entropy)
- Knowledge engine (catalyst lifecycle, archival, orphan detection, hygiene)
- Mind injection pipeline
- LLM reflection synthesis (Ollama integration)
- Diagnostic engine
- Metrics engine
- Pulse engine
- Feeling engine
- Override engine
- Project tracker
- Consolidation engine
- All plugins (entity extraction, hybrid retrieval, preference extraction, turn capture)
- All CLI scripts (graph operations, dream, knowledge management, migration, maintenance)

## Architecture

Single Rust binary. SQLite database (WAL mode). Axum HTTP/SSE server for MCP transport. No Python. No external runtime dependencies beyond SQLite and Ollama (for LLM reflection).

---

## Phase 1: Core Infrastructure

**Goal**: Config, connection pool, schema, error types, models, validation. Everything else depends on this.

### Tasks

| # | Task | Details | Python Source |
|---|------|---------|---------------|
| 1.1 | Project structure | Create `src/` module tree: `config.rs`, `db/mod.rs`, `db/pool.rs`, `db/schema.rs`, `db/migrations.rs`, `error.rs`, `models.rs`, `validation.rs` | `src/` |
| 1.2 | Config module | Port `TDGConfig` → Rust `Config` struct with `serde::Deserialize`. Support env vars: `TDG_HOME`, `TDG_DB_PATH`, `TDG_STATE_DIR`, `TDG_SKILLS_DIR`, `TDG_LEAN`. Default `~/.hermes/tdg/graph.db` | `core/config.py` |
| 1.3 | Error types | Define `TdgError` enum with `thiserror`: `Sqlite`, `Io`, `Validation`, `NotFound`, `PoolExhausted`, `BusyTimeout`, `SchemaMigration`, `Json`, `Hrr`, `Ollama`. Implement `From` conversions | `core/circuit_breaker.py` |
| 1.4 | Connection pool | Port Python `ConnectionPool` (queue.Queue, max 5) → Rust pool using `r2d2` + `r2d2_sqlite`. PRAGMA setup: `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `cache_size=-8000`, configurable `busy_timeout`. Include `backup()` | `core/graph_db.py:91-260` |
| 1.5 | Schema init | Port `init_schema()` → Rust `CREATE TABLE IF NOT EXISTS` for nodes, edges, events, embeddings. Port `init_fts()` → FTS5 virtual table. Port `migrate()` + `migrate_v3()` + `migrate_v4()` → Rust migration runner | `core/graph_db.py:450-750` |
| 1.6 | Input validation | Port constants: `MAX_TEXT_LENGTH=50000`, `MAX_NODE_ID_LENGTH=256`, `MAX_ALIASES=100`, `MAX_LIMIT=1000`, `MAX_TURNS=500`, `MAX_BULK_NODES=500`. Implement validators | `core/grammar/tdg_node_validation.py` |
| 1.7 | Models | Define `Node`, `Edge`, `Event`, `Embedding`, `DriveState`, `DualPoleDrive`, `DriveVector` structs with `serde::Serialize/Deserialize`. Match Python dict keys exactly | `core/graph_db.py`, `core/flow/tdg_flow_engine.py` |
| 1.8 | Circuit breaker | Port `CircuitBreaker` class: failure counting, cooldown period, half-open state | `core/circuit_breaker.py` |

### Success Criteria
- `cargo build` passes
- `cargo test` passes
- Schema creates all tables/indexes on first run
- Connection pool opens/closes/backs up cleanly

---

## Phase 2: CRUD Operations

**Goal**: All node/edge create/read/update/delete + batch operations.

### Tasks

| # | Task | Details | Python Source |
|---|------|---------|---------------|
| 2.1 | `add_node` | Insert with auto-generated ID (`n` + uuid4 hex[:12]), timestamps, JSON serialization | `graph_db.py:767-807` |
| 2.2 | `get_node` | SELECT by ID, deserialize JSON fields, return `Option<Node>` | `graph_db.py:835-843` |
| 2.3 | `update_node` | Dynamic UPDATE with optional fields, auto-set `updated_at` | `graph_db.py:808-834` |
| 2.4 | `delete_node` | Soft-delete: set `valid_to = now()`, also soft-delete connected edges | `graph_db.py:844-857` |
| 2.5 | `hard_delete_node` | Actually remove from DB | `graph_db.py:858-870` |
| 2.6 | `add_edge` | Insert with auto-generated ID (`e` + uuid4 hex[:12]), timestamps, weight, properties | `graph_db.py:894-954` |
| 2.7 | `get_edges` | Query by source_id, target_id, edge_type, optional `include_deleted` | `graph_db.py:956-981` |
| 2.8 | `delete_edge` | Soft-delete: set `valid_to = now()` | `graph_db.py:982-995` |
| 2.9 | `update_edge` | Update weight and/or properties | `graph_db.py:996-1021` |
| 2.10 | Batch operations | `add_nodes_batch`, `add_edges_batch` using `executemany` with transactions | `graph_db.py:1022-1093` |
| 2.11 | `count_nodes`, `count_edges` | COUNT queries with optional type filter | `graph_db.py:871-893` |

---

## Phase 3: Query Engine

**Goal**: Search, traversal, graph analysis.

### Tasks

| # | Task | Details | Python Source |
|---|------|---------|---------------|
| 3.1 | `query_nodes` | Filter by node_type, lifecycle_state, source, valid_to, agent_id. Pagination. `include_deleted` flag | `graph_db.py:1124-1166` |
| 3.2 | `search` (FTS5) | Full-text search using `nodes_fts`. BM25 ranking. Top N with scores | `graph_db.py:1167-1196` |
| 3.3 | `search_hybrid` | Combine FTS5 + vector similarity. Weighted: `score = alpha * fts + beta * cosine` | `graph_db.py:1237-1254` |
| 3.4 | `search_similar` | Cosine similarity against embeddings table. Brute-force scan | `graph_db.py:1255-1322` |
| 3.5 | `pathfind` | BFS shortest path. Adjacency from active edges. Returns path as list of node dicts | `graph_db.py:1197-1236` |
| 3.6 | `node_graph` | Recursive BFS neighborhood expansion up to depth, max_nodes limit | `graph_db.py:1357-1395` |
| 3.7 | Holonic traversal | `get_neighbors`, `get_parents`, `get_children`, `get_siblings`, `get_by_depth`, `get_depths`, `get_holonic_path`, `get_agent_path`, `get_containment_depth`, `get_peers`, `holographic_view` | `graph_db.py:1094-1615` |
| 3.8 | Backfill helpers | `backfill_parent_ids`, `backfill_agent_path` | `graph_db.py:1703-1751` |

---

## Phase 4: Event Store, Rating & Banks

**Goal**: Event sourcing, trust scoring, memory rating, bank management.

### Tasks

| # | Task | Details | Python Source |
|---|------|---------|---------------|
| 4.1 | `record_event` | Insert into events table with auto-generated event_id, timestamp, optional node_id, payload_json | `graph_db.py:742-766` |
| 4.2 | `rate_node` | Increment helpful_count. Recompute trust_score: `_compute_trust(confidence, helpful, retrieval)` | `graph_db.py:1825-1881` |
| 4.3 | `record_retrieval` | Increment retrieval_count | `graph_db.py:1849-1855` |
| 4.4 | `get_trust_score` | Compute trust from confidence, helpful, retrieval counts | `graph_db.py:1856-1872` |
| 4.5 | `list_by_trust` | Query nodes sorted by trust score descending | `graph_db.py:1882-1920` |
| 4.6 | BankManager | `set_context`, `get_current_bank_id`, `tag_node`, `get_bank_nodes`, `get_bank_stats`, `list_banks`, `tag_node_on_write` | `graph_db.py:1921-2026` |
| 4.7 | `stats` | Aggregate stats: node counts by type, edge counts, total, FTS index size | `graph_db.py:1323-1356` |

---

## Phase 5: HRR Compositional Algebra

**Goal**: Replace numpy with `ndarray`. Full HRR implementation in Rust.

### Tasks

| # | Task | Details | Python Source |
|---|------|---------|---------------|
| 5.1 | Core HRR | `phase_encode`, `bind`, `unbind`, `bundle`, `cosine_similarity`, `normalize`. 1024-dim vectors. Use `ndarray` crate | `core/hrr.py` |
| 5.2 | HRR retriever | `probe`, `related`, `reason`, `contradict`. Memory banks (numpy → ndarray) | `core/hrretriever.py` |
| 5.3 | Embedding engine | ONNX runtime integration for MiniLM-L6-v2 quantized model. `embed`, `embed_batch`, cosine similarity | `core/mind/embedding_engine.py` |
| 5.4 | Serialization | `serialize_embedding` (f32 → bytes), `deserialize_embedding` | `core/mind/embedding_engine.py:274-289` |

### Dependencies
- `ndarray` for vector operations
- `ort` (ONNX Runtime) for embedding model inference

---

## Phase 6: Flow Engine

**Goal**: Drive state propagation, polarity diagnostics, graph entropy.

### Tasks

| # | Task | Details | Python Source |
|---|------|---------|---------------|
| 6.1 | Drive models | `DriveState`, `DualPoleDrive`, `DriveVector` structs. Intrinsic signatures per node_type | `core/flow/tdg_flow_engine.py:1-115` |
| 6.2 | `emit_downward` | Propagate drive states from parent to children. Stabilization logic | `core/flow/tdg_flow_engine.py:286-398` |
| 6.3 | `aggregate_upward` | Aggregate child drive states back to parent | `core/flow/tdg_flow_engine.py:457-536` |
| 6.4 | `renormalize_graph` | Reset all nodes to intrinsic drive signatures | `core/flow/tdg_flow_engine.py:537-594` |
| 6.5 | `diagnose_polarity` | Detect polarity imbalances across the graph | `core/flow/tdg_flow_engine.py:595-660` |
| 6.6 | `compute_graph_entropy` | Shannon entropy of drive distribution | `core/flow/tdg_flow_engine.py:661-712` |

---

## Phase 7: Knowledge Engine

**Goal**: Catalyst lifecycle, archival, orphan detection, hygiene.

### Tasks

| # | Task | Details | Python Source |
|---|------|---------|---------------|
| 7.1 | Catalyst models | `CatalystProfile`, `CatalystType`, `DecayPolicy`, `HygieneReport` structs | `core/knowledge/tdg_knowledge_engine.py:1-175` |
| 7.2 | `classify_catalyst` | Infer catalyst type from node data, compute archive_after timestamp | `core/knowledge/tdg_knowledge_engine.py:297-417` |
| 7.3 | `link_catalyst_to_structure` | Connect catalyst nodes to knowledge graph structure | `core/knowledge/tdg_knowledge_engine.py:418-506` |
| 7.4 | `evaluate_integration_quality` | Score how well a node is integrated into the graph | `core/knowledge/tdg_knowledge_engine.py:507-634` |
| 7.5 | `archive_stale_nodes` | Archive nodes past their archive_after date | `core/knowledge/tdg_knowledge_engine.py:635-732` |
| 7.6 | `detect_orphans` | Find nodes with no edges | `core/knowledge/tdg_knowledge_engine.py:838-938` |
| 7.7 | `prune_dangling_edges` | Remove edges pointing to deleted nodes | `core/knowledge/tdg_knowledge_engine.py:939-977` |
| 7.8 | `enforce_observation_lifecycle` | Promote observations through lifecycle stages | `core/knowledge/tdg_knowledge_engine.py:978-1037` |
| 7.9 | `generate_hygiene_report` | Full hygiene analysis | `core/knowledge/tdg_knowledge_engine.py:1038-1170` |
| 7.10 | `process_catalyst_lifecycle` | End-to-end catalyst processing | `core/knowledge/tdg_knowledge_engine.py:1171-1211` |
| 7.11 | `reverse_archival` | Restore archived nodes | `core/knowledge/tdg_knowledge_engine.py:1246-1300` |

---

## Phase 8: Mind Injection Pipeline

**Goal**: Complete mind state generation, diagnostics, metrics, feelings.

### Tasks

| # | Task | Details | Python Source |
|---|------|---------|---------------|
| 8.1 | Diagnostic engine | `DiagnosticEngine::analyze()`: drive pattern analysis, phantom detection, consecutive drive/quadrant detection | `core/mind/diagnostic_engine.py` |
| 8.2 | Metrics engine | `MetricsEngine`: cycle recording, lead tracking, wisdom detection, 24h allocation, drive distribution, freshness checks | `core/mind/metrics_engine.py` |
| 8.3 | Feeling engine | `FeelingEngine::generate()`: drive state extraction, energy level, stuck pattern detection, metric feelings, summary | `core/mind/feeling_engine.py` |
| 8.4 | Override engine | `OverrideEngine::generate()`: context-aware overrides | `core/mind/override_engine.py` |
| 8.5 | Project tracker | `ProjectTracker`: create, update_phase, advance_phase, get_status, list_active, mark_deferred | `core/mind/project_tracker.py` |
| 8.6 | Pulse engine | `pulse()`: node pulse analysis, pattern classification, gap detection | `core/mind/pulse_engine.py` |
| 8.7 | Terrain engine | `generate_terrain_context()`: social terrain, skill discovery | `core/mind/terrain.py` |
| 8.8 | Sections generator | `generate_pulse_section()`, `generate_social_terrain_section()`, `generate_revenue_urgency_section()`, `generate_sensory_field()` | `core/mind/sections.py` |
| 8.9 | Data loader | `load_meta_view()`, `load_drive_matrix()`, `load_constraints()`, `load_working_memory()`, `load_loop_state()`, `load_polarity()`, `load_hygiene()`, `load_micro_slice()` | `core/mind/data_loader.py` |
| 8.10 | Consolidation engine | `run()`: health snapshot, graph consolidation | `core/mind/consolidation_engine.py` |
| 8.11 | Reflect engine | `run()`: cluster-based reflection, LLM integration via Ollama | `core/mind/reflect_engine.py` |
| 8.12 | Injector | `generate_prompt()`, `write_mind_state_file()`: full mind state assembly | `core/mind/injector.py` |

---

## Phase 9: tdg_ops & tdg_impl

**Goal**: High-level operations, CLI commands, facade class.

### Tasks

| # | Task | Details | Python Source |
|---|------|---------|---------------|
| 9.1 | Reconcile | `reconcile()`: drive matrix reconciliation, orphan cleanup, edge pruning | `core/tdg_ops.py:42-256` |
| 9.2 | Micro/macro slice | `micro_slice()`: current quadrant focus. `macro_slice()`: depth-based overview | `core/tdg_ops.py:257-535` |
| 9.3 | Record action | `record_action()`: capture actions with quadrant and entities | `core/tdg_ops.py:351-448` |
| 9.4 | Flow up | `flow_up()`: upward drive propagation | `core/tdg_ops.py:449-488` |
| 9.5 | Polarity/hygiene | `polarity()`, `hygiene()`: graph health diagnostics | `core/tdg_ops.py:489-506` |
| 9.6 | Stage status | `stage_status()`: development stage tracking | `core/tdg_ops.py:536-556` |
| 9.7 | Drive matrix report | `drive_matrix_report()`: full 16-cell matrix output | `core/tdg_ops.py:650-690` |
| 9.8 | TDG facade | `TDG` struct: `status()`, `run_cycle()`, `get_registry()` | `core/tdg_impl.py` |
| 9.9 | CLI commands | `cmd_graph`, `cmd_dream`, `cmd_knowledge`, `migrate_to_v3` | `core/tdg_ops.py:691-896` |

---

## Phase 10: Plugins

**Goal**: Port all plugin functionality.

### Tasks

| # | Task | Details | Python Source |
|---|------|---------|---------------|
| 10.1 | Entity extractor | `EntityExtractor`: NER from text, entity linking | `plugins/tdg/entity_extractor.py` |
| 10.2 | Hybrid retriever | `HybridRetriever`: combine FTS5 + HRR + embedding similarity | `plugins/tdg/hybrid_retriever.py` |
| 10.3 | Preference extractor | `PreferenceExtractor`: learn user preferences from interactions | `plugins/tdg/preference_extractor.py` |
| 10.4 | Turn capture | `TurnCapture`: capture conversation turns into graph events | `plugins/tdg/turn_capture.py` |
| 10.5 | Mind state plugin | `mind_state.py`: mind state formatting for prompts | `plugins/tdg/mind_state.py` |
| 10.6 | Reflect tool plugin | `reflect_tool.py`: reflection with LLM integration | `plugins/tdg/reflect_tool.py` |

---

## Phase 11: MCP Transport & CLI

**Goal**: Axum HTTP/SSE server, all 17 MCP tools, full CLI.

### Tasks

| # | Task | Details | Python Source |
|---|------|---------|---------------|
| 11.1 | Axum server | HTTP server: `POST /mcp` (JSON-RPC), `GET /sse` (SSE stream), `POST /tools/{name}` (REST fallback). CORS | `mcp/tdg_mcp_server.py` |
| 11.2 | MCP protocol | JSON-RPC 2.0: `initialize`, `tools/list`, `tools/call` | `mcp/_shared.py` |
| 11.3 | Tool definitions | All 17 tools with JSON Schema parameters | `mcp/tools/*.py` |
| 11.4 | Tool implementations | Port each tool to call GraphDB methods. Lean guard, validation, error responses | `mcp/tools/*.py` |
| 11.5 | Lean mode | `_lean_guard` — skip heavy operations when `TDG_LEAN=true` | `mcp/_ctx.py` |
| 11.6 | CLI | `tdg-rust serve`, `tdg-rust migrate`, `tdg-rust backup`, `tdg-rust stats`, `tdg-rust dream`, `tdg-rust graph`, `tdg-rust knowledge` | `core/tdg_ops.py:main()` |

### Tool Mapping (17 tools)

| Python Tool | Rust Function | Module |
|-------------|---------------|--------|
| `tdg_bank` | `bank_action()` | `tools/banks.rs` |
| `tdg_search` | `search()` | `tools/core.rs` |
| `tdg_get_node` | `get_node()` | `tools/core.rs` |
| `tdg_query_events` | `query_events()` | `tools/core.rs` |
| `tdg_entity` | `entity_resolve()` | `tools/entity.rs` |
| `tdg_mind_state` | `mind_state()` | `tools/mind.rs` |
| `tdg_observe` | `observe()` | `tools/mind.rs` |
| `tdg_get_related` | `get_related()` | `tools/mind.rs` |
| `tdg_reflect` | `reflect()` | `tools/reflect.rs` |
| `tdg_maintenance` | `maintenance()` | `tools/utility.rs` |
| `tdg_get_schema` | `get_schema()` | `tools/utility.rs` |
| `tdg_create` | `create()` | `tools/write.rs` |
| `tdg_update` | `update()` | `tools/write.rs` |
| `tdg_connect` | `connect()` | `tools/write.rs` |
| `tdg_bulk_create` | `bulk_create()` | `tools/write.rs` |
| `tdg_record_exec` | `record_exec()` | `tools/write.rs` |
| `tdg_rate_memory` | `rate_memory()` | `tools/write.rs` |

---

## Phase 12: Scripts & Utilities

**Goal**: Port all CLI scripts.

### Tasks

| # | Task | Details | Python Source |
|---|------|---------|---------------|
| 12.1 | Audit integration | `audit_integration.py` → `tdg-rust audit` | `scripts/audit_integration.py` |
| 12.2 | Check constraints | `check_constraints.py` → `tdg-rust check` | `scripts/check_constraints.py` |
| 12.3 | Persistence unifier | `persistence_unifier.py` → `tdg-rust unify` | `scripts/persistence_unifier.py` |
| 12.4 | Reconcile constraints | `reconcile_constraints_v2.py` → `tdg-rust reconcile-constraints` | `scripts/reconcile_constraints_v2.py` |
| 12.5 | Sync skills | `sync_skills_to_tdg.py` → `tdg-rust sync-skills` | `scripts/sync_skills_to_tdg.py` |
| 12.6 | Auto capture | `tdg_auto_capture.py` → `tdg-rust auto-capture` | `scripts/tdg_auto_capture.py` |
| 12.7 | Create nodes | `tdg_create.py` → `tdg-rust create` | `scripts/tdg_create.py` |
| 12.8 | Embed backfill | `tdg_embed_backfill.py` → `tdg-rust backfill-embeddings` | `scripts/tdg_embed_backfill.py` |
| 12.9 | Maintenance check | `tdg_maintenance_check.py` → `tdg-rust maintenance-check` | `scripts/tdg_maintenance_check.py` |
| 12.10 | Repair orphans | `tdg_repair_orphans.py` → `tdg-rust repair-orphans` | `scripts/tdg_repair_orphans.py` |

---

## Phase 13: Testing & Validation

**Goal**: Ensure Rust implementation matches Python behavior.

### Tasks

| # | Task | Details |
|---|------|---------|
| 13.1 | Unit tests | Each module has `#[cfg(test)]`. Target: 500+ tests |
| 13.2 | Integration tests | Full CRUD + query + event + flow + knowledge workflows |
| 13.3 | Python comparison | Export Python test fixtures to JSON, load in Rust, compare results |
| 13.4 | Benchmarks | `criterion` benchmarks for: add_node, search, pathfind, batch insert, HRR bind/unbind |
| 13.5 | Fuzz testing | `cargo-fuzz` for input validation and SQL construction |
| 13.6 | CI pipeline | GitHub Actions: build, test, clippy, fmt, benchmarks |

---

## Parallel Execution Opportunities

### Phase 1: Sequential (foundation)
### Phase 2: Sequential (builds on Phase 1)
### Phase 3: Sequential (builds on Phase 2)
### Phase 4: Sequential (builds on Phase 2)

### Phases 5-8 can parallelize after Phase 4:
- Phase 5 (HRR) — independent
- Phase 6 (Flow) — needs Phase 4 for events
- Phase 7 (Knowledge) — needs Phase 4 for events
- Phase 8 (Mind) — needs Phases 5, 6, 7

### Phases 9-12 sequential after Phase 8:
- Phase 9 (tdg_ops) — needs all above
- Phase 10 (Plugins) — needs all above
- Phase 11 (MCP) — needs all above
- Phase 12 (Scripts) — needs all above

### Phase 13: After everything

---

## New Cargo.toml Dependencies

```toml
[dependencies]
# Core
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Database
rusqlite = { version = "0.32", features = ["bundled", "backup"] }
r2d2 = "0.8"
r2d2_sqlite = "0.24"
filelock = "3"

# HTTP/SSE (MCP transport)
axum = { version = "0.8", features = ["json"] }
tower-http = { version = "0.6", features = ["cors", "trace"] }
tokio-stream = "0.1"
futures = "0.3"

# Linear algebra (HRR)
ndarray = { version = "0.16", features = ["serde"] }

# ONNX Runtime (embeddings)
ort = { version = "2", features = ["download-binaries"] }

# Date/time
chrono = { version = "0.4", features = ["serde"] }

# UUID
uuid = { version = "1", features = ["v4", "serde"] }

# CLI
clap = { version = "4", features = ["derive"] }

# Environment
dotenvy = "0.15"

# Hashing (for vector similarity)
ahash = "0.8"

# LLM (Ollama)
reqwest = { version = "0.12", features = ["json"] }

# Testing/Benchmarks
criterion = { version = "0.5", features = ["html_reports"] }
tempfile = "3"
```

---

## Wire Compatibility

All JSON responses must match Python format exactly:
- Node dict keys: `id`, `node_type`, `name`, `description`, `properties`, `quadrants`, `drives`, `lifecycle_state`, `teleological_level`, `developmental_stage`, `confidence`, `source`, `parent_ids`, `agent_path`, `created_at`, `updated_at`, `valid_from`, `valid_to`, `helpful_count`, `retrieval_count`, `agent_id`
- Edge dict keys: `id`, `source_id`, `target_id`, `edge_type`, `weight`, `properties`, `valid_from`, `valid_to`, `created_at`, `updated_at`, `agent_id`
- Event dict keys: `event_id`, `event_action`, `node_id`, `payload_json`, `timestamp`
- All timestamps in ISO 8601 format
- JSON fields serialized as JSON strings (not nested objects)

---

## Estimated Timeline

| Phase | Duration | Dependencies |
|-------|----------|--------------|
| 1: Core Infrastructure | Week 1-2 | None |
| 2: CRUD Operations | Week 2-3 | Phase 1 |
| 3: Query Engine | Week 3-4 | Phase 2 |
| 4: Event Store & Rating | Week 3-4 | Phase 2 |
| 5: HRR Algebra | Week 4-5 | Phase 1 |
| 6: Flow Engine | Week 5-6 | Phase 4 |
| 7: Knowledge Engine | Week 5-6 | Phase 4 |
| 8: Mind Pipeline | Week 6-8 | Phases 5,6,7 |
| 9: tdg_ops & Facade | Week 8-9 | Phase 8 |
| 10: Plugins | Week 8-9 | Phase 8 |
| 11: MCP Transport | Week 9-10 | Phase 9 |
| 12: Scripts | Week 10 | Phase 9 |
| 13: Testing & Validation | Week 10-12 | All |

**Total**: ~12 weeks for complete standalone Rust TDG.

---

## Risk Mitigation

1. **HRR correctness**: Port numpy operations to ndarray with exact same math. Validate with test vectors from Python.
2.2 **ONNX inference**: `ort` crate handles model loading. Test with same MiniLM model.
3. **FTS5**: `rusqlite` bundled includes FTS5. No external dependency.
4. **Vector search**: Brute-force cosine for <100K nodes. Profile and add HNSW if needed.
5. **MCP protocol**: Follow official spec. JSON-RPC 2.0. SSE for subscriptions.
6. **Wire compatibility**: Export Python test fixtures, compare JSON output exactly.

---
---

# PART 2: COMPLETED PHASES & REMAINING WORK

> **Last updated**: 2026-06-18
> **Current state**: Phases 1-13 COMPLETE (365 tests, 0 failures). Phases 14-19 REMAINING.

---

## Completed Phase Status

| Phase | Status | Tests | Notes |
|-------|--------|-------|-------|
| 1: Core Infrastructure | ✅ COMPLETE | — | config, pool, schema, error, models, validation |
| 2: CRUD Operations | ✅ COMPLETE | — | Full CRUD + batch ops in db/crud.rs (1380 LOC) |
| 3: Query Engine | ✅ COMPLETE | — | FTS5, cosine similarity, BFS pathfind, node_graph |
| 4: Event Store & Rating | ✅ COMPLETE | — | db/events.rs (331 LOC), trust scoring |
| 5: HRR Algebra | ✅ COMPLETE | 12 proptest | ndarray-based, 1024-dim, bind/unbind/bundle |
| 6: Flow Engine | ✅ COMPLETE | — | 3-stage pipeline, polarity, entropy (994 LOC) |
| 7: Knowledge Engine | ✅ COMPLETE | — | Catalyst lifecycle, archival, hygiene (1146 LOC) |
| 8: Mind Pipeline | ✅ PARTIAL | — | diagnostic, feeling, metrics, project_tracker, pulse, reflect_engine, consolidation_engine |
| 9: tdg_ops & Facade | ✅ COMPLETE | — | ops.rs (697 LOC), reconcile, micro_slice, record_action |
| 10: Plugins | ✅ COMPLETE | — | entity_extractor, hybrid_retriever, preference_extractor, turn_capture |
| 11: MCP Transport | ✅ COMPLETE | — | 17 tools, axum server, SSE transport |
| 12: Scripts | ✅ COMPLETE | — | 10 CLI commands (464 LOC) |
| 13: Testing | ✅ COMPLETE | 365 | 180 lib + 144 main + 8 integration + 12 proptest + 21 scripts |
| **14: Engine Parity** | ⬜ PENDING | — | **NEW — see below** |
| **15: Mind Pipeline** | ⬜ PENDING | — | **NEW — see below** |
| **16: Event Sourcing** | ⬜ PENDING | — | **NEW — see below** |
| **17: Audit Engine** | ⬜ PENDING | — | **NEW — see below** |
| **18: Graph Projection** | ⬜ PENDING | — | **NEW — see below** |
| **19: Polish & Release** | ⬜ PENDING | — | **NEW — see below** |

---

## Rust Module Inventory (35 files, 14,160 LOC)

```
src/
├── lib.rs (40 LOC)              — Module declarations + re-exports
├── main.rs (314 LOC)            — CLI entry point (clap)
├── config.rs (165 LOC)          — TDGConfig, path properties
├── error.rs (49 LOC)            — TdgError enum (13 variants)
├── models.rs (286 LOC)          — Node, Edge, Event, Embedding, DriveState, NodeQuery
├── validation.rs (435 LOC)      — NodeContract, validate_node/edge_creation
├── schema.rs (356 LOC)          — Stage, TelosLevel, CatalystType, DigestionStatus, Quadrant
├── hrr.rs (243 LOC)             — HRR algebra (phase_encode, bind, unbind, bundle)
├── flow.rs (994 LOC)            — FlowDriveState, 3-stage pipeline, polarity, entropy
├── knowledge.rs (1146 LOC)      — Catalyst lifecycle, archival, hygiene
├── telearchy.rs (375 LOC)       — Stage-gated telos hierarchy, evidence collection
├── digestion.rs (348 LOC)       — Catalyst digestion pipeline
├── circuit_breaker.rs (354 LOC) — State machine, PreWriteSnapshot, TransactionSnapshot
├── ops.rs (697 LOC)             — Facade: reconcile, micro_slice, record_action, CLI dispatchers
├── db/
│   ├── mod.rs
│   ├── pool.rs (163 LOC)        — Connection pool
│   ├── schema.rs (375 LOC)      — CREATE TABLE, FTS5, triggers, migrations
│   ├── crud.rs (1380 LOC)       — Full CRUD + batch + query + search + pathfind
│   └── events.rs (331 LOC)      — Event store
├── mind/
│   ├── mod.rs
│   ├── diagnostic.rs (453 LOC)  — DiagnosticEngine, 8-step analysis pipeline
│   ├── feeling.rs (308 LOC)     — FeelingEngine, drive state extraction
│   ├── metrics.rs (317 LOC)     — MetricsEngine, cycle recording, lead tracking
│   ├── pulse.rs (443 LOC)       — PulseEngine, node pulse analysis
│   ├── project_tracker.rs (249 LOC) — ProjectTracker, phase management
│   ├── reflect_engine.rs (426 LOC)  — Entity clustering, SHA-256 fingerprints
│   └── consolidation_engine.rs (379 LOC) — 4-phase cross-session synthesis
├── grammar/
│   ├── mod.rs
│   ├── auto_wire.rs (245 LOC)   — Edge creation from NODE_CONTRACT rules
│   └── node_grammar.rs (376 LOC) — CatalystType→NodeBlueprint mapping
├── score/
│   └── mod.rs (352 LOC)         — 5-layer provenance-aware scoring
├── plugins/
│   ├── mod.rs
│   ├── entity_extractor.rs (282 LOC)
│   ├── hybrid_retriever.rs (407 LOC)
│   ├── preference_extractor.rs
│   └── turn_capture.rs
├── mcp/
│   ├── mod.rs
│   ├── server.rs (49 LOC)       — Axum MCP server
│   ├── tools.rs (550 LOC)       — 17 MCP tool implementations
│   └── tests.rs (382 LOC)       — MCP integration tests
└── scripts/
    └── mod.rs (464 LOC)          — 10 CLI commands
```

---

## Python Module Inventory (27 files, 17,982 LOC)

```
core/
├── tdg.py (35 LOC)              — Re-export facade
├── tdg_impl.py (583 LOC)        — TDG class, TDGPersistence, EventStore, SnapshotManager, ReplayEngine
├── tdg_ops.py (1005 LOC)        — 15+ operational commands, full CLI argparse
├── config.py (124 LOC)          — TDGConfig dataclass
├── graph_db.py (2026 LOC)       — GraphDB, ConnectionPool, CircuitBreaker, FileLock, CRUD
├── hrr.py (294 LOC)             — HRR algebra (numpy)
├── hrretriever.py (410 LOC)     — HRRRetriever, memory banks, SNR checks
├── circuit_breaker.py (285 LOC) — CircuitBreaker, PreWriteSnapshot, TransactionSnapshot
├── deprecation_registry.py (226 LOC) — Deprecated component tracking
├── schema/
│   └── canonical_schema.py (1087 LOC) — Stage, TelosLevel, NodeType, EdgeType, DriveDiagnosis, EdgeTypeContract
├── flow/
│   └── tdg_flow_engine.py (787 LOC) — 3-stage pipeline, quadrant modulators, channel-aware clipping
├── knowledge/
│   └── tdg_knowledge_engine.py (1416 LOC) — Catalyst lifecycle, archival index, full provenance
├── telearchy/
│   └── tdg_telearchy_engine.py (294 LOC) — Stage-gated telos hierarchy
├── digestion/
│   └── tdg_digestion_engine.py (262 LOC) — Catalyst digestion pipeline
├── score/
│   └── tdg_score_reconciler.py (395 LOC) — 5-layer provenance scoring
├── grammar/
│   ├── tdg_node_validation.py (492 LOC) — Node contracts, edge validation
│   ├── tdg_node_grammar.py (194 LOC) — CatalystType→NodeBlueprint
│   └── auto_wire.py (170 LOC) — Edge creation from contracts
├── audit/
│   ├── tdg_audit_engine.py (341 LOC) — 5-report audit system, AnomalyRegistry
│   └── deprecation_registry.py (226 LOC)
├── mind/
│   ├── diagnostic_engine.py (682 LOC) — 12-step pipeline, escalation, phantom detection
│   ├── feeling_engine.py (376 LOC)
│   ├── metrics_engine.py (549 LOC)
│   ├── pulse_engine.py (590 LOC)
│   ├── project_tracker.py (463 LOC)
│   ├── reflect_engine.py (367 LOC)
│   ├── consolidation_engine.py (406 LOC)
│   ├── override_engine.py (122 LOC) — DEPRECATED shim
│   ├── embedding_engine.py (299 LOC) — ONNX MiniLM-L6-v2
│   ├── injector.py (498 LOC) — Terrain-first 9-section context generator
│   ├── terrain.py (279 LOC) — Skill discovery, terrain context
│   ├── sections.py (427 LOC) — Revenue urgency, pulse, social terrain, sensory field
│   └── data_loader.py (193 LOC) — 9 state file loaders
└── persistence/
    └── (dual SQLite+NetworkX persistence layer)
```

---

## Gap Analysis Summary

### Features in Python, Missing in Rust

| # | Feature | Python LOC | Priority | Effort | Phase |
|---|---------|-----------|----------|--------|-------|
| 1 | Quadrant modulators (flow) | ~80 | HIGH | LOW | 14 |
| 2 | Channel-aware clipping (flow) | ~60 | MEDIUM | LOW | 14 |
| 3 | Archival index (knowledge) | ~100 | MEDIUM | LOW | 14 |
| 4 | Full provenance (knowledge) | ~80 | MEDIUM | LOW | 14 |
| 5 | Escalation ladder (diagnostic) | ~120 | HIGH | LOW | 14 |
| 6 | Phantom detection (diagnostic) | ~60 | MEDIUM | LOW | 14 |
| 7 | Config-driven thresholds (diagnostic) | ~40 | MEDIUM | LOW | 14 |
| 8 | meta_view command (ops) | ~200 | HIGH | MEDIUM | 14 |
| 9 | drive_matrix_report (ops) | ~80 | MEDIUM | LOW | 14 |
| 10 | Data loader (mind) | 193 | HIGH | LOW | 15 |
| 11 | Terrain engine (mind) | 279 | HIGH | MEDIUM | 15 |
| 12 | Sections generator (mind) | 427 | HIGH | MEDIUM | 15 |
| 13 | Mind injector (mind) | 498 | HIGH | MEDIUM | 15 |
| 14 | Event sourcing / JSONL journal | ~300 | MEDIUM | HIGH | 16 |
| 15 | SnapshotManager | ~150 | MEDIUM | MEDIUM | 16 |
| 16 | ReplayEngine | ~100 | LOW | MEDIUM | 16 |
| 17 | Audit engine (5 reports) | 341 | MEDIUM | MEDIUM | 17 |
| 18 | AnomalyRegistry | ~100 | MEDIUM | LOW | 17 |
| 19 | Deprecation registry | 226 | LOW | LOW | 17 |
| 20 | Graph projection (petgraph) | ~400 | LOW | HIGH | 18 |

### Features in Rust, Not in Python (Advantages)

| # | Feature | Rust Module | Benefit |
|---|---------|-------------|---------|
| 1 | MCP server (17 tools) | mcp/ | AI agent integration |
| 2 | Proptest fuzz testing | tests/proptest_fuzz.rs | Property-based verification |
| 3 | Type-safe error handling | error.rs | No runtime panics |
| 4 | 3x test coverage (365 vs ~120) | tests/ | Higher confidence |
| 5 | Zero-copy deserialization | models.rs | Better performance |
| 6 | Async I/O (Tokio) | main.rs | Non-blocking operations |

---

# PART 3: REMAINING DEVELOPMENT PHASES

---

## Phase 14: Engine Parity Fixes

**Goal**: Close functional gaps in flow, knowledge, diagnostic, and ops engines.
**Duration**: 1-2 days
**Dependencies**: Phases 1-13 complete

### Tasks

| # | Task | Details | Python Source | Effort |
|---|------|---------|---------------|--------|
| 14.1 | Add quadrant modulators to flow.rs | Add `QUADRANT_MODULATORS` constant (4×4 drive multipliers). Apply in `emit_downward()` — multiply drive contribution by quadrant modulator. Python: `flow.py:116-130` | `flow/tdg_flow_engine.py:116-130` | LOW |
| 14.2 | Add channel-aware clipping to flow.rs | Replace simplified `edge_flow_rate()` with `EDGE_TYPE_CONTRACTS` lookup (28 edge types with flow_rate, aggregation_weight). Python: `flow.py:131-200` | `flow/tdg_flow_engine.py:131-200` | LOW |
| 14.3 | Add event recording to flow.rs | Emit `DRIVE_RECOMPUTED` event after `receive_stabilize()`. Emit `GRAPH_RENORMALIZED` after `renormalize_graph()`. Python: `flow.py:400-420` | `flow/tdg_flow_engine.py:400-420` | LOW |
| 14.4 | Add archival index to knowledge.rs | Write archival records to disk JSON (not just DB). Add `ArchivalIndex` struct with `save()`/`load()` methods. Python: `knowledge.py:733-837` | `knowledge/tdg_knowledge_engine.py:733-837` | LOW |
| 14.5 | Align CatalystType variants | Python uses 8 keyword-based types (signal/event/insight/feedback/data_point/reflection/constraint_signal/opportunity_signal). Rust uses 6 (Signal/Insight/Feedback/Metric/Observation/Unknown). Add missing variants. Python: `knowledge.py:176-296` | `knowledge/tdg_knowledge_engine.py:176-296` | LOW |
| 14.6 | Add full provenance to ArchivalRecord | Capture edge_history and linkage_history in ArchivalRecord. Currently only records node_id/reason/timestamps. Python: `knowledge.py:59-120` | `knowledge/tdg_knowledge_engine.py:59-120` | LOW |
| 14.7 | Add escalation ladder to diagnostic.rs | Implement soft→strong→mandatory escalation with revenue correlation. Add `EscalationLevel` enum and `escalation_ladder()` method. Python: `diagnostic.py:200-350` | `mind/diagnostic_engine.py:200-350` | MEDIUM |
| 14.8 | Add phantom detection to diagnostic.rs | Detect eros drive in wrong quadrant. Add `detect_phantom()` method. Python: `diagnostic.py:351-400` | `mind/diagnostic_engine.py:351-400` | LOW |
| 14.9 | Add metrics staleness to diagnostic.rs | Check metrics lag (time since last metrics update). Add `check_metrics_staleness()` method. Python: `diagnostic.py:401-450` | `mind/diagnostic_engine.py:401-450` | LOW |
| 14.10 | Add config-driven thresholds to diagnostic.rs | Load DiagnosticThresholds from TOML config file instead of hardcoded defaults. Python: `diagnostic.py:1-80` (YAML loading) | `mind/diagnostic_engine.py:1-80` | LOW |
| 14.11 | Add meta_view command to ops.rs | Implement `meta_view()` — strategic landscape with drive diagnosis, telos hierarchy, constraint surface. Python: `tdg_ops.py:121-256` | `tdg_ops.py:121-256` | MEDIUM |
| 14.12 | Add drive_matrix_report to ops.rs | Implement `drive_matrix_report()` — full 16-cell matrix output. Python: `tdg_ops.py:650-690` | `tdg_ops.py:650-690` | LOW |
| 14.13 | Add stage_status to ops.rs | Implement `stage_status()` — development stage tracking. Python: `tdg_ops.py:536-556` | `tdg_ops.py:536-556` | LOW |
| 14.14 | Add lean mode to flow.rs | Check config lean flag for early returns in heavy operations. Python: `flow.py:537-545` | `flow/tdg_flow_engine.py:537-545` | LOW |
| 14.15 | Add missing intrinsic signatures to flow.rs | Add Value, Bond, Narrative node types to intrinsic signatures (currently 19, Python has 21). Python: `flow.py:30-115` | `flow/tdg_flow_engine.py:30-115` | LOW |
| 14.16 | Unit tests for all new code | Each addition gets `#[cfg(test)]` tests. Target: 20+ new tests | — | MEDIUM |

### Success Criteria
- All existing 365 tests still pass
- 20+ new tests added
- flow.rs has quadrant modulators and channel-aware clipping
- knowledge.rs has archival index and aligned CatalystType
- diagnostic.rs has escalation, phantom detection, metrics staleness
- ops.rs has meta_view, drive_matrix_report, stage_status

---

## Phase 15: Mind Pipeline Port

**Goal**: Port remaining mind modules (data_loader, terrain, sections, injector).
**Duration**: 2-3 days
**Dependencies**: Phase 14 complete

### Tasks

| # | Task | Details | Python Source | Effort |
|---|------|---------|---------------|--------|
| 15.1 | Port data_loader.rs | 9 state file loaders: `load_meta_view()`, `load_drive_matrix()`, `load_constraints()`, `load_working_memory()`, `load_loop_state()`, `load_polarity()`, `load_hygiene()`, `load_micro_slice()`, `load_recent_graph_events()`. Use `robust_json_load()` for graceful failure. Python: `mind/data_loader.py` (193 LOC) | `mind/data_loader.py` | LOW |
| 15.2 | Port terrain.rs | `discover_skills_for_terrain()` — find skills via ENABLES/PACKAGES edges to dense node types. `generate_terrain_context()` — domain-agnostic graph snapshot. Python: `mind/terrain.py` (279 LOC) | `mind/terrain.py` | MEDIUM |
| 15.3 | Port sections.rs | `generate_revenue_urgency_section()` (MAKE CONFIGURABLE — not hardcoded revenue targets), `generate_pulse_section()`, `generate_social_terrain_section()`, `generate_sensory_field()`, `query_sqlite_skills()`, `query_sqlite_constraints()`, `detect_wisdom_signals()`. Python: `mind/sections.py` (427 LOC) | `mind/sections.py` | MEDIUM |
| 15.4 | Port injector.rs | `generate_prompt()` — terrain-first 9-section context block. `write_mind_state_file()` — JSON output. Wisdom detection cadence (every 5 cycles). Python: `mind/injector.py` (498 LOC) | `mind/injector.py` | MEDIUM |
| 15.5 | Add terrain integration to diagnostic.rs | Port `integrate_terrain()` from Python — merge diagnostic+terrain context. Python: `mind/diagnostic_engine.py:550-620` | `mind/diagnostic_engine.py:550-620` | LOW |
| 15.6 | Add prompt formatting to diagnostic.rs | Port `diagnostic_prompt_section()` — format diagnostic output for agent consumption. Python: `mind/diagnostic_engine.py:621-682` | `mind/diagnostic_engine.py:621-682` | LOW |
| 15.7 | Update lib.rs with new modules | Add `data_loader`, `terrain`, `sections`, `injector` modules + re-exports | — | LOW |
| 15.8 | Unit tests for all new modules | Each module gets `#[cfg(test)]` tests. Target: 20+ new tests | — | MEDIUM |

### Design Decisions
- **sections.rs**: Make revenue targets configurable via TOML config (not hardcoded like Python)
- **injector.rs**: Use `Config` struct for section toggles (lean mode = fewer sections)
- **terrain.rs**: Cache terrain context for 5 cycles (avoid recomputation)

### Success Criteria
- All existing 365 tests still pass
- 20+ new tests added
- `injector.rs` can generate a full context block from DB state
- `sections.rs` produces configurable prompt sections
- `terrain.rs` discovers skills via graph edges
- `data_loader.rs` loads all 9 state files with graceful failure

---

## Phase 16: Event Sourcing

**Goal**: Add JSONL event journal, snapshot manager, deterministic replay.
**Duration**: 2-3 days
**Dependencies**: Phase 14 complete (event recording in flow.rs)

### Tasks

| # | Task | Details | Python Source | Effort |
|---|------|---------|---------------|--------|
| 16.1 | Enhance events table | Add `event_type` column (drive_recomputed, graph_renormalized, node_created, etc.). Add `session_id` column for grouping. Python: `tdg_impl.py:200-300` | `tdg_impl.py:200-300` | LOW |
| 16.2 | Implement JSONL event writer | `EventJournal` struct — append events to JSONL file (not just SQLite). Thread-safe with mutex. Python: `tdg_impl.py:300-400` | `tdg_impl.py:300-400` | MEDIUM |
| 16.3 | Implement SnapshotManager | `SnapshotManager` — periodic state snapshots to JSON files. `save_snapshot()`, `load_snapshot()`, `verify_snapshot()`. Python: `tdg_impl.py:400-500` | `tdg_impl.py:400-500` | MEDIUM |
| 16.4 | Implement ReplayEngine | `ReplayEngine` — deterministic replay from JSONL journal. `replay_from_event()`, `replay_full()`. Python: `tdg_impl.py:500-583` | `tdg_impl.py:500-583` | MEDIUM |
| 16.5 | Add backup verification | `verify_backup()` — check snapshot integrity after backup. Python: `tdg_impl.py:150-200` | `tdg_impl.py:150-200` | LOW |
| 16.6 | Unit tests | Snapshot save/load, replay determinism, JSONL append. Target: 10+ tests | — | MEDIUM |

### Success Criteria
- Events are written to both SQLite AND JSONL file
- Snapshots can be saved/loaded/verified
- Replay produces identical state from JSONL journal
- All existing 365 tests still pass

---

## Phase 17: Audit Engine

**Goal**: Port 5-report audit system, anomaly registry, deprecation registry.
**Duration**: 1-2 days
**Dependencies**: Phase 14 complete

### Tasks

| # | Task | Details | Python Source | Effort |
|---|------|---------|---------------|--------|
| 17.1 | Port audit_engine.rs | `AuditEngine` with 5 reports: integrity (schema violations, dangling edges, orphans, stale), polarity (wraps flow::diagnose_polarity), stage (wraps telearchy::validate_hierarchy), persistence (event count, snapshot validity), capability (broken/unused capabilities). Python: `audit/tdg_audit_engine.py` (341 LOC) | `audit/tdg_audit_engine.py` | MEDIUM |
| 17.2 | Port AnomalyRegistry | JSON-backed anomaly tracking with chronic detection (≥3 occurrences). `Anomaly` struct, `register()`, `detect_chronic()`, `summary()`. Python: `audit/tdg_audit_engine.py:150-300` | `audit/tdg_audit_engine.py:150-300` | LOW |
| 17.3 | Port deprecation_registry.rs | Persistent JSON tracking deprecated components. `register()`, `mark_removed()`, `list_active()`, `summary()`. Pre-populate with 8 known deprecated items. Python: `audit/deprecation_registry.py` (226 LOC) | `audit/deprecation_registry.py` | LOW |
| 17.4 | Add full_audit_bundle() | Aggregate all reports into `AuditBundle` with overall_health (healthy/degraded/unhealthy). Python: `audit/tdg_audit_engine.py:300-341` | `audit/tdg_audit_engine.py:300-341` | LOW |
| 17.5 | Add export_audit_markdown() | Format audit bundle as Markdown. Python: `audit/tdg_audit_engine.py:100-150` | `audit/tdg_audit_engine.py:100-150` | LOW |
| 17.6 | Add audit CLI command | `tdg-rust audit` — run full audit bundle, output Markdown. Python: `scripts/audit_integration.py` | `scripts/audit_integration.py` | LOW |
| 17.7 | Unit tests | Each report, anomaly registry, deprecation registry. Target: 10+ tests | — | MEDIUM |

### Success Criteria
- `tdg-rust audit` produces 5-report bundle with overall health assessment
- AnomalyRegistry detects chronic anomalies (≥3 occurrences)
- DeprecationRegistry tracks 8 known deprecated items
- All existing 365 tests still pass

---

## Phase 18: Graph Projection Evaluation

**Goal**: Evaluate petgraph crate for in-memory graph projection. Benchmark vs SQL-only.
**Duration**: 1 day (decision point)
**Dependencies**: Phase 14 complete

### Tasks

| # | Task | Details | Effort |
|---|------|---------|--------|
| 18.1 | Add petgraph dependency | `petgraph = "0.6"` in Cargo.toml | LOW |
| 18.2 | Implement GraphProjection | Build petgraph DiGraph from SQLite tables. `build_projection()`, `refresh_projection()`. Python: `tdg_impl.py:100-200` | MEDIUM |
| 18.3 | Benchmark renormalize_graph | Compare SQL-only vs petgraph projection on 100/1000/10000 node graphs | MEDIUM |
| 18.4 | Benchmark diagnose_polarity | Compare SQL-only vs petgraph projection | MEDIUM |
| 18.5 | Benchmark pathfind | Compare SQL BFS vs petgraph BFS | LOW |
| 18.6 | Decision | If petgraph is >2x faster: keep. If <2x faster: skip (SQL-only is simpler) | — |

### Success Criteria
- Benchmark results documented
- Decision made: adopt petgraph or skip
- If adopted: integration tests pass with projection

---

## Phase 19: Polish & Release

**Goal**: Documentation, benchmarks, integration tests, release preparation.
**Duration**: 1-2 days
**Dependencies**: All previous phases complete

### Tasks

| # | Task | Details | Effort |
|---|------|---------|--------|
| 19.1 | Update criterion benchmarks | Add benchmarks for: meta_view, inject, audit, telearchy, digestion | MEDIUM |
| 19.2 | End-to-end integration test | Catalyst → observation → hypothesis → capability → stage advance flow | MEDIUM |
| 19.3 | README.md | Architecture overview, quick start, CLI reference, API docs | MEDIUM |
| 19.4 | cargo doc | Generate and verify documentation | LOW |
| 19.5 | Clippy + fmt | `cargo clippy --all-targets` + `cargo fmt --check` | LOW |
| 19.6 | Cargo.toml cleanup | Remove unused dependencies, verify versions | LOW |
| 19.7 | Version bump | Set version to 1.0.0-rc.1 | LOW |

### Success Criteria
- All benchmarks pass
- End-to-end test demonstrates full catalyst lifecycle
- README is comprehensive
- Zero warnings, zero clippy suggestions
- Ready for cargo publish

---

## Execution Order (Dependency Graph)

```
Phase 14 (Engine Parity)
    ├── 14.1-14.3: Flow engine fixes
    ├── 14.4-14.6: Knowledge engine fixes
    ├── 14.7-14.10: Diagnostic engine fixes
    ├── 14.11-14.15: Ops facade additions
    └── 14.16: Tests
    
Phase 15 (Mind Pipeline) ─── depends on Phase 14
    ├── 15.1: data_loader.rs
    ├── 15.2: terrain.rs ─── depends on 15.1
    ├── 15.3: sections.rs ─── depends on 15.2
    ├── 15.4: injector.rs ─── depends on 15.1-15.3
    ├── 15.5-15.6: diagnostic integration
    └── 15.7-15.8: lib.rs + tests

Phase 16 (Event Sourcing) ─── depends on Phase 14
    ├── 16.1-16.2: Event journal
    ├── 16.3: SnapshotManager
    ├── 16.4: ReplayEngine
    └── 16.5-16.6: Backup verification + tests

Phase 17 (Audit Engine) ─── depends on Phase 14
    ├── 17.1-17.2: Audit engine + anomaly registry
    ├── 17.3: Deprecation registry
    ├── 17.4-17.6: Bundle + export + CLI
    └── 17.7: Tests

Phase 18 (Graph Projection) ─── depends on Phase 14
    └── 18.1-18.6: Benchmark + decision

Phase 19 (Polish) ─── depends on ALL above
    └── 19.1-19.7: Benchmarks, tests, docs, release
```

### Parallelization Opportunities

After Phase 14 completes:
- **Phase 15** (Mind Pipeline) and **Phase 17** (Audit Engine) can run in parallel
- **Phase 16** (Event Sourcing) can run in parallel with 15 and 17
- **Phase 18** (Graph Projection) can run in parallel with 15, 16, 17

Phase 19 must wait for all above.

---

## Estimated Timeline

| Phase | Duration | Dependencies | Priority |
|-------|----------|--------------|----------|
| 14: Engine Parity | 1-2 days | Phases 1-13 | HIGH |
| 15: Mind Pipeline | 2-3 days | Phase 14 | HIGH |
| 16: Event Sourcing | 2-3 days | Phase 14 | MEDIUM |
| 17: Audit Engine | 1-2 days | Phase 14 | MEDIUM |
| 18: Graph Projection | 1 day | Phase 14 | LOW |
| 19: Polish & Release | 1-2 days | All | MEDIUM |

**Total remaining**: ~8-13 days for complete Rust TDG.

---

## Risk Mitigation (Updated)

1. **sections.py hardcoded values**: Make revenue targets configurable in Rust TOML config. Don't hardcode business logic.
2. **Petgraph evaluation**: Benchmark before committing. If SQL-only is fast enough, skip Phase 18.
3. **Event sourcing complexity**: Start with SQLite-only events. Add JSONL only if needed for audit trail.
4. **Injector dependencies**: injector.rs imports ALL mind modules. Port in dependency order: data_loader → terrain → sections → injector.
5. **Test regression**: Run `cargo test` after each phase. Never break existing 365 tests.
