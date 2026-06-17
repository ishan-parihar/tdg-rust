//! TDG Plugins
//!
//! Port of `plugins/tdg/` — entity extraction, hybrid retrieval,
//! preference extraction, and turn capture.

pub mod entity_extractor;
pub mod hybrid_retriever;
pub mod preference_extractor;
pub mod turn_capture;

pub use entity_extractor::EntityExtractor;
pub use hybrid_retriever::HybridRetriever;
pub use preference_extractor::PreferenceExtractor;
pub use turn_capture::TurnCapture;
