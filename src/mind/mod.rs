//! TDG Mind Injection Pipeline
//!
//! Port of `core/mind/` — the meta-cognitive audit and prompt injection system.
//!
//! This module provides:
//! - DiagnosticEngine: behavioral pattern analysis and suggestions
//! - FeelingEngine: emotional state generation from drive data
//! - PulseEngine: structural gap detection per node type
//! - DataLoader: filesystem + DB state loading
//! - MindInjector: full prompt assembly for LLM context
//! - GraphMind: graph-level mind integration pass (Phase 12 — the closed loop)

pub mod consolidation_engine;
pub mod data_loader;
pub mod diagnostic;
pub mod embedding;
pub mod feeling;
pub mod graph_mind;
pub mod injector;

pub mod pulse;
pub mod reflect_engine;
pub mod sections;
pub mod state;
pub mod terrain;
