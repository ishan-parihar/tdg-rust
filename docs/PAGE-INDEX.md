# Ownership map — every active doc, the question it owns, its status

Status: LIVE (current truth) · EVIDENCE (dated measured verdict) · PLAN-ACTIVE.

| File | Status | Owns | Not for |
|---|---|---|---|
| [`CURRENT-STATE.md`](CURRENT-STATE.md) | LIVE | What is the current state of tdg-rust (version, build, tests)? | — |
| [`RUNBOOK.md`](RUNBOOK.md) | LIVE | Which exact commands and environment traps apply when operating the repo? | — |
| [`RECORDS.md`](RECORDS.md) | LIVE | Which decisions (AD) and regressions (RG) are recorded? | — |
| [`NEURAL-PLASTICITY-AUDIT-AND-REFACTOR-PLAN.md`](NEURAL-PLASTICITY-AUDIT-AND-REFACTOR-PLAN.md) | EVIDENCE | Can TDG act like a brain? Neural-plasticity and integration-fragmentation audit with refactor plan. | — |
| [`NEURO-BIO-AUDIT-V2.md`](NEURO-BIO-AUDIT-V2.md) | EVIDENCE | Does the architecture match neuro-psycho-biological brain factors? Audit with ponytail/gap plan. | — |
| [`ONTOLOGICAL-ALIGNMENT-AUDIT.md`](ONTOLOGICAL-ALIGNMENT-AUDIT.md) | EVIDENCE | Is tdg-rust ontologically aligned, and is its memory operationally effective? First pass. | — |
| [`ONTOLOGICAL-ALIGNMENT-AUDIT-V2.md`](ONTOLOGICAL-ALIGNMENT-AUDIT-V2.md) | EVIDENCE | Second pass: which ontological-alignment and memory-efficacy gaps remain? | — |
| [`ONTOLOGICAL-ALIGNMENT-AUDIT-V3.md`](ONTOLOGICAL-ALIGNMENT-AUDIT-V3.md) | EVIDENCE | Third pass: ontological alignment and memory operational efficacy. | — |
| [`SESSION_HOOKS.md`](SESSION_HOOKS.md) | Session Hooks — Ambient Context at Session Start — AXI §7: Register your tool into the agent's session lifecycle so every conversation starts with relevant state… |
| [`AUDIT-2026-06-18.md`](archive/pre-refactor/AUDIT-2026-06-18.md) | LIVE | Python vs Rust TDG Audit — 2026-06-18 (Post-P2 Enhancements) | — |
| [`AUDIT_REPORT_2026-07.md`](archive/pre-refactor/AUDIT_REPORT_2026-07.md) | LIVE | TDG-Rust Audit Report — July 2026 — This audit verifies the operational state of the TDG-Rust system (v0.4.3) against the detailed proce… | — |
| [`BUG-FIX-PLAN.md`](archive/pre-refactor/BUG-FIX-PLAN.md) | LIVE | TDG-Rust Bug Fix Plan — 1. C1+C2 (data corruption — reads wrong data) 2. C3 (data corruption — writes wrong type) 3. H3 (que… | — |
| [`COMPREHENSIVE-GAP-ANALYSIS.md`](archive/pre-refactor/COMPREHENSIVE-GAP-ANALYSIS.md) | LIVE | Comprehensive Gap Analysis: Python TDG vs Rust TDG — 1. LLM Provider Abstraction — src/llm/ (1,447 lines): Trait-based with OpenAI, Anthropic, Ollama imp… | — |
| [`CRATE_RESEARCH.md`](archive/pre-refactor/CRATE_RESEARCH.md) | LIVE | Crate Research: Implementing Audit Gaps — Key: bind(a, b) = IFFT(FFT(a) FFT(b)), unbind(c, r) = IFFT(FFT(c) conj(FFT(r)))… | — |
| [`FORWARD-DEVELOPMENT-PLAN.md`](archive/pre-refactor/FORWARD-DEVELOPMENT-PLAN.md) | LIVE | Forward Development Plan: tdg-rust — 1. Plugin tests (~77 new tests): - hybridretriever.rs: embedding cosine boost, stop words, FTS5 quer… | — |
| [`HERMES-INTEGRATION.md`](archive/pre-refactor/HERMES-INTEGRATION.md) | LIVE | TDG-Rust Integration with Hermes-Agent: Hooks Architecture — Hermes-agent's memory provider system uses a hook-based architecture where providers are called at s… | — |
| [`HOLONIC-SCIENCE-AUDIT-AND-REFACTOR-PLAN.md`](archive/pre-refactor/HOLONIC-SCIENCE-AUDIT-AND-REFACTOR-PLAN.md) | LIVE | tdg-rust Holonomic-Science Audit & Refactor Plan — > tdg-rust stores holonic vocabulary as data; it does not enact holonic metabolism as computation. D… | — |
| [`MCP-TOKEN-OPTIMIZATION.md`](archive/pre-refactor/MCP-TOKEN-OPTIMIZATION.md) | LIVE | TDG-Rust MCP Token Optimization Analysis — 91 schemars(description = "...") fields across 28 parameter structs. Each field description adds tok… | — |
| [`PONYTAIL-AUDIT-AND-OPTIMIZATION-PLAN.md`](archive/pre-refactor/PONYTAIL-AUDIT-AND-OPTIMIZATION-PLAN.md) | LIVE | Ponytail Audit + Optimization Plan — tdg-rust v0.5.0 (Post-Phase 6) — Net removable: ~6,500 lines (5,500 docs + 869KB artifact + 88 orphaned script + 46 Dockerfile + ~100… | — |
| [`PYTHON_VS_RUST_AUDIT.md`](archive/pre-refactor/PYTHON_VS_RUST_AUDIT.md) | LIVE | Python vs Rust TDG Audit — 1. HRR algebra is wrong — hrr.rs uses element-wise multiply/divide instead of circular convolution v… | — |
| [`TDG-COMPUTATIONAL-DESIGN.md`](archive/pre-refactor/TDG-COMPUTATIONAL-DESIGN.md) | LIVE | TDG Computational Design — How Holonic Operations Actually Run — The audit identified 20 holonic primitives tdg-rust needs to embody. But embodying them naively — ti… | — |
| [`TDG-RUST-FRAGMENTATION-REPORT.md`](archive/pre-refactor/TDG-RUST-FRAGMENTATION-REPORT.md) | LIVE | TDG-Rust Infrastructure — Complete Fragmentation Report — The TDG-Rust codebase has 35 fragmentation/implementation gaps across 7 layers. The core problem is… | — |
| [`UPDATED_AUDIT.md`](archive/pre-refactor/UPDATED_AUDIT.md) | LIVE | Comprehensive Python vs Rust TDG Audit — The Rust implementation is ~95% complete. The critical gaps are: 1. Write guard not integrated into… | — |
| [`upgrade-plan_2026-06-30.md`](archive/pre-refactor/upgrade-plan_2026-06-30.md) | LIVE | TDG Memory Infrastructure Audit And Upgrade Plan — Audit target: /home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust Commit: d41c5ad Scope: source, sche… | — |
| [`ASKS.md`](kb/ASKS.md) | LIVE | Operator questions and rulings — genome q add "<question>" · genome q rule Q1 "<ruling>" --applied <owner path>. A ruling is done onl… | — |
| [`CLOSED.md`](kb/CLOSED.md) | LIVE | CLOSED — tried and rejected — Append-only. An entry names the item, the verdict, the measurement that killed it (a path), and the… | — |
| [`WORKLOG.md`](kb/WORKLOG.md) | LIVE | Worklog — Append-only: genome log "<entry>", one per unit of work.… | — |
| [`feed.md`](kb/feed.md) | LIVE | Agent feed — Append-only. genome post "msg" --wf <wf> --to <agent>; genome feed --to <you>.… | — |
