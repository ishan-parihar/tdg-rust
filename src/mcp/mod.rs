//! MCP Transport — Axum HTTP/SSE server with JSON-RPC 2.0 protocol
//!
//! Port of `mcp/tdg_mcp_server.py` and `mcp/_shared.py`.

pub mod protocol;
pub mod server;
pub mod tools;

// MCP constants (from _shared.py)
pub const MAX_TEXT_LENGTH: usize = 50_000;
pub const MAX_NODE_ID_LENGTH: usize = 256;
pub const MAX_ALIASES: usize = 100;
pub const MAX_LIMIT: i64 = 1000;
pub const MAX_TURNS: i64 = 500;
pub const MAX_BULK_NODES: usize = 500;
