# C2 Fix: Blocking SQLite in Async Tokio Handlers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move all blocking SQLite and file I/O operations off the tokio runtime thread using `spawn_blocking`, preventing thread starvation under concurrent MCP requests.

**Architecture:** Create a `run_blocking` helper that wraps blocking work in `tokio::task::spawn_blocking`. Each async tool handler delegates its SQLite operations to this helper. Sub-structs (HealthMonitor, TrustStore) are wrapped at the call site, not internally.

**Tech Stack:** Rust, tokio, rusqlite, rmcp

## Global Constraints

- VPS target: Debian 12, glibc 2.36, 1vCPU
- Binary size budget: ~12MB (ONNX-enabled)
- No new dependencies (tokio already in Cargo.toml)
- Preserve existing API signatures (all `#[tool]` functions keep same params/return types)
- Tests must pass: `cargo test`

## File Structure

| File | Change |
|------|--------|
| `src/mcp/tools.rs` | Add `run_blocking` helper, wrap 26 async tool bodies |
| `src/mcp/health.rs` | No internal changes (wrapped at call site) |
| `src/mcp/trust.rs` | No internal changes (wrapped at call site) |

---

## Task 1: Add `run_blocking` helper

**Files:**
- Modify: `src/mcp/tools.rs:56-65`

**Interfaces:**
- Consumes: `Arc<ConnectionPool>`
- Produces: `async fn run_blocking<T: Send + 'static>(pool, f) -> T`

- [ ] **Step 1: Add the helper function**

```rust
// After the get_conn helper (line 65), add:

async fn run_blocking<T: Send + 'static>(
    pool: Arc<ConnectionPool>,
    f: impl FnOnce(&ConnectionPool) -> T + Send + 'static,
) -> Result<T, McpError> {
    tokio::task::spawn_blocking(move || f(&pool))
        .await
        .map_err(|e| McpError::internal_error(format!("Task join error: {e}"), None))
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | grep -E "^error"`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/mcp/tools.rs
git commit -m "feat(mcp): add run_blocking helper for spawn_blocking"
```

---

## Task 2: Wrap Category A tools (lightweight, 10 functions)

**Files:**
- Modify: `src/mcp/tools.rs` — lines 101-770

**Pattern:** Replace `let conn = get_conn(&self.pool)?;` with `let conn = run_blocking(self.pool.clone(), |pool| get_conn(pool)?).await?;`

- [ ] **Step 1: Wrap tdg_search (line 101)**

```rust
// Before:
let conn = get_conn(&self.pool)?;

// After:
let pool = self.pool.clone();
let query = params.query.clone();
let limit = params.limit;
let node_type = params.node_type.clone();
let result = run_blocking(pool, move |pool| {
    let conn = get_conn(pool)?;
    let retriever = crate::plugins::HybridRetriever::new();
    retriever.search(&conn, &query, limit.unwrap_or(10).min(50), node_type.as_deref().filter(|s| !s.is_empty()))
}).await?;
```

- [ ] **Step 2: Wrap tdg_prefetch (line 128)** — same pattern as search

- [ ] **Step 3: Wrap tdg_graph_health (line 266)** — wrap all 9 query_rows in one block

- [ ] **Step 4: Wrap tdg_get_node (line 309)** — wrap get_node + get_edges

- [ ] **Step 5: Wrap tdg_query_events (line 341)** — wrap prepare + query_map

- [ ] **Step 6: Wrap tdg_rate_memory (line 753)** — wrap execute + query_row

- [ ] **Step 7: Wrap tdg_get_schema (line 1205)** — wrap prepare + query_map

- [ ] **Step 8: Wrap tdg_bank (line 1236)** — wrap prepare + query_map

- [ ] **Step 9: Wrap tdg_get_related (line 1083)** — wrap get_edges + get_node loop

- [ ] **Step 10: Wrap tdg_get_trust (line 1614)** — wrap trust_store call in spawn_blocking

- [ ] **Step 11: Wrap tdg_adjust_trust (line 1644)** — wrap trust_store call

- [ ] **Step 12: Wrap tdg_health_check (line 1669)** — wrap health_monitor call

- [ ] **Step 13: Wrap tdg_system_health (line 1694)** — wrap health_monitor call

- [ ] **Step 14: Verify compiles and tests pass**

Run: `cargo test 2>&1 | tail -5`
Expected: All tests pass

- [ ] **Step 15: Commit**

```bash
git add src/mcp/tools.rs
git commit -m "feat(mcp): wrap lightweight tools in spawn_blocking"
```

---

## Task 3: Wrap Category B tools (CRUD, 8 functions)

**Files:**
- Modify: `src/mcp/tools.rs` — lines 395-770

- [ ] **Step 1: Wrap tdg_create (line 395)** — wrap add_node + add_edge loop

- [ ] **Step 2: Wrap tdg_update (line 492)** — wrap get_node + update_node

- [ ] **Step 3: Wrap tdg_connect (line 552)** — wrap get_node x2 + pathfind + add_edge + flow::emit_downward + flow::renormalize_graph

- [ ] **Step 4: Wrap tdg_bulk_create (line 654)** — wrap add_node loop + add_edge loop

- [ ] **Step 5: Wrap tdg_record_exec (line 714)** — wrap add_node + get_node + add_edge

- [ ] **Step 6: Wrap tdg_observe (line 976)** — wrap add_node + search + entity_extractor + DigestionEngine

- [ ] **Step 7: Wrap tdg_entity (line 1269)** — wrap query_nodes + get_node

- [ ] **Step 8: Wrap tdg_context (line 1868)** — wrap injector::generate_prompt

- [ ] **Step 9: Verify compiles and tests pass**

Run: `cargo test 2>&1 | tail -5`
Expected: All tests pass

- [ ] **Step 10: Commit**

```bash
git add src/mcp/tools.rs
git commit -m "feat(mcp): wrap CRUD tools in spawn_blocking"
```

---

## Task 4: Wrap heavy hitters (mind_state, reflect, 3 functions)

**Files:**
- Modify: `src/mcp/tools.rs` — lines 774-1600

- [ ] **Step 1: Wrap tdg_mind_state (line 774)** — heaviest function, wraps pragma + count_nodes x5 + count_edges + query_row x3 + prepare x5

- [ ] **Step 2: Wrap tdg_reflect (line 1322)** — 2nd heaviest, wraps query_nodes x4 + count_edges + count_nodes + prepare + store_synthesis + pattern_synthesis

- [ ] **Step 3: Wrap tdg_reflect_run (line 1581)** — wrap ReflectEngine::new + .run()

- [ ] **Step 4: Verify compiles and tests pass**

Run: `cargo test 2>&1 | tail -5`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add src/mcp/tools.rs
git commit -m "feat(mcp): wrap heavy tool handlers in spawn_blocking"
```

---

## Task 5: Wrap file I/O + remaining tools

**Files:**
- Modify: `src/mcp/tools.rs` — lines 159-270, 1137-1200, 1882-1900

- [ ] **Step 1: Wrap tdg_export (line 159)** — wrap std::fs::write in spawn_blocking (or use tokio::fs)

- [ ] **Step 2: Wrap tdg_import (line 213)** — wrap std::fs::read_to_string in spawn_blocking

- [ ] **Step 3: Wrap tdg_maintenance (line 1137)** — wrap knowledge calls

- [ ] **Step 4: Wrap tdg_self_manage (line 1176)** — wrap SelfManager::new + .run()

- [ ] **Step 5: Wrap tdg_graph_stats (line 1713)** — wrap count_nodes + count_edges + GraphProjection::build

- [ ] **Step 6: Wrap tdg_consolidate (line 1882)** — wrap ConsolidationEngine::new + .run()

- [ ] **Step 7: Verify compiles and tests pass**

Run: `cargo test 2>&1 | tail -5`
Expected: All tests pass

- [ ] **Step 8: Commit**

```bash
git add src/mcp/tools.rs
git commit -m "feat(mcp): wrap file I/O and remaining tools in spawn_blocking"
```

---

## Task 6: Final verification + cleanup

- [ ] **Step 1: Full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 2: Check for remaining blocking calls**

Run: `grep -n "get_conn\|pool\.get_connection\|std::fs::" src/mcp/tools.rs | grep -v "run_blocking\|//"`
Expected: Only the `run_blocking` helper definition and comments remain

- [ ] **Step 3: Verify no dead code warnings on new code**

Run: `cargo check 2>&1 | grep "run_blocking"`
Expected: No warnings about run_blocking

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "feat(mcp): C2 fix — move all blocking SQLite off async runtime

Wraps 30 async tool handlers with spawn_blocking to prevent tokio
thread starvation under concurrent MCP requests. Uses a run_blocking
helper for consistent pattern across all tools."
```

---

## Verification Checklist

After all tasks, verify:

1. `cargo test` — all tests pass
2. `cargo clippy` — no new warnings
3. `grep -rn "get_conn\|pool\.get_connection" src/mcp/tools.rs` — only in `run_blocking` helper
4. Manual test: start server with `cargo run -- serve`, send concurrent requests, verify no hangs
