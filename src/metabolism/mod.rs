//! Metabolism — the event-driven metabolic engine (Phase 2 + Phase 3).
//!
//! This module implements the Tier 2 async metabolism from the computational
//! design doc. It provides:
//!
//! - [`lesser_cycle`] — the M·P·C·E state machine (the trusted anchor)
//! - [`attractor`] — A(H) = ⟨A_M, A_P, A_G, Γ⟩ attractor field (Phase 3)
//! - [`health`] — G_z, P_z, and Resonance metrics (Phase 3)
//! - [`worker`] — the background worker pool that processes metabolism jobs
//!
//! ## Architecture
//!
//! ```text
//! Tier 1 (sync write)          Tier 2 (async metabolism)
//! ─────────────────────        ─────────────────────────
//! tdg_observe()                pending_metabolism table
//! tdg_connect()                       │
//!       │                             ↓
//!       └── enqueue ──────────► MetabolismWorker
//!                                    │
//!                                    ├─ lesser_tick (Phase 2)
//!                                    ├─ recompute_attractor (Phase 3)
//!                                    ├─ recompute_health (Phase 3)
//!                                    └─ resonance_update (Phase 3)
//! ```
//!
//! ## Memory footprint (2GB VPS lean profile)
//!
//! - `pending_metabolism` table: ~1 MB typical (< 1K jobs × ~1KB each)
//! - `lesser_cycle_json` column: ~200 bytes per touched holon
//! - `attractor_field_json` + `health_json`: ~400 bytes per touched holon
//! - `resonance_graph` table: ~10 MB (top-10 partners × 100K holons × 50 bytes)
//! - Worker pool: 1 worker by default (TDG_METABOLISM_WORKERS env var)
//! - Each worker holds 1 SQLite connection (~5 MB)

pub mod attractor;
pub mod health;
pub mod lesser_cycle;
pub mod worker;

pub use attractor::{
    compute as compute_attractor, load as load_attractor, save as save_attractor,
    AttractorField, ArchetypalLoads, ChoiceFlag, CouplingTensor, ReservoirAttractor,
    StabilityFilter,
};
pub use health::{
    interpret_resonance, load as load_health, resonance, save as save_health, Health, HealthState,
};
pub use lesser_cycle::{
    generate_catalyst, load_state, save_state, tick, CycleThresholds, LesserCycleState,
    LesserPhase, ReservoirState, Shadow, TickResult,
};
pub use worker::{JobType, MetabolismWorker, PendingJob};
