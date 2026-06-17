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

pub mod diagnostic;
pub mod feeling;
pub mod metrics;
pub mod project_tracker;
pub mod pulse;

