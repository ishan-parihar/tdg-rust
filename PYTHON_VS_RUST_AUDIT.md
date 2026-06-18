# Python vs Rust TDG Audit

**Date**: 2026-06-18
**Python project**: `../tdg/` (31,217 lines, 90 .py files)
**Rust project**: `tdg-rust/src/` (26,050 lines, 61 .rs files)

---

## Executive Summary

| Metric | Python | Rust | Status |
|--------|--------|------|--------|
| **Total Lines** | 31,217 | 26,050 | Rust 83% of Python |
| **Tests** | 576 (claimed) | 471 | Rust 82% of Python |
| **MCP Tools** | 17 | 26 | ✅ Rust exceeds (+9) |
| **Mind Modules** | 15 | 17 (+2 Rust-only) | ✅ Parity |
| **Plugins** | 7 | 5 | ⚠️ 2 missing |

**Overall**: Rust is ~85% functionally complete vs Python. Critical gap is HRR algebra (correctness issue).

---

## P0 — Correctness Issues

1. **HRR algebra is wrong** — `hrr.rs` uses element-wise multiply/divide instead of circular convolution via FFT. Retrieval results will be fundamentally different.
2. **Missing `valid_to IS NULL` filters** — `reflect_engine.rs`, `consolidation_engine.rs` count soft-deleted nodes as active.
3. **FTS5 rank not normalized** — `hybrid_retriever.rs:113` uses `node.confidence` instead of FTS5 rank.

---

## P1 — Missing Plugins

1. `reflect_tool.py` (817 lines) — LLM-powered synthesis
2. `TDGMemoryProvider` (1,200+ lines) — Full orchestration layer
3. `mind_state.py` (181 lines) — Mind state bridge

---

## P2 — Missing Infrastructure

1. Write guard (circuit breaker + FileLock)
2. Write transaction with rollback
3. Event triggers (auto-capture)
4. Holonic queries (9 methods)
5. Trust scoring (4 methods)
6. BankManager
7. Dream engine
8. Digestion engine
9. Telearchy engine
10. Node grammar

---

## P3 — Mind Module Integration

1. Feeling engine NOT in injector
2. Diagnostic engine NOT in injector
3. Metrics engine NOT in injector
4. LLM synthesis missing in consolidation
5. 8 closure rules missing in pulse
6. 5 drive pathology checks missing in feeling
7. Social terrain section missing
8. Wisdom detection missing

---

## Forward Development Plan

| Phase | Focus | Effort |
|-------|-------|--------|
| Phase 1 | Correctness (HRR, filters, FTS5) | 1-2 days |
| Phase 2 | Core infrastructure | 3-5 days |
| Phase 3 | Mind module integration | 3-5 days |
| Phase 4 | Plugin completion | 3-5 days |
| Phase 5 | MCP tool parity | 2-3 days |
| Phase 6 | Documentation | 2-3 days |

**Total**: 15-25 days
