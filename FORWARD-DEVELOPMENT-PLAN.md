# Forward Development Plan: tdg-rust

## Audit Summary

**Date**: 2026-06-18
**Python project**: `../tdg/` (31,217 lines)
**Rust project**: `tdg-rust/src/` (24,968 lines)
**Status**: 338 tests passing, 0 failures, 0 compilation errors

---

## Functional Gaps

### 1. NetworkX Graph Projection (Not Ported)

| Aspect | Python | Rust | Gap |
|--------|--------|------|-----|
| NetworkX | `_sqlite_to_networkx()` builds full graph in memory | Raw SQL queries only | **Functional gap** |
| Graph algorithms | NetworkX built-in (centrality, community, etc.) | Only petgraph PageRank | **Operational gap** |
| Write transaction | `graph_write_lock()` context manager | SQLite WAL + Mutex | Different concurrency model |

**Impact**: Python uses NetworkX for complex graph operations (centrality, community detection, shortest paths). Rust uses SQL queries which are faster for CRUD but lack advanced graph algorithms.

**Recommendation**: For advanced graph operations, add `petgraph`-based implementations where needed. Current PageRank via petgraph is already better than NetworkX for that specific algorithm.

### 2. Deprecation Registry (Not Ported)

| Aspect | Python | Rust | Gap |
|--------|--------|------|-----|
| Module | `deprecation_registry.py` (226 lines) | Not ported | **Minor gap** |
| Purpose | JSON-backed deprecation tracking | N/A | Low priority |

**Impact**: Migration metadata only. Not needed for runtime operation.

### 3. Background Writer Thread (Different Approach)

| Aspect | Python | Rust | Gap |
|--------|--------|------|-----|
| Architecture | Queue-based writer thread with atexit drain | Arc<Mutex<>> + ConnectionPool | Different pattern |
| Thread-local DB | `self._thread_db = _GDB(db_path)` | `pool.get_connection()` | Both work |
| Rate limiting | `_MAX_WRITES_PER_SECOND = 10` | Not implemented | **Minor gap** |

**Impact**: Python's thread-based writer is Hermes-specific. Rust's approach is more appropriate for async/multi-threaded environments.

### 4. Hermes Integration (Not Portable)

| Aspect | Python | Rust | Gap |
|--------|--------|------|-----|
| MemoryProvider | `TDGMemoryProvider` (1995 lines) | N/A | **Not applicable** |
| Agent lifecycle | `prefetch()`, `sync_turn()`, `on_session_end()` | N/A | **Not applicable** |
| Plugin system | `MemoryProvider` ABC | N/A | **Not applicable** |

**Impact**: TDGMemoryProvider is Hermes agent framework-specific. Not portable to Rust. The Rust equivalent would be a client library for whatever agent framework is used.

### 5. YAML Configuration Loading (Partial)

| Aspect | Python | Rust | Gap |
|--------|--------|------|-----|
| Config format | YAML (`config/*.yaml`) + JSON | JSON + env vars | **Partial gap** |
| Diagnostic thresholds | `diagnostic_thresholds.yaml` | Not loaded | **Minor gap** |

**Impact**: Diagnostic thresholds are currently hardcoded in Rust. Should be configurable via YAML/JSON.

---

## Operational Gaps

### 1. Test Coverage

| Metric | Python | Rust | Gap |
|--------|--------|------|-----|
| Total tests | 576 (claimed) | 338 | **138 fewer tests** |
| Core tests | 200+ | 297 | Parity |
| Plugin tests | 100+ | 23 | **77 fewer plugin tests** |
| MCP tests | 100+ | 12 | **88 fewer MCP tests** |
| Integration tests | 100+ | 21 | **79 fewer integration tests** |

**Impact**: Lower regression protection. New plugin features (Phase 2) need tests.

**Recommendation**: Add tests for:
- Alias resolution (entity_extractor)
- Contradiction detection (turn_capture)
- Recurring/cross-cycle patterns (preference_extractor)
- Embedding cosine boost (hybrid_retriever)
- Batch extraction (entity_extractor)
- Deterministic constraint IDs (preference_extractor)

### 2. Error Handling Consistency

| Aspect | Python | Rust | Gap |
|--------|--------|------|-----|
| Error types | Exceptions | `TdgResult<T>` | Different pattern |
| Error propagation | `try/except` | `?` operator | Different but both work |
| Logging | `logger.debug/warning/error` | `tracing::debug/warn/error` | Different framework |

**Impact**: No functional gap — different but equivalent error handling patterns.

### 3. Documentation

| Aspect | Python | Rust | Gap |
|--------|--------|------|-----|
| API docs | Docstrings on all public methods | Some `///` docstrings, many missing | **Minor gap** |
| README | Comprehensive (120 lines) | Brief (35 lines) | **Minor gap** |
| Examples | Inline in README | Not present | **Minor gap** |

**Impact**: Lower onboarding experience for new developers.

### 4. Docker/Deployment

| Aspect | Python | Rust | Gap |
|--------|--------|------|-----|
| Dockerfile | ✅ | ❌ | **Missing** |
| docker-compose.yml | ✅ | ❌ | **Missing** |
| Render.com deployment | Configured | Not configured | **Minor gap** |

**Impact**: Deployment convenience only.

### 5. Visualization

| Aspect | Python | Rust | Gap |
|--------|--------|------|-----|
| HTML viewer | `viz/tdg-graph.html` | Not present | **Missing** |
| JSON export | `viz/tdg-graph.json` | Not present | **Missing** |

**Impact**: Debugging/visualization convenience only.

---

## Effectiveness Gaps

### 1. Python's Strengths (Not Yet in Rust)

| Feature | Python Advantage | Rust Status | Gap |
|---------|-----------------|-------------|-----|
| Dynamic typing | Flexible node/edge schemas | Static types | Different tradeoff |
| Interactive REPL | Python REPL for debugging | `cargo run` CLI | Different UX |
| Hot reload | Module reimport | Rebuild required | Different workflow |
| Library ecosystem | numpy, networkx, onnxruntime | ndarray, petgraph, ort | Parity (different crates) |

### 2. Rust's Strengths (Not in Python)

| Feature | Rust Advantage | Python Status | Gap |
|---------|---------------|---------------|-----|
| Performance | 10-100x faster | Baseline | **Rust advantage** |
| Memory safety | Zero-cost abstractions | GC overhead | **Rust advantage** |
| Concurrency | `tokio` async runtime | GIL-bound threads | **Rust advantage** |
| Type safety | `TdgResult<T>`, `Option<T>` | Runtime type errors | **Rust advantage** |
| Compile-time checks | Feature gates, exhaustive matches | Runtime errors | **Rust advantage** |
| Binary deployment | Single static binary | Requires Python runtime | **Rust advantage** |

### 3. Code Quality Metrics

| Metric | Python | Rust | Gap |
|--------|--------|------|-----|
| Cyclomatic complexity | Moderate | Low | **Rust better** |
| Lines per function | 15-50 avg | 10-30 avg | **Rust better** |
| Test coverage | ~85% (claimed) | ~70% (estimated) | Python better |
| Documentation | Comprehensive | Sparse | Python better |

---

## Development Plan

### Phase 1: Test Coverage (Priority: HIGH)

**Goal**: Add 150+ tests to reach ~488 total tests

1. **Plugin tests** (~77 new tests):
   - `hybrid_retriever.rs`: embedding cosine boost, stop words, FTS5 query prep
   - `entity_extractor.rs`: alias resolution, inverted index caching, batch extraction, scored matching
   - `turn_capture.rs`: rate limiting, contradiction detection, EXPERIENCES edge, quadrant inference
   - `preference_extractor.rs`: batch processing, recurring patterns, cross-cycle patterns, deterministic IDs

2. **MCP tool tests** (~88 new tests):
   - All 26 tools with lean mode, error cases, edge cases
   - Integration with graph DB operations

3. **Integration tests** (~79 new tests):
   - End-to-end: create node → search → update → delete
   - Plugin integration: entity extraction → turn capture → contradiction detection
   - Mind pipeline: diagnostic → feeling → injector

**Effort**: 5-7 days
**Impact**: Higher regression protection

### Phase 2: Documentation (Priority: MEDIUM)

**Goal**: Comprehensive API docs and README

1. **README.md** expansion:
   - Architecture overview (similar to Python README)
   - Quick start guide
   - Configuration reference
   - Deployment instructions

2. **API documentation**:
   - All public structs, traits, functions
   - Module-level docs
   - Usage examples

3. **Code examples**:
   - Basic graph operations
   - Plugin usage
   - Mind pipeline integration

**Effort**: 3-4 days
**Impact**: Better onboarding experience

### Phase 3: Configuration & Deployment (Priority: MEDIUM)

**Goal**: Production-ready deployment

1. **YAML config loading**:
   - `diagnostic_thresholds.yaml` support
   - `embeddings.json` support
   - `reflect.json` support

2. **Docker setup**:
   - `Dockerfile` for Rust binary
   - `docker-compose.yml` with dependencies

3. **Render.com deployment**:
   - Configuration for HTTP/SSE transport
   - Health check endpoints

**Effort**: 3-4 days
**Impact**: Deployment convenience

### Phase 4: Advanced Graph Operations (Priority: LOW)

**Goal**: NetworkX feature parity where needed

1. **Centrality algorithms** (if needed):
   - PageRank (already done via petgraph)
   - Betweenness centrality
   - Eigenvector centrality

2. **Community detection** (if needed):
   - Louvain method
   - Label propagation

3. **Graph statistics** (if needed):
   - Clustering coefficient
   - Connected components
   - Graph diameter

**Effort**: 5-10 days (if needed)
**Impact**: Advanced analytics capabilities

### Phase 5: Visualization (Priority: LOW)

**Goal**: Debugging and visualization tools

1. **HTML viewer**:
   - Port `viz/tdg-graph.html` to Rust
   - Interactive graph visualization

2. **JSON export**:
   - `viz/tdg-graph.json` generation
   - Export for external tools

**Effort**: 2-3 days
**Impact**: Debugging convenience

---

## Summary

### Completed (Phase 0-2)
- ✅ Fixed 7 compilation errors
- ✅ Added lean mode to all 26 MCP tools
- ✅ Enhanced tdg_observe with digestion pipeline
- ✅ Implemented FTS5 hybrid search
- ✅ Enhanced tdg_mind_state with health/detail/verify modes
- ✅ Enhanced tdg_reflect with LLM fallback chain
- ✅ Enhanced tdg_connect with auto edge type detection
- ✅ Enhanced hybrid_retriever.rs with embedding cosine boost
- ✅ Enhanced entity_extractor.rs with alias resolution
- ✅ Enhanced turn_capture.rs with rate limiting, contradiction detection
- ✅ Enhanced preference_extractor.rs with batch processing, patterns

### Remaining (Phase 3-5)
- 📋 Test coverage (~150 new tests)
- 📋 Documentation (README + API docs)
- 📋 Configuration (YAML loading)
- 📋 Deployment (Docker + Render)
- 📋 Advanced graph operations (if needed)
- 📋 Visualization (if needed)

### Estimated Total Effort
- Phase 1 (Tests): 5-7 days
- Phase 2 (Documentation): 3-4 days
- Phase 3 (Config/Deploy): 3-4 days
- Phase 4 (Advanced): 5-10 days (if needed)
- Phase 5 (Viz): 2-3 days (if needed)

**Total**: 15-28 days depending on scope

### Key Decision Points

1. **Do we need NetworkX-level graph algorithms?** If yes, Phase 4 is required. If no, skip it.

2. **Do we need the Hermes MemoryProvider?** If yes, create a Rust client library. If no, skip it.

3. **Do we need YAML config loading?** If yes, add `serde_yaml` dependency. If no, keep JSON + env vars.

4. **Do we need Docker?** If yes, create Dockerfile. If no, skip deployment for now.

### Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking changes in Phase 3 | Low | High | Test before each change |
| Missing test coverage | Medium | Medium | Systematic test writing |
| YAML parsing complexity | Low | Low | Use `serde_yaml` crate |
| Docker image size | Medium | Low | Multi-stage build |
