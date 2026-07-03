# Comprehensive Python vs Rust TDG Audit

**Date**: 2026-06-18 (Post-Upgrade)
**Python project**: `../tdg/` (29,000+ lines, 85 .py files, v1.0.0)
**Rust project**: `tdg-rust/src/` (28,080 lines, 57+ .rs files, v0.2.0)

---

## Executive Summary

| Metric | Python | Rust | Delta |
|--------|--------|------|-------|
| **Total Lines** | 29,000+ | 28,080 | Rust 97% of Python |
| **Tests** | 576 | 338 | ⚠️ Rust 59% of Python |
| **MCP Tools** | 16 | 26 | ✅ Rust exceeds (+10) |
| **Mind Modules** | 14 | 15 (+1 lifecycle) | ✅ Parity |
| **Plugins** | 7 | 4 | ⚠️ 3 missing (intentional) |
| **Node Types** | 20 | 18 | ⚠️ 2 missing (v4.1 holonic) |
| **Edge Types** | 32 | 35 | ✅ Rust exceeds (+3) |

**Overall**: Rust is ~95% functionally complete vs Python. The gaps are: 2 missing node types (v4.1 holonic), 3 missing plugins (intentional exclusion), and lower test coverage. The previous audit's P0 issues (HRR determinism, injector wiring) have been resolved in the most recent commit.

---

## What Was Fixed in This Upgrade Cycle

| Fix | Status | Impact |
|-----|--------|--------|
| HRR algebra (FFT circular convolution) | ✅ Fixed | P0 correctness |
| HRR phase_encode deterministic (seeded RNG) | ✅ Fixed | P0 determinism |
| valid_to IS NULL filters (11 queries) | ✅ Fixed | Data integrity |
| FTS5 rank normalization | ✅ Fixed | Search quality |
| Async writer (turn_capture) | ✅ Added | Performance |
| Write guard module | ✅ Created | Inter-process safety |
| Event triggers (5 capture triggers) | ✅ Added | Event sourcing |
| Clustering (DBSCAN + K-Means) | ✅ Added | Pattern extraction |
| Injector wired to diagnostic/feeling/metrics | ✅ Fixed | P0 prompt quality |
| feeling_state_prompt() formatter | ✅ Added | P0 emotional state |

---

## Remaining Gaps

### P0 — Critical (Fix Before Deployment)

| # | Issue | Location | Impact | Effort |
|---|-------|----------|--------|--------|
| 1 | **Write guard NOT integrated into CRUD** | `db/write_guard.rs` | No inter-process safety on writes | 4 hours |
| 2 | **Circuit breaker NOT wired into write path** | `circuit_breaker.rs` | No automatic failure detection | 4 hours |
| 3 | **Trust store in-memory only** | `mcp/tools.rs` (TrustStore) | Trust lost on restart | 4 hours |
| 4 | **Health monitor in-memory only** | `mcp/tools.rs` (HealthMonitor) | Health history lost on restart | 4 hours |
| 5 | **unwrap() in tools.rs trust store locks** | `mcp/tools.rs:314` | Panic under contention | 1 hour |

### P1 — Missing Features (High Priority)

| # | Issue | Location | Impact | Effort |
|---|-------|----------|--------|--------|
| 6 | **Missing node types: value, bond, narrative** | `models.rs` | v4.1 holonic schema incomplete | 2 hours |
| 7 | **Diagnostic thresholds hardcoded** | `mind/diagnostic.rs` | Not configurable | 2 hours |
| 8 | **Micro_slice simplified** | `ops.rs` | No pathway chain | 4 hours |
| 9 | **write_mind_state_file incomplete** | `mind/injector.rs` | Missing feeling/escalation data | 2 hours |
| 10 | **Duplicate trigger sets** | `db/schema.rs` | Redundant event rows | 1 hour |
| 11 | **TDGMemoryProvider not ported** | `plugins/` | No Hermes integration (1,995 lines) | 3-5 days |
| 12 | **ReflectTool not ported** | `plugins/` | No LLM synthesis (817 lines) | 1-2 days |
| 13 | **Node/edge size limits** | `db/crud.rs` | No capacity enforcement | 2 hours |

### P2 — Quality Improvements (Medium Priority)

| # | Issue | Location | Impact | Effort |
|---|-------|----------|--------|--------|
| 14 | **Connection pool not cached per path** | `db/pool.rs` | Minor perf issue | 1 hour |
| 15 | **Docker compose missing** | `/` | Deployment convenience | 1 hour |
| 16 | **HTML visualization missing** | `visualization.rs` | Debugging convenience | 4 hours |
| 17 | **Deprecation registry not ported** | `core/audit/` | Migration metadata only | 2 hours |
| 18 | **Lean mode less aggressive than Python** | `mcp/tools.rs` | Doesn't reduce memory/cron | 2 hours |

---

## Modules at Parity

| Module | Python | Rust | Notes |
|--------|--------|------|-------|
| Config | ✅ | ✅ | Rust has YAML/JSON/env (superior) |
| Connection Pool | ✅ | ✅ | Rust has health check (superior) |
| Circuit Breaker | ✅ | ✅ | Rust has 3-state (superior) |
| Flow Engine | ✅ | ✅ | Full dual-pole with quadrant modulators |
| CRUD | ✅ | ✅ | Rust has batch ops (superior) |
| FTS5 | ✅ | ✅ | Equivalent |
| Event Triggers | ✅ | ✅ | Rust has MORE triggers (superior) |
| Event Sourcing | ✅ | ✅ | Rust has JSONL journal |
| Schema Migration | ✅ | ✅ | Equivalent |
| Node Grammar | ✅ | ✅ | Equivalent |
| Auto-Wire | ✅ | ✅ | Equivalent |
| Validation | ✅ | ✅ | Rust has NodeContract system |
| Digestion | ✅ | ✅ | Equivalent |
| Telearchy | ✅ | ✅ | Equivalent |
| Consolidation | ✅ | ✅ | Equivalent |
| Reflection | ✅ | ✅ | Rust more complete |
| Metrics | ✅ | ✅ | Equivalent |
| Feeling | ✅ | ✅ | Different prose quality |
| Pulse | ✅ | ✅ | Equivalent |
| Diagnostic | ✅ | ✅ | Rust more code, hardcoded thresholds |
| Terrain | ✅ | ✅ | Equivalent |
| Project Tracker | ✅ | ✅ | Rust simpler |
| Data Loader | ✅ | ✅ | Equivalent |
| Audit | ✅ | ✅ | Equivalent |
| Score | ✅ | ✅ | Equivalent |
| Knowledge | ✅ | ✅ | Equivalent |
| Embedding | ✅ | ✅ | Feature-gated ONNX |
| Graph Projection | ✅ | ✅ | Different impl (petgraph vs NetworkX) |
| Graph Algorithms | ✅ | ✅ | Rust has PageRank, centrality |
| Clustering | ❌ | ✅ | Rust-only (linfa DBSCAN/KMeans) |
| Visualization | ❌ | ✅ | Rust-only (D3.js/DOT) |
| LLM Providers | ❌ | ✅ | Rust-only (4 providers) |
| Session Lifecycle | ❌ | ✅ | Rust-only (FSM) |
| Mind State Manager | ❌ | ✅ | Rust-only (dual persistence) |

---

## Forward Development Plan

### Phase 1: Production Hardening (1-2 days)

| Task | Priority | Effort |
|------|----------|--------|
| Wire WriteGuard into CRUD operations | P0 | 4 hours |
| Wire CircuitBreaker into write path | P0 | 4 hours |
| Persist trust store to SQLite | P0 | 4 hours |
| Persist health monitor to SQLite | P0 | 4 hours |
| Replace unwrap() calls in tools.rs | P0 | 1 hour |

### Phase 2: Schema Completion (1 day)

| Task | Priority | Effort |
|------|----------|--------|
| Add missing node types: value, bond, narrative | P1 | 2 hours |
| Externalize diagnostic thresholds to YAML | P1 | 2 hours |
| Fix write_mind_state_file (feeling/escalation data) | P1 | 2 hours |
| Consolidate duplicate trigger sets | P1 | 1 hour |
| Add node/edge size limits to CRUD | P1 | 2 hours |

### Phase 3: Plugin Completion (5-10 days)

| Task | Priority | Effort |
|------|----------|--------|
| Port TDGMemoryProvider (1,995 lines) | P1 | 3-5 days |
| Port ReflectTool (817 lines) | P1 | 1-2 days |
| Fix micro_slice pathway chain | P2 | 4 hours |

### Phase 4: Deployment (1-2 days)

| Task | Priority | Effort |
|------|----------|--------|
| Add Docker compose | P2 | 1 hour |
| Add HTML visualization | P2 | 4 hours |
| Port deprecation registry | P2 | 2 hours |
| Make lean mode more aggressive | P2 | 2 hours |

### Phase 5: Test Coverage (ongoing)

| Task | Priority | Effort |
|------|----------|--------|
| Add plugin integration tests | P1 | 1 week |
| Add MCP tool end-to-end tests | P1 | 1 week |
| Add event sourcing stress tests | P2 | 3 days |
| Add LLM provider fallback tests | P2 | 2 days |

---

## Summary

| Phase | Focus | Effort | Impact |
|-------|-------|--------|--------|
| Phase 1 | Production hardening (write guard, trust persistence) | 1-2 days | Critical — safety |
| Phase 2 | Schema completion (node types, thresholds) | 1 day | High — parity |
| Phase 3 | Plugin completion (TDGMemoryProvider, ReflectTool) | 5-10 days | High — integration |
| Phase 4 | Deployment (Docker, visualization) | 1-2 days | Medium — convenience |
| Phase 5 | Test coverage | Ongoing | Medium — regression |

**Total estimated effort: 10-17 days**

The Rust implementation is ~95% complete. The critical gaps are:
1. Write guard not integrated into CRUD (safety gap — P0)
2. Trust/health stores in-memory only (data loss on restart — P0)
3. Missing v4.1 holonic node types (schema gap — P1)

Once Phase 1 is complete, the project is deployment-ready. Phase 3 (plugins) is optional for standalone MCP server usage — only needed for Hermes integration.

---

## Detailed Gap Analysis

### A. Functional Gaps (Features Missing in Rust)

| Feature | Python | Rust | Impact |
|---------|--------|------|--------|
| **Node types: value, bond, narrative** | ✅ v4.1 holonic schema | ❌ Not in NODE_TYPES | Schema incomplete |
| **TDGMemoryProvider** | ✅ 1,995 lines (Hermes integration) | ❌ Intentionally excluded | No Hermes plugin |
| **ReflectTool (standalone)** | ✅ 817 lines (plugins/tdg/reflect_tool.py) | ⚠️ Partial (in tools.rs) | LLM synthesis exists but less feature-rich |
| **Deprecation registry** | ✅ 226 lines (audit/deprecation_registry.py) | ❌ Not ported | Migration metadata only |
| **Diagnostic thresholds YAML** | ✅ config/diagnostic_thresholds.yaml | ❌ Hardcoded in diagnostic.rs | Not tunable |
| **Docker compose** | ✅ docker-compose.yml | ❌ Missing | Deployment convenience |
| **HTML visualization** | ✅ viz/tdg-graph.html | ❌ Text-only output | Debugging convenience |
| **Lean mode depth** | ✅ Drops HRR, ONNX, banks, reduces cron | ⚠️ Only skips MCP tools | Less aggressive |

### B. Operational Gaps (Production Readiness)

| Area | Python | Rust | Gap |
|------|--------|------|-----|
| **Write safety** | FileLock + CircuitBreaker + PreWriteSnapshot in CRUD | WriteGuard module EXISTS but NOT called from CRUD | 🔴 Critical |
| **Trust persistence** | SQLite columns (helpful_count, retrieval_count) | In-memory Mutex<HashMap> — lost on restart | 🔴 Critical |
| **Health persistence** | SQLite-backed health checks | In-memory Vec<HealthCheckRecord> — lost on restart | 🟡 Medium |
| **Error handling** | Exceptions with stack traces | TdgResult<T> — good, but unwrap() in tools.rs | 🟡 Medium |
| **Config loading** | YAML + env vars, CFG singleton | env + JSON + YAML via figment | ✅ Parity |
| **Connection pool** | Queue-based, cached per path | Condvar-based, not cached per path | ✅ Parity |
| **Event sourcing** | SQLite triggers + JSONL | SQLite triggers + JSONL | ✅ Parity |
| **Backup** | rusqlite backup API | rusqlite backup API | ✅ Parity |

### C. Effectiveness Gaps (Quality/Usability)

| Area | Python | Rust | Assessment |
|------|--------|------|------------|
| **MCP tool count** | 16 tools | 26 tools | ✅ Rust exceeds |
| **MCP tool quality** | Descriptive errors, context-aware | Type-safe, auto-schema | ✅ Different strengths |
| **Type safety** | Dynamic typing | Static + ownership | ✅ Rust superior |
| **Memory safety** | GC | Ownership model | ✅ Rust superior |
| **Concurrency** | Threading + GIL | Tokio async | ✅ Rust superior |
| **Performance** | ~5ms per turn overhead | <1ms estimated | ✅ Rust superior |
| **Binary size** | N/A (Python) | ~15MB release | ✅ Rust superior |
| **Startup time** | ~200ms | ~10ms | ✅ Rust superior |
| **Test coverage** | 576 tests (~85%) | 338 tests (~60%) | ⚠️ Python higher |
| **Documentation** | Comprehensive README | Comprehensive README | ✅ Parity |

### D. Rust-Only Advantages (Not in Python)

1. **LLM Provider Abstraction** — Trait-based with OpenAI, Anthropic, Ollama + FallbackProvider chain
2. **Session Lifecycle FSM** — Idle→Active→Paused→Error→Completed (725 lines)
3. **MindStateManager Dual Persistence** — JSON + SQLite WAL (394 lines)
4. **Graph-Aware HRR Retriever** — probe/related/reason/contradict with graph structure
5. **Petgraph PageRank** — Real graph algorithm, not approximated
6. **Trust Store + Health Monitor** — In-memory with circuit breakers
7. **Type-Safe Error Handling** — TdgResult<T> with TdgError enum
8. **Feature-Gated ONNX** — Compile-time feature gate for ONNX inference
9. **Event Sourcing** — EventJournal, ReplayEngine, SnapshotManager
10. **Write Guard** — File-based inter-process write serialization (not yet integrated)
11. **Clustering** — TF-IDF + DBSCAN/K-Means via linfa
12. **Leiden Community Detection** — Real graph algorithm
13. **Criterion Benchmarks** — Performance regression testing
14. **Property-Based Tests** — proptest for graph operations
15. **Snapshot Tests** — insta for output verification
