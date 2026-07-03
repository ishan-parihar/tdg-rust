//! # MCP Transport
//!
//! Model Context Protocol server implementation using the official `rmcp` crate.
//! Exposes all TDG graph operations as MCP tools for AI agent integration.
//!
//! ## Architecture
//!
//! - [`server`] — Transport layer: stdio (for Hermes/Claude integration) and
//!   streamable HTTP/SSE for remote connections.
//! - [`tools`] — All 36 TDG MCP tools with automatic schema generation via
//!   `#[tool]` and `#[tool_router]` macros.
//! - [`synthesis_helpers`] — LLM provider chain, output parsing, and
//!   pattern-based synthesis fallback (extracted from `tools.rs` in Phase 0
//!   refactor to shrink the god module).
//! - [`params`] — Tool parameter structs (JSON schema generation).
//! - [`trust`] — Agent trust-score store with SQLite persistence.
//! - [`health`] — Service health monitor with circuit breakers.
//!
//! ## Tools (36)
//!
//! The server exposes these MCP tools, grouped by capability:
//!
//! **Search** — `tdg_search`, `tdg_prefetch`
//! **CRUD** — `tdg_create`, `tdg_update`, `tdg_get_node`, `tdg_bulk_create`,
//!           `tdg_observe`, `tdg_record_exec`
//! **Edges** — `tdg_connect`, `tdg_get_related`
//! **Events** — `tdg_query_events`
//! **Rating** — `tdg_rate_memory`
//! **Mind** — `tdg_mind_state`, `tdg_context`, `tdg_consolidate`
//! **Synthesis** — `tdg_reflect` (LLM-powered), `tdg_reflect_run` (clustering)
//! **Trust** — `tdg_get_trust`, `tdg_adjust_trust`
//! **Health** — `tdg_health_check`, `tdg_system_health`, `tdg_graph_health`,
//!             `tdg_graph_stats`
//! **Schema** — `tdg_get_schema`
//! **Multi-agent** — `tdg_bank`
//! **Entities** — `tdg_entity`
//! **Maintenance** — `tdg_maintenance`, `tdg_enrich`, `tdg_self_manage`,
//!                 `tdg_renormalize`
//! **Audit** — `tdg_audit`
//! **Persistence** — `tdg_save_mind_state`, `tdg_load_mind_state`,
//!                  `tdg_get_project_context`, `tdg_set_project_context`
//! **Import/Export** — `tdg_export`, `tdg_import`
//!
//! See `src/mcp/tools.rs` for the canonical tool list and implementations.
//!
//! ## Constants
//!
//! - [`MAX_TEXT_LENGTH`] — Maximum text payload size (50 KB)
//! - [`MAX_NODE_ID_LENGTH`] — Maximum node ID length (256 chars)
//! - [`MAX_ALIASES`] — Maximum aliases per node (100)
//! - [`MAX_LIMIT`] — Maximum query limit (1000)
//! - [`MAX_TURNS`] — Maximum conversation turns (500)
//! - [`MAX_BULK_NODES`] — Maximum nodes per bulk operation (500)

pub mod health;
pub(crate) mod helpers;
pub mod params;
pub mod server;
pub(crate) mod synthesis_helpers;
pub mod tools;
pub(crate) mod trust;

#[cfg(test)]
mod tests;

// MCP constants (from _shared.py)
pub const MAX_TEXT_LENGTH: usize = 50_000;
pub const MAX_NODE_ID_LENGTH: usize = 256;
pub const MAX_ALIASES: usize = 100;
pub const MAX_LIMIT: i64 = 1000;
pub const MAX_TURNS: i64 = 500;
pub const MAX_BULK_NODES: usize = 500;
