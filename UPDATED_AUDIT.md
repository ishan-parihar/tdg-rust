# Updated Python vs Rust TDG Audit

**Date**: 2026-06-18 (Post-Upgrade)
**Python project**: `../tdg/` (31,217 lines, 90 .py files)
**Rust project**: `tdg-rust/src/` (27,829 lines, 63 .rs files)

---

## Executive Summary

| Metric | Python | Rust | Status |
|--------|--------|------|--------|
| **Total Lines** | 31,217 | 27,829 | Rust 89% of Python |
| **Tests** | 576 (claimed) | 482 | Rust 84% of Python |
| **MCP Tools** | 17 | 26 | ✅ Rust exceeds (+9) |
| **Mind Modules** | 15 | 17 (+2 Rust-only) | ✅ Parity |
| **Plugins** | 7 | 5 | ⚠️ 2 missing |

**Overall**: Rust is ~90% functionally complete vs Python. Critical gaps are HRR behavioral divergences and injector integration.

---

## What Was Fixed in This Upgrade Cycle

| Fix | Status | Impact |
|-----|--------|--------|
| HRR algebra (FFT circular convolution) | ✅ Fixed | P0 correctness |
| valid_to IS NULL filters (11 queries) | ✅ Fixed | Data integrity |
| FTS5 rank normalization | ✅ Fixed | Search quality |
| Async writer (turn_capture) | ✅ Added | Performance |
| Write guard module | ✅ Created | Inter-process safety |
| Event triggers (5 capture triggers) | ✅ Added | Event sourcing |
| Clustering (DBSCAN + K-Means) | ✅ Added | Pattern extraction |

---

## Remaining Gaps

### P0 — Behavioral Divergences (Fix Immediately)

| # | Issue | Location | Impact |
|---|-------|----------|--------|
| 1 | **HRR phase_encode non-deterministic** | `hrr.rs:32-44` | Memory banks not reproducible across sessions |
| 2 | **HRR unbind uses correlation, not pseudo-inverse** | `hrr.rs:52-64` | Different numerical behavior than Python |
| 3 | **HRR bundle doesn't normalize** | `hrr.rs:57-66` | Vector magnitude grows, degrades similarity |
| 4 | **Write guard NOT integrated into CRUD** | `db/write_guard.rs` | No inter-process safety on writes |
| 5 | **Circuit breaker NOT wired into write path** | `circuit_breaker.rs` | No automatic failure detection |

### P1 — Missing Integrations (High Priority)

| # | Issue | Location | Impact |
|---|-------|----------|--------|
| 6 | **Injector NOT wired to engines** | `mind/injector.rs` | No feeling/diagnostic/metrics in prompt |
| 7 | **Missing feeling_state_prompt()** | `mind/feeling.rs` | No emotional state formatting |
| 8 | **Missing social terrain section** | `mind/sections.rs` | No social context |
| 9 | **Missing wisdom detection** | `mind/sections.rs` | No emergent patterns |
| 10 | **TDGMemoryProvider not ported** | `plugins/` | No Hermes integration (1,995 lines) |
| 11 | **ReflectTool not ported** | `plugins/` | No LLM synthesis (817 lines) |
| 12 | **Node/edge size limits** | `db/crud.rs` | No capacity enforcement |
| 13 | **Pre-write snapshot not in CRUD** | `circuit_breaker.rs` | No rollback on failure |

### P2 — Quality Improvements (Medium Priority)

| # | Issue | Location | Impact |
|---|-------|----------|--------|
| 14 | **Diagnostic thresholds hardcoded** | `mind/diagnostic.rs` | Not configurable |
| 15 | **Micro_slice simplified** | `ops.rs` | No pathway chain |
| 16 | **write_mind_state_file incomplete** | `mind/injector.rs` | Missing feeling/escalation data |
| 17 | **Duplicate trigger sets** | `db/schema.rs` | Redundant event rows |
| 18 | **Connection pool not cached per path** | `db/pool.rs` | Minor perf issue |

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

### Phase 1: HRR Behavioral Fixes (1-2 days)

| Task | Priority | Effort |
|------|----------|--------|
| Make phase_encode deterministic with seeded RNG | P0 | 1 hour |
| Change unbind to pseudo-inverse (matching Python) | P0 | 2 hours |
| Add normalization to bundle | P0 | 30 min |

### Phase 2: Write Guard Integration (2-3 days)

| Task | Priority | Effort |
|------|----------|--------|
| Wire WriteGuard into CRUD operations | P0 | 4 hours |
| Wire CircuitBreaker into write path | P0 | 4 hours |
| Add PreWriteSnapshot to CRUD | P1 | 4 hours |
| Add node/edge size limits | P1 | 2 hours |

### Phase 3: Injector Integration (3-5 days)

| Task | Priority | Effort |
|------|----------|--------|
| Wire injector to diagnostic engine | P1 | 4 hours |
| Wire injector to feeling engine | P1 | 4 hours |
| Wire injector to metrics engine | P1 | 2 hours |
| Add feeling_state_prompt() formatter | P1 | 2 hours |
| Add social terrain section | P1 | 2 hours |
| Add wisdom detection | P1 | 2 hours |
| Fix write_mind_state_file | P1 | 2 hours |

### Phase 4: Plugin Completion (5-10 days)

| Task | Priority | Effort |
|------|----------|--------|
| Port TDGMemoryProvider (1,995 lines) | P1 | 3-5 days |
| Port ReflectTool (817 lines) | P1 | 1-2 days |
| Externalize diagnostic thresholds | P2 | 2 hours |

### Phase 5: Cleanup (1-2 days)

| Task | Priority | Effort |
|------|----------|--------|
| Consolidate duplicate trigger sets | P2 | 1 hour |
| Fix micro_slice pathway chain | P2 | 4 hours |
| Update README (HRR is implemented) | P2 | 30 min |

---

## Summary

| Phase | Focus | Effort | Impact |
|-------|-------|--------|--------|
| Phase 1 | HRR behavioral fixes | 1-2 days | Critical — correctness |
| Phase 2 | Write guard integration | 2-3 days | Critical — safety |
| Phase 3 | Injector integration | 3-5 days | High — prompt quality |
| Phase 4 | Plugin completion | 5-10 days | High — feature parity |
| Phase 5 | Cleanup | 1-2 days | Medium — code quality |

**Total estimated effort: 12-22 days**

The Rust implementation is ~90% complete. The critical gaps are:
1. HRR behavioral divergences (determinism, unbind algorithm, normalization)
2. Write guard not integrated into CRUD (safety gap)
3. Injector not wired to engines (prompt quality gap)

Once these are fixed, the remaining work is plugin completion (TDGMemoryProvider, ReflectTool) which can be done incrementally.
