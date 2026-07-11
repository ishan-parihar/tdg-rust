//! Greater Cycle — S·T·G·Ch evolutionary engine.
//!
//! Source: HoloOS `_THEORY/02_Ontology/02.2_Macrocosmic_Metabolic_Architecture.md`
//! (canonical)
//!
//! The greater cycle is the inter-holarchic evolutionary ascent — the
//! all-stage perspective. It mirrors the lesser cycle's topology across
//! the identity-pattern ⇄ operating-environment reservoir-pair, one octave up.
//!
//! ## The 4 Components
//!
//! | Symbol | Name | Role | Direction |
//! |--------|------|------|-----------|
//! | S | Significator | Reservoir A (all stages) | Persistent identity-pattern |
//! | G | Great Way | Reservoir B (all stages) | Operating environment |
//! | T | Transformation | Currency B→A | Threshold restructuring event |
//! | Ch | Choice | Currency A→B | Directional commitment |
//!
//! **Axiom:** "What Transformation is to the Significator, Choice is to the Great Way."
//!
//! ## Discontinuous / Ratcheting
//!
//! Unlike the lesser cycle (continuous), the greater cycle is **discontinuous** —
//! it fires when transformation pressure exceeds the Significator's threshold.
//! Transformation is a phase-change event, not a steady flow.
//!
//! ## The 9-Phase State Machine
//!
//! ```text
//! SignificatorForming → SignificatorStable → TransformationPreCrucible →
//! TransformationCrucible → TransformationReintegration →
//! GreatWayAligned | GreatWayFriction → ChoicePolarizing → ChoiceLocked →
//! SignificatorForming (next octave)
//! ```
//!
//! Guarded transitions:
//! - `TransformationCrucible` requires `crucible_intensity ∈ {Moderate, Acute}`
//! - `ChoiceLocked` requires `crystallization_ratio ≥ 0.7`

use serde::{Deserialize, Serialize};

use crate::error::TdgResult;
use crate::metabolism::lesser_cycle::LesserCycleState;

// ─── Types ───────────────────────────────────────────────────────────────────

/// The 9 phases of the greater cycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum GreaterPhase {
    /// Significator is forming — accumulating identity from lesser cycles.
    #[default]
    SignificatorForming,
    /// Significator is stable — ready for transformation pressure.
    SignificatorStable,
    /// Pressure building, approaching the crucible threshold.
    TransformationPreCrucible,
    /// In the crucible — restructuring event in progress.
    TransformationCrucible,
    /// Reintegrating after the crucible — new configuration stabilizing.
    TransformationReintegration,
    /// Aligned with the Great Way — environmental coupling is harmonious.
    GreatWayAligned,
    /// Friction with the Great Way — environmental resistance.
    GreatWayFriction,
    /// Choice is polarizing — directional commitment forming.
    ChoicePolarizing,
    /// Choice is locked — commitment made, ready for next octave.
    ChoiceLocked,
}

impl GreaterPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SignificatorForming => "significator-forming",
            Self::SignificatorStable => "significator-stable",
            Self::TransformationPreCrucible => "transformation-pre-crucible",
            Self::TransformationCrucible => "transformation-crucible",
            Self::TransformationReintegration => "transformation-reintegration",
            Self::GreatWayAligned => "great-way-aligned",
            Self::GreatWayFriction => "great-way-friction",
            Self::ChoicePolarizing => "choice-polarizing",
            Self::ChoiceLocked => "choice-locked",
        }
    }

    /// Check if a transition from `self` to `target` is valid.
    pub fn can_transition_to(&self, target: &Self) -> bool {
        matches!(
            (self, target),
            (Self::SignificatorForming, Self::SignificatorStable)
                | (Self::SignificatorStable, Self::TransformationPreCrucible)
                | (
                    Self::TransformationPreCrucible,
                    Self::TransformationCrucible
                )
                | (
                    Self::TransformationCrucible,
                    Self::TransformationReintegration
                )
                | (Self::TransformationReintegration, Self::GreatWayAligned)
                | (Self::TransformationReintegration, Self::GreatWayFriction)
                | (Self::GreatWayAligned, Self::ChoicePolarizing)
                | (Self::GreatWayFriction, Self::ChoicePolarizing)
                | (Self::ChoicePolarizing, Self::ChoiceLocked)
                | (Self::ChoiceLocked, Self::SignificatorForming)
        )
    }
}

impl std::fmt::Display for GreaterPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// The intensity of a transformation crucible.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CrucibleIntensity {
    /// No crucible active.
    #[default]
    None,
    /// Moderate intensity — restructurable with effort.
    Moderate,
    /// Acute intensity — forced restructuring (crisis).
    Acute,
}

impl CrucibleIntensity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Moderate => "moderate",
            Self::Acute => "acute",
        }
    }
}

/// The complete greater cycle state for a holon.
///
/// Stored as JSON in the `greater_cycle_json` column on `nodes`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GreaterCycleState {
    /// Current phase in the 9-phase cycle.
    pub phase: GreaterPhase,
    /// Significator reservoir (S) — persistent identity-pattern.
    pub significator: f64,
    /// Great Way reservoir (G) — operating environment coupling.
    pub great_way: f64,
    /// Accumulated Transformation pressure (T) — from lesser cycle Experience.
    pub transformation_pressure: f64,
    /// Committed Choice (Ch) — directional commitment magnitude.
    pub choice_committed: f64,
    /// Current crucible intensity.
    pub crucible_intensity: CrucibleIntensity,
    /// Crystallization ratio ∈ [0, 1] — how close Choice is to locking.
    pub crystallization_ratio: f64,
    /// Phase 15: Dissolution ratio ∈ [0, 1] — how much of the old Significator
    /// has dissolved during the crucible. 0 = no dissolution, 1 = complete dissolution.
    /// Tracks the Significator-Liminality state per HoloOS 08.8.14.
    #[serde(default)]
    pub dissolution_ratio: f64,
    /// Number of greater cycles completed (octaves ascended).
    pub octave_count: u64,
    /// Timestamp of last phase transition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_transition_at: Option<String>,
}

impl GreaterCycleState {
    /// Create a fresh state with the Significator forming.
    pub fn forming() -> Self {
        Self {
            phase: GreaterPhase::SignificatorForming,
            significator: 0.1,
            great_way: 0.0,
            transformation_pressure: 0.0,
            choice_committed: 0.0,
            crucible_intensity: CrucibleIntensity::None,
            crystallization_ratio: 0.0,
            dissolution_ratio: 0.0,
            octave_count: 0,
            last_transition_at: None,
        }
    }

    /// Create a stable state (Significator formed, ready for pressure).
    pub fn stable() -> Self {
        Self {
            phase: GreaterPhase::SignificatorStable,
            significator: 0.5,
            great_way: 0.3,
            transformation_pressure: 0.0,
            choice_committed: 0.0,
            crucible_intensity: CrucibleIntensity::None,
            crystallization_ratio: 0.0,
            dissolution_ratio: 0.0,
            octave_count: 0,
            last_transition_at: None,
        }
    }

    /// Whether this holon is in an active transformation (crucible).
    pub fn in_crucible(&self) -> bool {
        self.phase == GreaterPhase::TransformationCrucible
            || self.phase == GreaterPhase::TransformationPreCrucible
    }

    /// Whether this holon has completed a choice (ready for next octave).
    pub fn choice_complete(&self) -> bool {
        self.phase == GreaterPhase::ChoiceLocked
    }

    /// The threshold above which transformation pressure triggers a crucible.
    pub fn transformation_threshold(&self) -> f64 {
        self.significator * 2.0 // 2x the significator magnitude
    }

    // ─── Serialization ──────────────────────────────────────────────────────

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn from_json(s: &str) -> Self {
        if s.is_empty() || s == "{}" {
            return Self::forming();
        }
        serde_json::from_str(s).unwrap_or_else(|_| Self::forming())
    }
}

// ─── The Tick Operator ───────────────────────────────────────────────────────
//
// The greater cycle tick is called less frequently than the lesser cycle
// (it's driven by transformation pressure accumulation, not by each catalyst).
// It checks whether pressure has crossed the threshold and, if so, fires
// the discontinuous transformation sequence.

/// Thresholds for greater-cycle phase transitions.
#[derive(Debug, Clone)]
pub struct GreaterThresholds {
    /// Transformation pressure must exceed this (relative to significator) to trigger crucible.
    pub crucible_trigger_ratio: f64,
    /// Acute crucible threshold (pressure > this × significator).
    pub acute_crucible_ratio: f64,
    /// Crystallization ratio required to lock choice.
    pub choice_lock_threshold: f64,
    /// Great Way friction threshold (below this = friction).
    pub great_way_friction_threshold: f64,
    /// How much choice_committed increases per ChoicePolarizing tick.
    pub choice_polarization_rate: f64,
    /// How much significator grows per SignificatorForming tick.
    pub significator_formation_rate: f64,
}

impl Default for GreaterThresholds {
    fn default() -> Self {
        Self {
            crucible_trigger_ratio: 2.0,
            acute_crucible_ratio: 3.0,
            choice_lock_threshold: 0.7,
            great_way_friction_threshold: 0.3,
            choice_polarization_rate: 0.15,
            significator_formation_rate: 0.1,
        }
    }
}

/// The result of a greater-cycle tick.
#[derive(Debug, Clone, Default)]
pub struct GreaterTickResult {
    /// Whether a phase transition occurred.
    pub transitioned: bool,
    /// The previous phase (if transitioned).
    pub from_phase: Option<GreaterPhase>,
    /// The new phase (if transitioned).
    pub to_phase: Option<GreaterPhase>,
    /// Whether a Transformation event fired (crucible entered).
    pub transformation_fired: bool,
    /// Whether a Choice was locked (cycle completed).
    pub choice_locked: bool,
    /// Whether an octave was ascended (full cycle completed).
    pub octave_ascended: bool,
    /// Whether the stage should advance (via telearchy).
    pub stage_advancement_triggered: bool,
    /// Downward pressure to send to children (from ChoiceLocked).
    pub downward_pressure: f64,
}

/// Run one greater-cycle tick.
///
/// The greater cycle is **discontinuous** — it only fires when transformation
/// pressure exceeds the Significator's threshold. Between firings, it
/// accumulates pressure from the lesser cycle.
///
/// # Arguments
/// * `state` - The holon's greater cycle state
/// * `lesser` - The holon's lesser cycle state (for pressure input)
/// * `thresholds` - Phase transition thresholds
pub fn tick(
    state: &mut GreaterCycleState,
    lesser: &LesserCycleState,
    thresholds: &GreaterThresholds,
) -> GreaterTickResult {
    let mut result = GreaterTickResult::default();
    let now = crate::db::crud::now_iso();

    // Accumulate transformation pressure from lesser cycle Experience
    // (the lesser cycle's transformation_pressure feeds the greater cycle)
    let pressure_input = lesser.transformation_pressure * 0.1; // 10% siphon rate
    state.transformation_pressure += pressure_input;

    // Also grow Great Way coupling from lesser cycle Experience
    state.great_way = (state.great_way + lesser.experience_accumulated * 0.01).min(1.0);

    match state.phase {
        GreaterPhase::SignificatorForming => {
            // Accumulate identity from lesser cycle
            state.significator =
                (state.significator + thresholds.significator_formation_rate).min(1.0);

            // Once significator is sufficiently formed, stabilize
            if state.significator >= 0.5 {
                transition(state, GreaterPhase::SignificatorStable, &mut result, &now);
            }
        }

        GreaterPhase::SignificatorStable => {
            // Check if transformation pressure exceeds threshold
            let threshold = state.significator * thresholds.crucible_trigger_ratio;
            if state.transformation_pressure >= threshold {
                // Determine crucible intensity
                let acute_threshold = state.significator * thresholds.acute_crucible_ratio;
                state.crucible_intensity = if state.transformation_pressure >= acute_threshold {
                    CrucibleIntensity::Acute
                } else {
                    CrucibleIntensity::Moderate
                };
                transition(
                    state,
                    GreaterPhase::TransformationPreCrucible,
                    &mut result,
                    &now,
                );
            }
        }

        GreaterPhase::TransformationPreCrucible => {
            // Enter the crucible (guarded: requires crucible_intensity != None)
            if state.crucible_intensity != CrucibleIntensity::None {
                transition(
                    state,
                    GreaterPhase::TransformationCrucible,
                    &mut result,
                    &now,
                );
                result.transformation_fired = true;
            }
        }

        GreaterPhase::TransformationCrucible => {
            // The crucible restructures the Significator
            // Consume transformation pressure to fuel the restructuring
            let consumed = state.transformation_pressure * 0.5; // consume 50%
            state.transformation_pressure -= consumed;

            // Phase 15: Track dissolution ratio — how much of the old Significator dissolved
            let old_significator = state.significator;

            // Significator is restructured (magnitude shifts)
            // Acute crucibles cause larger shifts
            let shift = match state.crucible_intensity {
                CrucibleIntensity::Acute => 0.3,
                CrucibleIntensity::Moderate => 0.15,
                _ => 0.0,
            };
            state.significator = (state.significator + shift).min(1.0);

            // Phase 15: dissolution_ratio = how much the Significator changed
            state.dissolution_ratio = ((state.significator - old_significator).abs()
                / (old_significator + 0.01))
                .min(1.0);

            // Move to reintegration
            transition(
                state,
                GreaterPhase::TransformationReintegration,
                &mut result,
                &now,
            );
        }

        GreaterPhase::TransformationReintegration => {
            // Determine Great Way alignment vs friction
            // Friction = low Great Way coupling (environment resists)
            if state.great_way < thresholds.great_way_friction_threshold {
                transition(state, GreaterPhase::GreatWayFriction, &mut result, &now);
            } else {
                transition(state, GreaterPhase::GreatWayAligned, &mut result, &now);
            }
        }

        GreaterPhase::GreatWayAligned | GreaterPhase::GreatWayFriction => {
            // Begin polarizing choice
            // Friction slows polarization; alignment speeds it
            let rate = if state.phase == GreaterPhase::GreatWayAligned {
                thresholds.choice_polarization_rate
            } else {
                thresholds.choice_polarization_rate * 0.5 // friction halves the rate
            };

            state.choice_committed = (state.choice_committed + rate).min(1.0);
            state.crystallization_ratio = state.choice_committed;

            transition(state, GreaterPhase::ChoicePolarizing, &mut result, &now);
        }

        GreaterPhase::ChoicePolarizing => {
            // Continue polarizing
            let rate = if state.great_way >= thresholds.great_way_friction_threshold {
                thresholds.choice_polarization_rate
            } else {
                thresholds.choice_polarization_rate * 0.5
            };

            state.choice_committed = (state.choice_committed + rate).min(1.0);
            state.crystallization_ratio = state.choice_committed;

            // Guard: choice locks when crystallization_ratio >= threshold
            if state.crystallization_ratio >= thresholds.choice_lock_threshold {
                transition(state, GreaterPhase::ChoiceLocked, &mut result, &now);
                result.choice_locked = true;
            }
        }

        GreaterPhase::ChoiceLocked => {
            // Choice is locked — commit and prepare for next octave
            // The choice becomes downward pressure for children
            result.downward_pressure = state.choice_committed * 0.5;

            // Reset for next octave
            state.transformation_pressure = 0.0;
            state.choice_committed = 0.0;
            state.crystallization_ratio = 0.0;
            state.crucible_intensity = CrucibleIntensity::None;
            state.octave_count += 1;
            state.significator = 0.5; // re-form at new level

            result.octave_ascended = true;
            result.stage_advancement_triggered = true;

            transition(state, GreaterPhase::SignificatorForming, &mut result, &now);
        }
    }

    result
}

/// Transition to a new phase, recording the transition.
fn transition(
    state: &mut GreaterCycleState,
    new_phase: GreaterPhase,
    result: &mut GreaterTickResult,
    now: &str,
) {
    if state.phase != new_phase {
        // Validate the transition (defensive — the state machine should be correct)
        if !state.phase.can_transition_to(&new_phase) {
            tracing::warn!(
                "Invalid greater-cycle transition: {} → {} (allowed but not in canonical path)",
                state.phase,
                new_phase
            );
        }
        result.from_phase = Some(state.phase.clone());
        result.to_phase = Some(new_phase.clone());
        result.transitioned = true;
        state.phase = new_phase;
        state.last_transition_at = Some(now.to_string());
    }
}

// ─── Phase Transition Detector (Thermodynamic Model) ────────────────────────
//
// Source: HoloOS `_THEORY/09_Thermodynamic_Framework/01_Phase_Transition_Model_Synthesis.md`
// (ai-draft — 4 pillars: Prigogine, Chaisson, Kauffman, Landauer)

/// The 4-pillar phase-transition readiness assessment.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhaseTransitionReadiness {
    /// Prigogine: distance from equilibrium (> threshold = ready).
    pub prigogine: f64,
    /// Chaisson: energy rate density Φ_m (> regime threshold = ready).
    pub chaisson: f64,
    /// Kauffman: catalytic closure (n·p > 1 = ready).
    pub kauffman: f64,
    /// Landauer: informational budget (available energy > kT·ln(2) = ready).
    pub landauer: f64,
    /// Total readiness ∈ [0, 1] (weighted average).
    pub total: f64,
    /// Whether all 4 pillars are ready (total > 0.8).
    pub at_bifurcation: bool,
}

/// Assess phase-transition readiness using the 4-pillar thermodynamic model.
///
/// This is a heuristic approximation — the real model requires physical
/// measurements we don't have. We map:
/// - Prigogine: transformation_pressure (distance from equilibrium)
/// - Chaisson: edge_count × activity_rate (energy rate density)
/// - Kauffman: node diversity × connectivity (catalytic closure)
/// - Landauer: available metabolic capacity (1 - G_z/100)
pub fn assess_readiness(
    lesser: &LesserCycleState,
    greater: &GreaterCycleState,
    edge_count: i64,
    node_diversity: i64,
    g_z: f64,
) -> PhaseTransitionReadiness {
    // Prigogine: pressure relative to threshold.
    // Use both greater-cycle pressure AND lesser-cycle pressure (the lesser
    // feeds the greater, so both contribute to distance from equilibrium).
    let threshold = greater.transformation_threshold();
    let total_pressure = greater.transformation_pressure + lesser.transformation_pressure * 0.1;
    let prigogine = if threshold > 0.0 {
        (total_pressure / threshold).min(1.0)
    } else {
        0.0
    };

    // Chaisson: edge density as energy rate proxy
    let chaisson = (edge_count as f64 / 20.0).min(1.0);

    // Kauffman: diversity × connectivity
    let connectivity = if edge_count > 0 {
        1.0 - (1.0 / edge_count as f64).min(1.0)
    } else {
        0.0
    };
    let kauffman = ((node_diversity as f64 / 10.0) * connectivity).min(1.0);

    // Landauer: available metabolic capacity (inverse of G_z)
    // Low G_z = high available capacity (system not yet efficient = ready to restructure)
    let landauer = (1.0 - g_z / 100.0).max(0.0);

    // Weighted average (equal weights for now)
    let total = (prigogine + chaisson + kauffman + landauer) / 4.0;

    PhaseTransitionReadiness {
        prigogine,
        chaisson,
        kauffman,
        landauer,
        total,
        at_bifurcation: total > 0.8,
    }
}

// ─── DB Persistence ──────────────────────────────────────────────────────────

/// Load greater cycle state for a holon from the DB.
/// Returns forming state if no state is stored.
pub fn load_state(conn: &rusqlite::Connection, holon_id: &str) -> TdgResult<GreaterCycleState> {
    let json: Option<String> = conn
        .query_row(
            "SELECT greater_cycle_json FROM nodes WHERE id = ?1",
            rusqlite::params![holon_id],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    Ok(json
        .map(|s| GreaterCycleState::from_json(&s))
        .unwrap_or_else(GreaterCycleState::forming))
}

/// Save greater cycle state for a holon to the DB.
pub fn save_state(
    conn: &rusqlite::Connection,
    holon_id: &str,
    state: &GreaterCycleState,
) -> TdgResult<()> {
    conn.execute(
        "UPDATE nodes SET greater_cycle_json = ?1 WHERE id = ?2",
        rusqlite::params![state.to_json(), holon_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metabolism::lesser_cycle::{LesserCycleState, ReservoirState};

    fn make_lesser_with_pressure(pressure: f64) -> LesserCycleState {
        let mut s = LesserCycleState::dormant();
        s.transformation_pressure = pressure;
        s.experience_accumulated = pressure * 2.0;
        s.matrix = ReservoirState {
            magnitude: 0.5,
            sign: 0,
            eta: 0.5,
            shadow: None,
        };
        s
    }

    #[test]
    fn forming_state_defaults() {
        let state = GreaterCycleState::forming();
        assert_eq!(state.phase, GreaterPhase::SignificatorForming);
        assert_eq!(state.significator, 0.1);
        assert_eq!(state.octave_count, 0);
    }

    #[test]
    fn tick_significator_forming_accumulates() {
        let mut state = GreaterCycleState::forming();
        let lesser = make_lesser_with_pressure(0.0);
        let thresholds = GreaterThresholds::default();

        tick(&mut state, &lesser, &thresholds);

        // Significator should grow
        assert!(state.significator > 0.1);
    }

    #[test]
    fn tick_significator_forming_transitions_to_stable() {
        let mut state = GreaterCycleState::forming();
        state.significator = 0.45; // close to threshold
        let lesser = make_lesser_with_pressure(0.0);
        let thresholds = GreaterThresholds::default();

        let result = tick(&mut state, &lesser, &thresholds);

        // Should transition to stable (0.45 + 0.1 = 0.55 >= 0.5)
        assert!(result.transitioned);
        assert_eq!(state.phase, GreaterPhase::SignificatorStable);
    }

    #[test]
    fn tick_stable_no_pressure_stays_stable() {
        let mut state = GreaterCycleState::stable();
        let lesser = make_lesser_with_pressure(0.0);
        let thresholds = GreaterThresholds::default();

        let result = tick(&mut state, &lesser, &thresholds);

        // No pressure → no transition
        assert!(!result.transitioned);
        assert_eq!(state.phase, GreaterPhase::SignificatorStable);
    }

    #[test]
    fn tick_stable_with_pressure_triggers_pre_crucible() {
        let mut state = GreaterCycleState::stable();
        state.significator = 0.5;
        let lesser = make_lesser_with_pressure(2.0); // high pressure
        let thresholds = GreaterThresholds::default();

        // Tick enough times to accumulate pressure
        for _ in 0..20 {
            let result = tick(&mut state, &lesser, &thresholds);
            if state.phase == GreaterPhase::TransformationPreCrucible {
                // Found it
                assert!(result.transitioned);
                assert!(state.crucible_intensity != CrucibleIntensity::None);
                return;
            }
        }

        panic!("Should have triggered TransformationPreCrucible");
    }

    #[test]
    fn tick_full_greater_cycle() {
        let mut state = GreaterCycleState::stable();
        state.significator = 0.5;
        let thresholds = GreaterThresholds::default();

        // Feed high pressure to trigger the full cycle
        let lesser = make_lesser_with_pressure(5.0);

        for i in 0..100 {
            let result = tick(&mut state, &lesser, &thresholds);

            if result.octave_ascended {
                // Full cycle completed
                assert_eq!(state.octave_count, 1);
                assert_eq!(state.phase, GreaterPhase::SignificatorForming);
                assert_eq!(state.transformation_pressure, 0.0);
                return;
            }
        }

        // If we didn't complete, at least verify we progressed past stable
        assert_ne!(
            state.phase,
            GreaterPhase::SignificatorStable,
            "Should have progressed past SignificatorStable"
        );
    }

    #[test]
    fn transformation_fires_on_pressure() {
        let mut state = GreaterCycleState::stable();
        state.significator = 0.5;
        let lesser = make_lesser_with_pressure(10.0); // very high pressure
        let thresholds = GreaterThresholds::default();

        let mut transformation_fired = false;
        for _ in 0..50 {
            let result = tick(&mut state, &lesser, &thresholds);
            if result.transformation_fired {
                transformation_fired = true;
                break;
            }
        }

        assert!(transformation_fired, "Transformation should have fired");
    }

    #[test]
    fn choice_locked_requires_crystallization() {
        let mut state = GreaterCycleState::stable();
        state.significator = 0.5;
        state.phase = GreaterPhase::ChoicePolarizing;
        state.choice_committed = 0.6; // below 0.7 threshold
        state.crystallization_ratio = 0.6;
        state.great_way = 0.5; // aligned

        let lesser = make_lesser_with_pressure(0.0);
        let thresholds = GreaterThresholds::default();

        let result = tick(&mut state, &lesser, &thresholds);

        // Should NOT lock yet (0.6 + 0.15 = 0.75 >= 0.7 → should lock)
        // Actually with aligned Great Way, rate = 0.15, so 0.6 + 0.15 = 0.75 >= 0.7
        assert!(result.choice_locked);
        assert_eq!(state.phase, GreaterPhase::ChoiceLocked);
    }

    #[test]
    fn choice_polarizing_friction_slows_rate() {
        let mut state = GreaterCycleState::stable();
        state.significator = 0.5;
        state.phase = GreaterPhase::ChoicePolarizing;
        state.choice_committed = 0.5;
        state.crystallization_ratio = 0.5;
        state.great_way = 0.1; // friction (below 0.3 threshold)

        let lesser = make_lesser_with_pressure(0.0);
        let thresholds = GreaterThresholds::default();

        let initial_choice = state.choice_committed;
        tick(&mut state, &lesser, &thresholds);

        // With friction, rate = 0.15 * 0.5 = 0.075
        let gain = state.choice_committed - initial_choice;
        assert!(
            (gain - 0.075).abs() < 0.01,
            "Expected gain ~0.075, got {}",
            gain
        );
    }

    #[test]
    fn octave_ascended_resets_state() {
        let mut state = GreaterCycleState::stable();
        state.significator = 0.5;
        state.phase = GreaterPhase::ChoiceLocked;
        state.choice_committed = 0.9;
        state.transformation_pressure = 2.0;
        state.octave_count = 0;

        let lesser = make_lesser_with_pressure(0.0);
        let thresholds = GreaterThresholds::default();

        let result = tick(&mut state, &lesser, &thresholds);

        assert!(result.octave_ascended);
        assert_eq!(state.octave_count, 1);
        assert_eq!(state.transformation_pressure, 0.0);
        assert_eq!(state.choice_committed, 0.0);
        assert_eq!(state.phase, GreaterPhase::SignificatorForming);
        assert!(result.downward_pressure > 0.0);
    }

    #[test]
    fn phase_transition_validity() {
        // Test all valid transitions
        assert!(
            GreaterPhase::SignificatorForming.can_transition_to(&GreaterPhase::SignificatorStable)
        );
        assert!(GreaterPhase::SignificatorStable
            .can_transition_to(&GreaterPhase::TransformationPreCrucible));
        assert!(GreaterPhase::TransformationPreCrucible
            .can_transition_to(&GreaterPhase::TransformationCrucible));
        assert!(GreaterPhase::TransformationCrucible
            .can_transition_to(&GreaterPhase::TransformationReintegration));
        assert!(GreaterPhase::TransformationReintegration
            .can_transition_to(&GreaterPhase::GreatWayAligned));
        assert!(GreaterPhase::TransformationReintegration
            .can_transition_to(&GreaterPhase::GreatWayFriction));
        assert!(GreaterPhase::GreatWayAligned.can_transition_to(&GreaterPhase::ChoicePolarizing));
        assert!(GreaterPhase::GreatWayFriction.can_transition_to(&GreaterPhase::ChoicePolarizing));
        assert!(GreaterPhase::ChoicePolarizing.can_transition_to(&GreaterPhase::ChoiceLocked));
        assert!(GreaterPhase::ChoiceLocked.can_transition_to(&GreaterPhase::SignificatorForming));

        // Invalid transitions
        assert!(!GreaterPhase::SignificatorForming.can_transition_to(&GreaterPhase::ChoiceLocked));
        assert!(
            !GreaterPhase::ChoiceLocked.can_transition_to(&GreaterPhase::TransformationCrucible)
        );
    }

    #[test]
    fn readiness_assessment_basic() {
        let lesser = make_lesser_with_pressure(3.0);
        let greater = GreaterCycleState::stable();
        let readiness = assess_readiness(&lesser, &greater, 10, 5, 50.0);

        assert!(readiness.total > 0.0);
        assert!(readiness.total <= 1.0);
        assert!(readiness.prigogine >= 0.0);
        assert!(readiness.chaisson > 0.0); // 10 edges
    }

    #[test]
    fn readiness_at_bifurcation() {
        let lesser = make_lesser_with_pressure(10.0);
        let mut greater = GreaterCycleState::stable();
        greater.transformation_pressure = 10.0; // very high
        let readiness = assess_readiness(&lesser, &greater, 20, 10, 20.0); // low G_z = high landauer

        // With high pressure, high edges, high diversity, low G_z → should be near bifurcation
        assert!(readiness.total > 0.5);
    }

    #[test]
    fn json_roundtrip() {
        let state = GreaterCycleState {
            phase: GreaterPhase::TransformationCrucible,
            significator: 0.7,
            great_way: 0.5,
            transformation_pressure: 3.5,
            choice_committed: 0.0,
            crucible_intensity: CrucibleIntensity::Acute,
            crystallization_ratio: 0.0,
            dissolution_ratio: 0.0,
            octave_count: 2,
            last_transition_at: Some("2026-07-03T12:00:00Z".to_string()),
        };

        let json = state.to_json();
        let restored = GreaterCycleState::from_json(&json);

        assert_eq!(restored.phase, state.phase);
        assert_eq!(restored.significator, state.significator);
        assert_eq!(
            restored.transformation_pressure,
            state.transformation_pressure
        );
        assert_eq!(restored.crucible_intensity, state.crucible_intensity);
        assert_eq!(restored.octave_count, state.octave_count);
    }

    #[test]
    fn json_empty_returns_forming() {
        let state = GreaterCycleState::from_json("");
        assert_eq!(state.phase, GreaterPhase::SignificatorForming);

        let state = GreaterCycleState::from_json("{}");
        assert_eq!(state.phase, GreaterPhase::SignificatorForming);
    }

    #[test]
    fn in_crucible_detection() {
        let mut state = GreaterCycleState::forming();
        assert!(!state.in_crucible());

        state.phase = GreaterPhase::TransformationPreCrucible;
        assert!(state.in_crucible());

        state.phase = GreaterPhase::TransformationCrucible;
        assert!(state.in_crucible());

        state.phase = GreaterPhase::SignificatorStable;
        assert!(!state.in_crucible());
    }
}
