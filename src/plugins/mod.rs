//! # Plugin System
//!
//! High-level plugins for entity extraction, retrieval, preference learning,
//! and conversation turn capture. Port of `plugins/tdg/` from the Python TDG.
//!
//! ## Plugins
//!
//! - [`entity_extractor`] — Pattern-based named entity recognition from text.
//!   Extracts people, tools, platforms, and other entities with confidence scores.
//! - [`hybrid_retriever`] — Combined FTS5 full-text search with trust-weighted
//!   and recency-weighted scoring for semantic node retrieval.
//! - [`preference_extractor`] — Detects user preferences, corrections, and
//!   constraints from natural language, mapping them to graph quadrants.
//! - [`turn_capture`] — Captures conversation turns with deduplication, rate
//!   limiting, and automatic quadrant classification.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tdg_rust::plugins::entity_extractor::EntityExtractor;
//!
//! let extractor = EntityExtractor::new();
//! let entities = extractor.extract("Alice mentioned Rust during the meeting", None);
//! ```

pub mod entity_extractor;
pub mod hybrid_retriever;
pub mod preference_extractor;
pub mod turn_capture;

pub use entity_extractor::EntityExtractor;
pub use hybrid_retriever::HybridRetriever;
