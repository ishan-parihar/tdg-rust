//! MCP Transport — Official rmcp SDK for Model Context Protocol
//!
//! Uses the official `rmcp` crate (v1.7) for spec-compliant MCP server.
//! Supports stdio transport (for Hermes/Claude integration) and HTTP/SSE.

pub mod tools;
pub mod server;

// MCP constants (from _shared.py)
pub const MAX_TEXT_LENGTH: usize = 50_000;
pub const MAX_NODE_ID_LENGTH: usize = 256;
pub const MAX_ALIASES: usize = 100;
pub const MAX_LIMIT: i64 = 1000;
pub const MAX_TURNS: i64 = 500;
pub const MAX_BULK_NODES: usize = 500;
