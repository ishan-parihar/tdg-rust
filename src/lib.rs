#![allow(dead_code)] // Library crate — public API items may not be used by the binary

//! TDG-Rust: Teleological Developmental Graph
//!
//! A complete Rust port of the Python TDG memory infrastructure.
//! Provides graph storage, HRR compositional algebra, flow engine,
//! knowledge engine, and mind injection pipeline.

pub mod audit;
pub mod circuit_breaker;
pub mod config;
pub mod db;
pub mod digestion;
pub mod error;
pub mod eventsourcing;
pub mod flow;
pub mod grammar;
pub mod hrr;
pub mod knowledge;
pub mod mcp;
pub mod models;
pub mod mind;
pub mod ops;
pub mod plugins;
pub mod score;
pub mod schema;
pub mod scripts;
pub mod telearchy;
pub mod validation;

pub use audit::{AuditBundle, AuditEngine, AuditReport, Anomaly, AnomalyRegistry, HealthStatus};
pub use circuit_breaker::{CircuitBreaker, CircuitState, PreWriteSnapshot, TransactionSnapshot};
pub use config::Config;
pub use db::{init_fts, init_schema, run_migrations, ConnectionPool};
pub use digestion::DigestionEngine;
pub use error::{TdgError, TdgResult};
pub use grammar::{auto_wire_edges, NodeBlueprint, NodeGrammar};
pub use models::{Edge, NewEdge, NewNode, Node, NodeQuery};
pub use score::{ProvenancedScore, ScoreReconciliationEngine, SourceLayer};
pub use schema::{CatalystType, DigestionStatus, Quadrant, Stage, TelosLevel};
pub use telearchy::{EvidenceCollector, TelearchyEngine, TelearchyReport};
pub use mind::consolidation_engine::{ConsolidationEngine, ConsolidationReport};
pub use mind::injector::{generate_prompt, write_mind_state_file};
pub use mind::reflect_engine::{ReflectEngine, ReflectResult};
pub use mind::terrain::{discover_skills_for_terrain, generate_terrain_context};
pub use mind::sections::{generate_revenue_urgency_section, generate_pulse_section, query_sqlite_skills, query_sqlite_constraints};
pub use eventsourcing::{EventJournal, ReplayEngine, SnapshotManager};
