//! # Plugin System
//!
//! High-level plugins for entity extraction, retrieval, and preference learning.
//!
//! ## Plugins
//!
//! - [`entity_extractor`] — Pattern-based named entity recognition from text.
//! - [`hybrid_retriever`] — Combined FTS5 full-text search with trust-weighted
//!   and recency-weighted scoring for semantic node retrieval.
//! - [`preference_extractor`] — Detects user preferences, corrections, and
//!   constraints from natural language, mapping them to graph quadrants.
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

pub use entity_extractor::EntityExtractor;
pub use hybrid_retriever::HybridRetriever;
