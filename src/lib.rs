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
pub mod graph_projection;
pub mod hrr;
pub mod knowledge;
pub mod llm;
pub mod mcp;
pub mod mind;
pub mod models;
pub mod ops;
pub mod plugins;
pub mod schema;
pub mod score;
pub mod scripts;
pub mod telearchy;
pub mod validation;

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
pub use score::{ProvenancedScore, ScoreReconciliationEngine, SourceLayer};
pub use telearchy::{EvidenceCollector, TelearchyEngine, TelearchyReport};
