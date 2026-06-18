//! TDG Mind Injection Pipeline
//!
//! Port of `core/mind/` — the meta-cognitive audit and prompt injection system.
//!
//! This module provides:
//! - DiagnosticEngine: behavioral pattern analysis and suggestions
//! - MetricsEngine: performance tracking, lead management, wisdom detection
//! - FeelingEngine: emotional state generation from drive data
//! - PulseEngine: structural gap detection per node type
//! - ProjectTracker: multi-phase project lifecycle management
//! - DataLoader: filesystem + DB state loading
//! - MindInjector: full prompt assembly for LLM context

pub mod consolidation_engine;
pub mod data_loader;
pub mod diagnostic;
pub mod embedding;
pub mod feeling;
pub mod injector;
pub mod metrics;
pub mod project_tracker;
pub mod pulse;
pub mod reflect_engine;
pub mod sections;
pub mod state;
pub mod terrain;
