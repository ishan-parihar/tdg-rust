# TDG-Rust Architecture Upgrade Plan

**Date**: 2026-06-28
**Status**: Approved — Implementation In Progress
**Author**: Sisyphus (Orchestrator)

---

## Executive Summary

The TDG (Teleological Developmental Graph) system has a sophisticated foundation — nodes, edges, embeddings, trust, pulse, and a mind layer — but suffers from architectural fragmentation. Key components are wired but never connected, duplicate logic exists across modules, and industry-proven GraphRAG patterns are missing entirely. This plan addresses these gaps in 5 phases, prioritized by impact-to-effort ratio.

---

## Current State: Fragmentation Map

### 1. Dead Code Paths (Wired but Never Called)

| Component | Location | Issue |
|-----------|----------|-------|
| `generate_prompt()` | `src/mind/injector.rs` | Public API, zero callers outside tests |
| `PulseEngine::evaluate()` | `src/mind/pulse.rs` | Sophisticated closure rules, but `sections.rs` uses simplified bar chart |
| `_meta_view` | `src/mind/injector.rs` | Loaded from disk, immediately discarded (`_` prefix) |
| `load_drive_matrix()` | `src/mind/injector.rs` | Defined but never called |
| `load_polarity()` | `src/mind/injector.rs` | Defined but never called |
| `load_hygiene()` | `src/mind/injector.rs` | Defined but never called |
| `load_micro_slice()` | `src/mind/injector.rs` | Defined but never called |

### 2. Dual/Parallel Systems

| System A | System B | Issue |
|----------|----------|-------|
| `MindStateManager` → `mind-state.json` | `write_mind_state_file()` → `tdg-mind-state.json` | Two schemas, two persistence mechanisms |
| `Enricher::backfill_embeddings()` | `Janitor::backfill_embeddings()` | Duplicate backfill logic |
| `ReflectEngine` (structural clustering) | `tdg_reflect` MCP tool (ad-hoc LLM query) | Same name, different implementations |

### 3. Missing Integration Points

| Gap | Impact |
|-----|--------|
| No real-time enrichment | New nodes invisible for minutes until batch cycle |
| No graph expansion in retrieval | Connected context missed even if nodes are related |
| No consolidation trigger | `ConsolidationEngine` exists but never auto-invokes |
| No query routing | All queries go through same FTS+embedding path |

### 4. Bugs and Dead Fields

| Bug | Location | Impact |
|-----|----------|--------|
| `quadrant` filter silently ignored | `query_nodes()` | Drive-based queries return wrong results |
| `discover_skills_for_terrain` shadowed variable | `src/mind/terrain.rs` | Queries ALL ENABLES edges regardless of node type |
| Diagnostic/feeling errors swallowed | `src/mind/injector.rs` | No visibility into failures |

---

## Upgrade Plan: 5 Phases

### Phase 1: Wire the Existing Pipeline

**Goal**: Connect the 90% of the mind layer that's already built but orphaned.
**Impact**: 🔴 Critical | **Effort**: 🟢 Low | **Risk**: 🟢 Minimal

#### Tasks

| # | Task | Files | Success Criteria |
|---|------|-------|------------------|
| 1.1 | Create `tdg_context` MCP tool | `src/mcp/tools.rs` | Tool returns structured context prompt from `generate_prompt()` |
| 1.2 | Unify mind state into single canonical file | `src/mind/injector.rs`, `src/mind/mod.rs` | One state file, one schema, `MindStateManager` delegates to it |
| 1.3 | Delete dead loaders | `src/mind/injector.rs` | `load_drive_matrix`, `load_polarity`, `load_hygiene`, `load_micro_slice` removed |
| 1.4 | Fix `discover_skills_for_terrain` bug | `src/mind/terrain.rs` | Use `_ntype` in query instead of querying all types |
| 1.5 | Implement `quadrant` filtering | `src/db/crud.rs` | `query_nodes()` filters by `json_extract(properties, '$.quadrant')` |
| 1.6 | Add error logging to diagnostic/feeling engines | `src/mind/injector.rs` | Log failures instead of swallowing with `if let Ok` |
| 1.7 | Remove stale doc files | `docs/*.md` | Delete or archive docs from June 2026 that reference removed features |

#### Verification
- `cargo test` passes with 0 failures
- `tdg_context` tool returns valid structured prompt
- State file unification verified via integration test

---

### Phase 2: Graph-Enhanced Retrieval

**Goal**: Implement "expand then rerank" — the industry standard for GraphRAG.
**Impact**: 🔴 Critical | **Effort**: 🟡 Medium | **Risk**: 🟡 Medium

#### Tasks

| # | Task | Files | Success Criteria |
|---|------|-------|------------------|
| 2.1 | Add 1-hop graph expansion to `HybridRetriever` | `src/retrieval.rs` | After initial scoring, expand neighbors and rerank |
| 2.2 | Add configurable sort options to `query_nodes` | `src/db/crud.rs` | Support sorting by confidence, trust, created_at, name |
| 2.3 | Add `created_at` range filter | `src/db/crud.rs`, `src/db/query.rs` | Filter nodes by temporal range |
| 2.4 | Add multi-type filter | `src/db/crud.rs`, `src/db/query.rs` | Query multiple node types in single call |
| 2.5 | Add `confidence` range filter | `src/db/crud.rs`, `src/db/query.rs` | Filter by confidence threshold |
| 2.6 | Implement query routing | `src/retrieval.rs` | Route factual→graph, semantic→vector, global→terrain |

#### Verification
- Graph expansion returns connected nodes even if they don't match query text
- Query routing selects correct path based on query type
- Sort/filter options verified via unit tests

---

### Phase 3: Real-Time Enrichment

**Goal**: Eliminate the "invisible window" where new nodes aren't searchable by embedding.
**Impact**: 🟠 High | **Effort**: 🟡 Medium | **Risk**: 🟡 Medium

#### Tasks

| # | Task | Files | Success Criteria |
|---|------|-------|------------------|
| 3.1 | Auto-populate drives/stages from static maps in `add_node()` | `src/mind/mod.rs`, `src/db/crud.rs` | New nodes get correct drive/stage on creation |
| 3.2 | Add inline embedding generation in `add_node()` | `src/db/crud.rs`, `src/embedding/mod.rs` | Feature-gated: new nodes immediately searchable |
| 3.3 | Consolidate `Enricher` + `Janitor` backfill | `src/maintenance/enricher.rs`, `src/maintenance/janitor.rs` | Single `backfill_repair()` function |
| 3.4 | Add background maintenance scheduler | `src/mind/mod.rs` | Configurable interval for auto-maintenance |
| 3.5 | Wire `PulseEngine` into injector | `src/mind/injector.rs`, `src/mind/pulse.rs` | Replace simplified bar chart with real structural gap analysis |

#### Verification
- New nodes appear in search results immediately (no batch delay)
- Consolidated backfill runs correctly for both embeddings and parent_ids
- Scheduler triggers maintenance at configured intervals

---

### Phase 4: Contextual Embeddings

**Goal**: Embeddings that encode graph topology, not just text.
**Impact**: 🟠 High | **Effort**: 🟠 High | **Risk**: 🟠 Higher

#### Tasks

| # | Task | Files | Success Criteria |
|---|------|-------|------------------|
| 4.1 | Enrich embedding text with edge context | `src/mind/enricher.rs` | `"{name} [ENABLES: X] [BLOCKS: Y]"` format |
| 4.2 | Add parent path to embedding text | `src/mind/enricher.rs` | `"{parent.name} > {name}"` format |
| 4.3 | Implement incremental community assignment | `src/db/mod.rs`, new file | Label propagation from neighbors |
| 4.4 | Add community summary table | `src/db/schema.rs` | Global queries answerable via summaries |

#### Verification
- Re-embed existing nodes with enriched text, verify improved search relevance
- Community assignment assigns correct clusters
- Global queries return coherent summaries

---

### Phase 5: Advanced Memory Patterns

**Goal**: Production-grade agentic memory with entity resolution and temporal awareness.
**Impact**: 🟡 Medium | **Effort**: 🟠 High | **Risk**: 🟠 Higher

#### Tasks

| # | Task | Files | Success Criteria |
|---|------|-------|------------------|
| 5.1 | Entity resolution at ingest | `src/mind/injector.rs` | Embed entity name, cosine search existing, LLM dedup |
| 5.2 | Community summaries as retrievable units | `src/retrieval.rs` | Global understanding via community chunks |
| 5.3 | Confidence decay | `src/db/crud.rs` | Stale knowledge surfaces less based on age/retrieval count |
| 5.4 | Duplicate node detection | `src/maintenance/janitor.rs` | Same name + type + source = merge |
| 5.5 | Orphaned subgraph detection | `src/maintenance/janitor.rs` | Reconnect suggestions for disconnected nodes |

#### Verification
- Entity resolution prevents duplicate nodes for same concept
- Confidence decay reduces ranking of old, unused nodes
- Orphan detection identifies and reports disconnected subgraphs

---

## Impact Matrix

| Phase | UX Impact | Effort | Risk | Dependencies |
|-------|-----------|--------|------|--------------|
| **Phase 1** | 🔴 Critical — agents get no structured context today | 🟢 Low — wiring existing code | 🟢 Minimal | None |
| **Phase 2** | 🔴 Critical — retrieval misses connected context | 🟡 Medium — new retriever logic | 🟡 Medium | Phase 1 |
| **Phase 3** | 🟠 High — new nodes invisible for minutes | 🟡 Medium — inline embedding + scheduler | 🟡 Medium | Phase 1 |
| **Phase 4** | 🟠 High — embeddings become graph-aware | 🟠 High — new embedding pipeline | 🟠 Higher | Phase 2, 3 |
| **Phase 5** | 🟡 Medium — prevents long-term degradation | 🟠 High — entity resolution, communities | 🟠 Higher | Phase 4 |

---

## Key Architectural Insights

### From Microsoft GraphRAG
- **Community-based summarization** enables global queries ("what are the main themes?")
- **Hierarchical community structure** allows multi-granularity queries
- The TDG already has all primitives (`get_neighbor_ids`, `pathfind`, `node_graph`, `consolidation_engine`)

### From Graphiti (Zep)
- **Entity resolution at ingest** prevents entity explosion
- **Temporal awareness** with edge expiration prevents stale knowledge
- **Episodic nodes** capture events/interactions, not just entities

### Current Gap
- **Current**: Embed `"{name} {description}"` → flat cosine search → return top-N
- **Target**: Embed entity + edge context → vector search for entry points → 1-hop graph expansion → rerank → community summaries for global queries

---

## Implementation Order

```
Phase 1 (Week 1)
├── 1.1 Create tdg_context MCP tool
├── 1.2 Unify mind state
├── 1.3 Delete dead loaders
├── 1.4 Fix discover_skills_for_terrain bug
├── 1.5 Implement quadrant filtering
├── 1.6 Add error logging
└── 1.7 Remove stale docs

Phase 2 (Week 2)
├── 2.1 Add 1-hop graph expansion
├── 2.2 Add sort options
├── 2.3 Add created_at filter
├── 2.4 Add multi-type filter
├── 2.5 Add confidence filter
└── 2.6 Implement query routing

Phase 3 (Week 3)
├── 3.1 Auto-populate drives/stages
├── 3.2 Inline embedding generation
├── 3.3 Consolidate backfill logic
├── 3.4 Background maintenance scheduler
└── 3.5 Wire PulseEngine

Phase 4 (Week 4-5)
├── 4.1 Enrich embedding text with edges
├── 4.2 Add parent path to embedding
├── 4.3 Community assignment
└── 4.4 Community summary table

Phase 5 (Future)
├── 5.1 Entity resolution
├── 5.2 Community summaries
├── 5.3 Confidence decay
├── 5.4 Duplicate detection
└── 5.5 Orphan detection
```

---

## Testing Strategy

- **Unit tests**: Each phase includes verification criteria
- **Integration tests**: End-to-end flow from node creation to retrieval
- **Regression tests**: `cargo test` must pass at every phase boundary
- **Manual verification**: Use MCP tools to verify context quality

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Breaking existing functionality | Run full test suite at each phase boundary |
| Performance degradation | Benchmark embedding generation and retrieval before/after |
| Schema migration issues | Use SQLite's `IF NOT EXISTS` for new tables |
| Regression in node creation | Feature-gate inline embedding with `#[cfg(feature)]` |

---

## Success Criteria

1. **Phase 1 complete**: `tdg_context` tool returns structured context, all tests pass
2. **Phase 2 complete**: Graph expansion returns connected nodes, query routing works
3. **Phase 3 complete**: New nodes searchable immediately, no batch delay
4. **Phase 4 complete**: Re-embedded nodes show improved search relevance
5. **Phase 5 complete**: Entity resolution prevents duplicates, confidence decay works

---

## Changelog

| Date | Change |
|------|--------|
| 2026-06-28 | Initial plan created after deep audit of mind, query, and retrieval layers |
| 2026-06-28 | Removed sqlite-vec dead code (prerequisite cleanup) |
