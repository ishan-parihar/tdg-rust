# TDG-Rust MCP Token Optimization Analysis

**Date:** 2026-06-28
**Tool count:** 31 MCP tools
**Parameter fields:** 91 schemars descriptions
**Parameter structs:** 28

---

## Current Token Footprint

### Tool Descriptions Over 40 Characters

| Tool | Length | Text |
|------|--------|------|
| `tdg_prefetch` | 111 | "Prefetch relevant context for a query — hybrid FTS5 + embedding search formatted for <memory-context> injection" |
| `tdg_self_manage` | 109 | "Run autonomous self-management: health check → janitor → enricher → archiver with before/after health scoring" |
| `tdg_graph_health` | 72 | "Check graph health — coverage metrics, edge noise, orphan count, DB size" |
| `tdg_system_health` | 66 | "Get overall system health summary including circuit breaker status" |
| `tdg_load_mind_state` | 65 | "Load mind state from disk and return a summary of the loaded data" |
| `tdg_connect` | 65 | "Connect two nodes with an edge (auto-detects type from node pair)" |
| `tdg_get_related` | 61 | "Traverse relationships from a node by edge type and direction" |
| `tdg_get_schema` | 60 | "Introspect the database schema (tables, columns, row counts)" |
| `tdg_get_node` | 58 | "Retrieve details for a specific node with optional context" |
| `tdg_search` | 54 | "Search graph memory using hybrid FTS5 full-text search" |
| `tdg_export` | 54 | "Export graph data to JSON file for migration or backup" |
| `tdg_get_trust` | 53 | "Get the trust score and metadata for a specific agent" |
| `tdg_adjust_trust` | 51 | "Adjust a node's confidence rating based on feedback" |
| `tdg_set_project_context` | 50 | "Set the project context string and persist to disk" |
| `tdg_record_exec` | 50 | "Record an execution outcome as an observation node" |

**15 tools over 40 chars** — each adds ~15-30 extra tokens per tools/list call.

### Parameter Field Bloat

91 `schemars(description = "...")` fields across 28 parameter structs. Each field description adds tokens to every tool listing.

---

## Optimization Opportunities

### Pattern 1: Description Compression (Quick Win)

**Target:** Compress all tool descriptions to ≤40 characters.

| Tool | Current | Compressed | Savings |
|------|---------|------------|---------|
| `tdg_prefetch` | 111 | "Search query → formatted context for injection" | ~70 chars |
| `tdg_self_manage` | 109 | "Run maintenance: health → janitor → enricher → archiver" | ~60 chars |
| `tdg_graph_health` | 72 | "Graph health metrics: coverage, noise, orphans, size" | ~30 chars |
| `tdg_system_health` | 66 | "System health with circuit breaker status" | ~25 chars |
| `tdg_load_mind_state` | 65 | "Load mind state from disk" | ~40 chars |
| `tdg_connect` | 65 | "Connect two nodes with an edge" | ~35 chars |
| `tdg_get_related` | 61 | "Get related nodes by edge type" | ~30 chars |
| `tdg_get_schema` | 60 | "Introspect DB schema" | ~40 chars |
| `tdg_get_node` | 58 | "Get node details with context" | ~30 chars |
| `tdg_search` | 54 | "Hybrid FTS5 + embedding search" | ~20 chars |
| `tdg_export` | 54 | "Export graph to JSON" | ~35 chars |
| `tdg_get_trust` | 53 | "Get agent trust score" | ~30 chars |
| `tdg_adjust_trust` | 51 | "Adjust node confidence" | ~30 chars |
| `tdg_set_project_context` | 50 | "Set project context string" | ~25 chars |
| `tdg_record_exec` | 50 | "Record execution outcome" | ~25 chars |

**Estimated savings:** ~500 tokens per tools/list call (15 tools × ~30 chars average × ~2 tokens/char)

### Pattern 2: Parameter Field Compression

**Target:** Compress parameter descriptions to ≤40 characters.

| Struct | Fields | Current Total | Optimized |
|--------|--------|---------------|-----------|
| `SearchParams` | 3 | ~80 chars | ~45 chars |
| `CreateParams` | 8 | ~200 chars | ~80 chars |
| `UpdateParams` | 6 | ~150 chars | ~60 chars |
| `QueryEventsParams` | 6 | ~150 chars | ~60 chars |
| `PrefetchParams` | 3 | ~80 chars | ~45 chars |
| `ExportParams` | 2 | ~60 chars | ~30 chars |
| `ImportParams` | 2 | ~50 chars | ~30 chars |

**Estimated savings:** ~300 tokens per tools/list call

### Pattern 3: Schema-as-Resource (Advanced)

**Current:** `tdg_get_schema` tool returns full schema on demand.

**Optimization:** Move schema to MCP resource endpoint:
- Add `resources/list` endpoint
- Add `resources/read` endpoint for `tdg://schema`
- Keep `tdg_get_schema` as fallback for non-resource clients

**Estimated savings:** ~2,000 tokens if schema is embedded in tool descriptions (currently it's not — schema is returned on demand via `tdg_get_schema`)

**Note:** The current implementation already follows the "on-demand lookup" pattern — schema is NOT embedded in tool descriptions. This optimization is already in place.

---

## Priority Implementation Plan

| Priority | Action | Effort | Token Savings |
|----------|--------|--------|---------------|
| **P0** | Compress 15 tool descriptions to ≤40 chars | 1 hour | ~500 tokens |
| **P1** | Compress parameter field descriptions | 2 hours | ~300 tokens |
| **P2** | Extract shared enums to constants | 1 hour | ~100 tokens |
| **P3** | Add MCP resource endpoints for schema | 2 hours | ~2,000 tokens (if schema embedded) |

**Total potential savings:** ~2,900 tokens per tools/list call

---

## What's Already Optimized

1. **No schema in tool descriptions** — Schema is returned on demand via `tdg_get_schema`
2. **No `#[serde(flatten)]`** — Zero flatten usages (verified)
3. **No duplicate enum definitions** — Node types and edge types are defined once in `schema.rs`
4. **Compact parameter structs** — Most structs have 2-6 fields

---

## Recommendation

**Implement P0 (description compression) immediately** — it's the highest ROI optimization. Compress the 15 over-length descriptions to ≤40 characters each. This is a 1-hour change that saves ~500 tokens per tools/list call.

P1 (parameter field compression) is also high ROI but takes longer. P2 and P3 are lower priority since the current implementation is already reasonably optimized.

Want me to implement the description compression?
