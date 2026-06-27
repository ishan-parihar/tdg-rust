#![allow(dead_code)] // Library crate — public API items may not be used by the binary

//! # TDG-Rust
//!
//! **Teleological Developmental Graph** — a memory infrastructure for AI agents.
//!
//! This crate is a Rust port of the [Python TDG](https://github.com/ishanp/tdg),
//! optimized for low resource usage, high throughput, and production deployment
//! as an MCP server.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │                  MCP Transport                   │
//! │          (stdio / HTTP-SSE via axum)             │
//! ├─────────────────────────────────────────────────┤
//! │                    Plugins                       │
//! │  entity_extractor · hybrid_retriever · turn_capture │
//! ├─────────────────────────────────────────────────┤
//! │              Mind / Knowledge                    │
//! │  consolidation · reflect · terrain · injector    │
//! ├─────────────────────────────────────────────────┤
//! │              Core Graph Engine                   │
//! │  grammar · flow · hrr · graph_algorithms         │
//! ├─────────────────────────────────────────────────┤
//! │              Persistence Layer                   │
//! │  db (SQLite+WAL) · eventsourcing · schema        │
//! ├─────────────────────────────────────────────────┤
//! │              Observability                       │
//! │  audit · circuit_breaker · validation              │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! ## Modules
//!
//! | Module | Description |
//! |--------|-------------|
//! | [`db`] | SQLite connection pooling, CRUD, schema migrations, FTS5 |
//! | [`mcp`] | MCP server (stdio + HTTP) with 17 tools for AI agents |
//! | [`plugins`] | Entity extraction, hybrid retrieval, turn capture |
//! | [`grammar`] | Node blueprint mapping and auto-wired edge creation |
//! | [`mind`] | Consolidation, reflection, terrain context, mind injection |
//! | [`knowledge`] | Knowledge engine for reasoning over the graph |
//! | [`flow`] | Execution flow engine for graph traversals |
//! | [`hrr`] | Holographic Reduced Representation vectors (1024-dim) |
//! | [`graph_algorithms`] | PageRank, shortest path, community detection |
//! | [`graph_projection`] | Subgraph projection and visualization |
//! | [`models`] | Core data types: [`Node`], [`Edge`], [`NewNode`], [`NewEdge`] |
//! | [`schema`] | Enums: [`Stage`], [`Quadrant`], [`CatalystType`], [`TelosLevel`] |
//! | [`config`] | Hierarchical configuration (YAML → JSON → env vars) |
//! | [`error`] | Unified error type [`TdgError`] and [`TdgResult`] |
//! | [`audit`] | Anomaly detection, health checks, audit bundles |
//! | [`circuit_breaker`] | Failure-threshold circuit breaker for write operations |
//! | [`eventsourcing`] | Event journal, replay engine, snapshot management |
//! | [`telearchy`] | Telearchy engine for evidence collection and reporting |
//! | [`digestion`] | Digestion engine for processing raw observations |
//! | [`llm`] | LLM integration for reflection and synthesis |
//! | [`ops`] | Operational utilities |
//! | [`scripts`] | CLI scripts and automation |
//! | [`visualization`] | Graph visualization output |
//! | [`validation`] | Node contract validation |
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use tdg_rust::{Config, ConnectionPool, init_schema, init_fts, run_migrations};
//!
//! // Load configuration (defaults → tdg.yaml → tdg.json → TDG_* env vars)
//! let config = Config::load().unwrap_or_default();
//!
//! // Initialize the database (path, max_connections, busy_timeout_ms)
//! let pool = ConnectionPool::new(
//!     config.db_path.to_str().unwrap(),
//!     5,
//!     30_000,
//! ).unwrap();
//! pool.with_connection(|conn| {
//!     init_schema(conn)?;
//!     init_fts(conn)?;
//!     run_migrations(conn)?;
//!     Ok(())
//! }).unwrap();
//!
//! // Start the MCP server
//! // tdg_rust::mcp::server::serve_stdio(pool);
//! ```
//!
//! ## Configuration
//!
//! Configuration is loaded via [`Config`] with this precedence:
//!
//! 1. Compiled defaults (`~/.hermes` home directory)
//! 2. `tdg.yaml` in the working directory
//! 3. `tdg.json` in the working directory
//! 4. Environment variables prefixed with `TDG_`
//!
//! Key settings:
//!
//! | Variable | Default | Description |
//! |----------|---------|-------------|
//! | `TDG_HOME` | `~/.hermes` | Base home directory |
//! | `TDG_DB_PATH` | `{home}/tdg/graph.db` | SQLite database path |
//! | `TDG_STATE_DIR` | `{home}/state` | State file directory |
//! | `TDG_SKILLS_DIR` | `{home}/skills` | Skills directory |
//! | `TDG_LEAN` | `false` | Lean mode (reduced memory) |

pub mod audit;
pub mod circuit_breaker;
pub mod clustering;
pub mod config;
pub mod db;
pub mod digestion;
pub mod error;
pub mod eventsourcing;
pub mod flow;
pub mod grammar;
pub mod graph_algorithms;
pub mod graph_projection;
pub mod hrr;
pub mod hrr_retriever;
pub mod knowledge;
pub mod llm;
pub mod maintenance;
pub mod mcp;
pub mod mind;
pub mod models;
pub mod ops;
pub mod plugins;
pub mod schema;
pub mod scripts;
pub mod telearchy;
#[cfg(test)]
pub mod test_utils;
pub mod validation;
pub mod visualization;

pub use audit::{Anomaly, AnomalyRegistry, AuditBundle, AuditEngine, AuditReport, HealthStatus};
pub use circuit_breaker::{CircuitBreaker, CircuitState, PreWriteSnapshot, TransactionSnapshot};
pub use config::Config;
pub use db::{init_fts, init_schema, run_migrations, ConnectionPool};
pub use digestion::DigestionEngine;
pub use error::{TdgError, TdgResult};
pub use eventsourcing::{EventJournal, ReplayEngine, SnapshotManager};
pub use grammar::{auto_wire_edges, NodeBlueprint, NodeGrammar};
pub use graph_projection::GraphProjection;
pub use mind::consolidation_engine::{ConsolidationEngine, ConsolidationReport};
pub use mind::injector::{generate_prompt, write_mind_state_file};
pub use mind::reflect_engine::{ReflectEngine, ReflectResult};
pub use mind::sections::{
    generate_pulse_section, generate_revenue_urgency_section, query_sqlite_constraints,
    query_sqlite_skills,
};
pub use mind::terrain::{discover_skills_for_terrain, generate_terrain_context};
pub use models::{Edge, NewEdge, NewNode, Node, NodeQuery};
pub use schema::{CatalystType, DigestionStatus, Quadrant, Stage, TelosLevel};
pub use telearchy::{EvidenceCollector, TelearchyEngine, TelearchyReport};
