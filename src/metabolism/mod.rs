//! Metabolism — the event-driven metabolic engine (Phase 2).
//!
//! This module implements the Tier 2 async metabolism from the computational
//! design doc. It provides:
//!
//! - [`lesser_cycle`] — the M·P·C·E state machine (the trusted anchor)
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
//!                                    ├─ lesser_tick
//!                                    ├─ recompute_attractor (Phase 3)
//!                                    └─ resonance_update (Phase 3)
//! ```
//!
//! ## Memory footprint (2GB VPS lean profile)
//!
//! - `pending_metabolism` table: ~1 MB typical (< 1K jobs × ~1KB each)
//! - `lesser_cycle_json` column: ~200 bytes per touched holon
//! - Worker pool: 1 worker by default (TDG_METABOLISM_WORKERS env var)
//! - Each worker holds 1 SQLite connection (~5 MB)

pub mod lesser_cycle;
pub mod worker;

pub use lesser_cycle::{
    generate_catalyst, load_state, save_state, tick, CycleThresholds, LesserCycleState,
    LesserPhase, ReservoirState, Shadow, TickResult,
};
pub use worker::{MetabolismWorker, JobType, PendingJob};
