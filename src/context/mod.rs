//! Context — the agent-facing API (Phase 5).
//!
//! This module provides the ContextPack (single-call structured context
//! aggregation) and 5-Gate Validation (epistemic gates for synthesis
//! submission).
//!
//! ## ContextPack
//!
//! The capstone of the agent API. Aggregates intra/inter/extra-holonic
//! context into a single structured object with `[status: {status}]` tags
//! on every claim. Replaces 6+ CLI calls with 1.
//!
//! ## 5-Gate Validation
//!
//! Every AI-produced synthesis must pass 5 gates before elevation above
//! `ai-draft`:
//!
//! 1. **Grounding** — cites ≥1 canonical node
//! 2. **Failure-mode** — no QIM failure modes + humanistic reduction
//! 3. **Joint validation** — open joints labeled; no canonical with open joints
//! 4. **Cosmological scope** — invariant claims cite ≥2 scales
//! 5. **Provenance completeness** — required fields present

pub mod context_pack;
pub mod validation;

pub use context_pack::{
    build as build_context_pack, Analogue, BondSummary, ContextPack, EventSummary, ExtraContext,
    GreatWaySummary, Grounding, InterContext, IntraContext, ParentSummary, ProvenanceSummary,
    ResonanceSummary, SubHolonSummary,
};
pub use validation::{
    load_report, save_report, validate as validate_synthesis, FailureMode, GateResult,
    SynthesisProvenance, ValidationReport,
};
