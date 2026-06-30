//! # MCP Transport
//!
//! Model Context Protocol server implementation using the official `rmcp` crate.
//! Exposes all TDG graph operations as MCP tools for AI agent integration.
//!
//! ## Architecture
//!
//! - [`server`] — Transport layer: stdio (for Hermes/Claude integration) and
//!   streamable HTTP/SSE for remote connections.
//! - [`tools`] — All 17 TDG MCP tools with automatic schema generation via
//!   `#[tool]` and `#[tool_router]` macros.
//!
//! ## Tools
//!
//! The server exposes these MCP tools:
//!
//! | Tool | Description |
//! |------|-------------|
//! | `search` | Full-text search across nodes |
//! | `create_node` | Create a new graph node |
//! | `update_node` | Update an existing node |
//! | `delete_node` | Soft-delete a node |
//! | `get_node` | Retrieve a node by ID |
//! | `list_nodes` | List nodes with filters |
//! | `create_edge` | Create an edge between nodes |
//! | `get_edges` | Get edges for a node |
//! | `mind_state` | Query mind injection state |
//! | `terrain_context` | Get terrain context for skills |
//! | `page_rank` | Compute PageRank scores |
//! | `consolidate` | Run memory consolidation |
//! | `reflect` | Run reflection synthesis |
//! | `rate_node` | Rate a node helpful/unhelpful |
//! | `audit_health` | Run audit health check |
//! | `bulk_create` | Bulk create nodes |
//! | `event_journal` | Query event journal |
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
pub mod params;
pub mod server;
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
