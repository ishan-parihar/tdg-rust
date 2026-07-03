# TDG Memory Infrastructure Audit And Upgrade Plan

Audit target: `/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust`
Commit: `d41c5ad`
Scope: source, schema, tests, docs, maintenance pipeline, MCP integration, dependency/ponytail audit. VPS runtime/database state was not directly inspected.

Verification baseline:
- `cargo check` passes with 19 warnings.
- `cargo check --features onnx` passes with 21 warnings.
- `cargo test --no-run` fails before test execution because `src/mcp/tests.rs` imports private parameter structs through `mcp::tools`.

## Priority Findings

| Priority | Finding | Impact | Evidence | Fix |
|---|---|---|---|---|
| P0 | Test suite does not compile | No reliable regression gate exists for memory fixes. | [src/mcp/tests.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/mcp/tests.rs):9 imports params via `crate::mcp::tools`; [src/mcp/tools.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/mcp/tools.rs):26 only privately imports `params::*`. | Import from `crate::mcp::params::*` or re-export params intentionally. Make `cargo test --no-run` the first gate. |
| P0 | FTS5 schema is structurally wrong | Health and direct FTS queries report 0 or fail; janitor/monitor paths are unreliable. | [src/db/schema.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/db/schema.rs):215 defines `node_id` in an external-content FTS table whose content table has `id`; [src/maintenance/monitor.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/maintenance/monitor.rs):51 counts `nodes_fts`; [src/maintenance/janitor.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/maintenance/janitor.rs):60 depends on `f.node_id`. | Recreate FTS schema with a matching column name or a non-external-content design. Do not just call `rebuild_fts()` at startup. |
| P0 | Hybrid embedding retrieval queries a nonexistent column | Semantic ranking silently collapses or fails; embeddings may never contribute. | [src/plugins/hybrid_retriever.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/plugins/hybrid_retriever.rs):292 selects `embedding`; [src/db/schema.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/db/schema.rs):145 defines `vector`. | Change query to `vector`, add dimension filtering, and add an integration test. |
| P0 | Entity extraction reports but does not wire extracted entities | Observations created through normal adapter flow remain orphaned unless explicit `entities` is supplied. | [src/mcp/tools.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/mcp/tools.rs):1116 wires only explicit `entities`; [src/mcp/tools.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/mcp/tools.rs):1157 extracts entities but only returns them. | Reuse one entity upsert/connect helper for explicit and extracted entities. |
| P1 | Entity alias APIs still use legacy `properties` column | Alias get/add/set fails against real schema; tests mask it. | [src/plugins/entity_extractor.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/plugins/entity_extractor.rs):517, [src/plugins/entity_extractor.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/plugins/entity_extractor.rs):548, [src/plugins/entity_extractor.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/plugins/entity_extractor.rs):593 use `properties`; [src/db/schema.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/db/schema.rs):108 uses `properties_json`. | Replace with `properties_json`; delete ad hoc legacy schemas in tests. |
| P1 | Embedding backfills store wrong/default dimension metadata | Gemma 768 vectors can be recorded as default 384; coverage metrics count stale/wrong-dimension vectors as healthy. | [src/db/schema.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/db/schema.rs):75 adds `dimension INTEGER DEFAULT 384`; [src/db/crud.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/db/crud.rs):217 inserts no dimension; [src/maintenance/enricher.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/maintenance/enricher.rs):163 inserts no dimension; [src/maintenance/janitor.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/maintenance/janitor.rs):295 inserts no dimension. | Centralize `upsert_embedding(node_id, vector, model, dimension)` and require dimensions from config/runtime result. |
| P1 | Maintenance MCP contract is fragmented | Tool schema advertises `action`; implementation ignores it and uses `phase`, so clients call the wrong API. | [src/mcp/params.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/mcp/params.rs):219 defines required `action`; [src/mcp/tools.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/mcp/tools.rs):1258 reads only `phase`. | Normalize on `action`, keep `phase` as deprecated alias for one release. |
| P1 | Stage coverage treats valid T0 as missing | Enricher no longer writes strings, but health still undercounts `being` stage 0. | [src/maintenance/enricher.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/maintenance/enricher.rs):80 assigns `being -> 0`; [src/maintenance/monitor.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/maintenance/monitor.rs):131 excludes `developmental_stage = 0`. | Decide whether `0` is valid T0 or missing. If valid, coverage should check `IS NOT NULL`. |
| P2 | Timestamp format mostly fixed but still inconsistent | Event ordering/filtering can drift where `datetime('now')` remains. | [src/db/events.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/db/events.rs):56 still uses `datetime('now')`; [src/db/schema.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/db/schema.rs):309 uses ISO `strftime`. | Route all timestamps through `now_iso()` or ISO `strftime`. |
| P2 | MCP tools file remains a god module | Large changes are risky and bugs recur after partial modularization. | [src/mcp/tools.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/mcp/tools.rs) is 2,650 LOC. | Split by domain: `search`, `crud`, `observe`, `maintenance`, `mind`, `reflect`, `schema`. |

## Upgrade Plan

1. Restore verification baseline.
   - Fix `src/mcp/tests.rs` imports.
   - Run `cargo test --no-run`.
   - Then run focused tests for schema, MCP tools, plugin integration.

2. Repair storage contracts.
   - Rebuild FTS schema correctly, with migration for existing DBs.
   - Centralize embedding storage and dimension/model metadata.
   - Fix `hybrid_retriever` to read `vector` and ignore mismatched dimensions.

3. Repair observe/entity integration.
   - Extract shared `upsert_entity_and_connect_observation()` helper.
   - Wire both explicit `entities` and `extracted_entities`.
   - Deduplicate by canonical name/type.
   - Add regression test: `tdg_observe("Used rust and docker")` creates observation, entity nodes, and `MENTIONS` edges.

4. Repair schema drift in plugins and tests.
   - Replace all `properties` references with `properties_json`.
   - Replace ad hoc test schemas with `init_schema + init_fts + run_migrations`.
   - Add one test that alias add/get/set works against live schema.

5. Normalize maintenance API.
   - Make `MaintenanceParams.action` optional or actually used.
   - Accept old `phase` only as alias.
   - Add tests for `rebuild_fts`, `health`, `gc_all` contract.

6. Make health meaningful.
   - FTS coverage should compare active nodes to indexed active nodes.
   - Embedding coverage should require matching configured dimension/model.
   - Stage coverage should define T0 semantics explicitly.
   - Health score should not count broken FTS or wrong-dimension embeddings.

7. Refactor after bugs are covered.
   - Split `src/mcp/tools.rs`.
   - Move SQL snippets into typed repository functions.
   - Introduce small helpers for JSON-column read/update.
   - Keep public MCP response shapes stable unless explicitly versioned.

## Ponytail Audit

`delete` archived refactor/audit plans that are not executable source of truth. Keep one current upgrade plan and move historical docs out of repo or into release notes. [docs/archive](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/docs/archive)

`delete` committed local agent memory/session state. These are workspace artifacts, not project source. [.memsearch](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/.memsearch)

`delete` stale private planning artifact if not intentionally part of repo. [.sisyphus](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/.sisyphus)

`shrink` replace ad hoc SQLite schemas in tests with shared test DB setup. Same coverage, less schema drift. [tests/plugin_integration.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/tests/plugin_integration.rs):130

`yagni` remove or privatize public fields exposing crate-private internals. [src/mcp/tools.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/mcp/tools.rs):87

`shrink` remove unused import and constants. [src/mcp/tools.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/mcp/tools.rs):6, [src/mind/embedding.rs](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/src/mind/embedding.rs):41

`native` remove unused dev dependencies if no planned tests use them: `mockall`, `fake`, `insta`. [Cargo.toml](/home/ishanp/Documents/GitHub/MY-PROJECTS/tdg-rust/Cargo.toml):78

Estimated removable: ~4k-7k non-source/documentation/artifact lines, ~150-250 test boilerplate lines, 3 dev dependencies if unused. Source-code cuts should wait until P0/P1 regressions are covered.
