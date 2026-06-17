#![allow(dead_code)] // Library crate — public API items may not be used by the binary

//! TDG-Rust: Teleological Developmental Graph
//!
//! A complete Rust port of the Python TDG memory infrastructure.
//! Provides graph storage, HRR compositional algebra, flow engine,
//! knowledge engine, and mind injection pipeline.

pub mod config;
pub mod db;
pub mod error;
pub mod flow;
pub mod hrr;
pub mod knowledge;
pub mod mcp;
pub mod models;
pub mod mind;
pub mod ops;
pub mod plugins;
pub mod scripts;
pub mod validation;

// Re-export key types for plugin/library use
pub use config::Config;
pub use db::{init_fts, init_schema, run_migrations, ConnectionPool};
pub use error::{TdgError, TdgResult};
pub use models::{Edge, NewEdge, NewNode, Node, NodeQuery};
