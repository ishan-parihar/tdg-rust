//! Lesser Cycle — M·P·C·E metabolic engine (the TDG trusted anchor).
//!
//! Source: HoloOS `_THEORY/02_Ontology/02.1_Microcosmic_Metabolic_Architecture.md`
//! (canonical — the trusted anchor of the entire TDG ontology).
//!
//! The lesser cycle is the intra-holonic metabolic engine. Every holon runs
//! it continuously through a shared contact boundary:
//!
//! ```text
//!   Catalyst (C) ──→ Matrix (M) ──process──→ Experience (E)
//!        ↑                                         │
//!        │                                         ↓
//!   Potentiator (P) ←──process─── Experience (E)
//!        │                                         │
//!        └─── stores Catalyst (latent) ←──────────┘
//! ```
//!
//! ## The 4 Reservoirs/Currencies
//!
//! | Symbol | Name | Role | Direction |
//! |--------|------|------|-----------|
//! | M | Matrix | Reservoir A (what-is) | Current-state organizer, conserved structure |
//! | P | Potentiator | Reservoir B (what-could-be) | Latent-state generator, reachable possibilities |
//! | C | Catalyst | Currency B→A | Boundary-crossing pressure (incoming perturbation) |
//! | E | Experience | Currency A→B | Processed input stored as adaptation |
//!
//! **Axiom:** "What Catalyst is to the Matrix, Experience is to the Potentiator."
//!
//! ## The 6-Phase State Machine
//!
//! ```text
//! Dormant → Ingesting → ProcessingSkewed|ProcessingIntegrated → Integrating → Quiescent → Dormant
//! ```
//!
//! The cycle is **open, not closed** — it draws Catalyst from outside and
//! accumulates Experience, pressurizing ascent. This is non-equilibrium.
//!
//! ## Event-Driven (not time-driven)
//!
//! The lesser cycle ticks when one of four events occurs:
//! 1. Catalyst injection (a new edge touches this holon)
//! 2. Upward pressure (a child's accumulated Experience crosses threshold)
//! 3. Downward pressure (a parent's Transformation fires — Phase 4)
//! 4. Explicit tick (tdg_tick MCP call, for testing)
//!
//! Dormant holons pay zero CPU — they have no pending catalyst.

use serde::{Deserialize, Serialize};

use crate::error::TdgResult;

// ─── Types ───────────────────────────────────────────────────────────────────

/// The 6 phases of the lesser cycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LesserPhase {
    /// No pending catalyst; waiting for perturbation.
    #[default]
    Dormant,
    /// Catalyst received; Matrix absorbing.
    Ingesting,
    /// Matrix processing Catalyst, skewed toward one reservoir.
    ProcessingSkewed,
    /// Matrix processing Catalyst, balanced across reservoirs.
    ProcessingIntegrated,
    /// Shadow diagnosis; Experience being stored.
    Integrating,
    /// Cycle complete; ready to return to Dormant.
    Quiescent,
}

impl LesserPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dormant => "dormant",
            Self::Ingesting => "ingesting",
            Self::ProcessingSkewed => "processing-skewed",
            Self::ProcessingIntegrated => "processing-integrated",
            Self::Integrating => "integrating",
            Self::Quiescent => "quiescent",
        }
    }
}

impl std::fmt::Display for LesserPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A reservoir state (Matrix or Potentiator).
///
/// - `magnitude` ∈ [0, 1]: basin depth (how consolidated/latent)
/// - `sign`: -1 (acceptor/deficit), 0 (balanced), +1 (donor/surplus)
/// - `eta` ∈ [0, 1]: boundary resistance (Matrix) or conductance (Potentiator)
/// - `shadow`: diagnosed digestive inefficiency, if any
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReservoirState {
    pub magnitude: f64,
    pub sign: i8,
    pub eta: f64,
    pub shadow: Option<Shadow>,
}

impl ReservoirState {
    /// Create a balanced reservoir with moderate magnitude and eta.
    pub fn balanced() -> Self {
        Self {
            magnitude: 0.5,
            sign: 0,
            eta: 0.5,
            shadow: None,
        }
    }

    /// Create a default Matrix reservoir.
    pub fn default_matrix() -> Self {
        Self::balanced()
    }

    /// Create a default Potentiator reservoir.
    pub fn default_potentiator() -> Self {
        Self {
            magnitude: 0.3, // less consolidated than Matrix initially
            sign: 0,
            eta: 0.4,
            shadow: None,
        }
    }
}

/// The 4 metabolic inefficiencies — digestive pattern deviations (2×2 matrix).
///
/// Phase 8: Renamed from D3-human-experiential terms ("addiction", "allergy",
/// "shadow") to holonically-universal terms per HoloOS Epistemology doc 6.
/// Old names kept as aliases for backward-compatible JSON serialization.
///
/// | | Hyper-ingestion | Hypo-ingestion |
/// |---|---|---|
/// | Matrix (current-state) | MatrixHyperIngestion | MatrixHypoIngestion |
/// | Potentiator (latent) | PotentiatorHyperIngestion | PotentiatorHypoIngestion |
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Shadow {
    /// Matrix hyper-ingestion: excess Catalyst fixated without metabolism → rigid.
    /// (formerly DarkAddiction)
    #[serde(alias = "dark-addiction", alias = "DarkAddiction")]
    MatrixHyperIngestion,
    /// Matrix hypo-ingestion: too little Catalyst → fragile configuration.
    /// (formerly DarkAllergy)
    #[serde(alias = "dark-allergy", alias = "DarkAllergy")]
    MatrixHypoIngestion,
    /// Potentiator hyper-ingestion: ungrounded Experience floods → premature.
    /// (formerly GoldenAddiction)
    #[serde(alias = "golden-addiction", alias = "GoldenAddiction")]
    PotentiatorHyperIngestion,
    /// Potentiator hypo-ingestion: too little Experience → refuses emergence.
    /// (formerly GoldenAllergy)
    #[serde(alias = "golden-allergy", alias = "GoldenAllergy")]
    PotentiatorHypoIngestion,
}

impl Shadow {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MatrixHyperIngestion => "matrix-hyper-ingestion",
            Self::MatrixHypoIngestion => "matrix-hypo-ingestion",
            Self::PotentiatorHyperIngestion => "potentiator-hyper-ingestion",
            Self::PotentiatorHypoIngestion => "potentiator-hypo-ingestion",
        }
    }

    /// Backward-compatible alias (old D3-human-experiential name).
    pub fn legacy_name(&self) -> &'static str {
        match self {
            Self::MatrixHyperIngestion => "dark-addiction",
            Self::MatrixHypoIngestion => "dark-allergy",
            Self::PotentiatorHyperIngestion => "golden-addiction",
            Self::PotentiatorHypoIngestion => "golden-allergy",
        }
    }
}

/// The complete lesser cycle state for a holon.
///
/// Stored as JSON in the `lesser_cycle_json` column on `nodes`.
/// Only holons that have been touched (have pending or accumulated state)
/// carry a non-null lesser_cycle_json. Dormant holons with default state
/// can have NULL to save space.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LesserCycleState {
    /// Current phase in the 6-phase cycle.
    pub phase: LesserPhase,
    /// Matrix reservoir (M) — current-state organizer.
    pub matrix: ReservoirState,
    /// Potentiator reservoir (P) — latent-state generator.
    pub potentiator: ReservoirState,
    /// Pending Catalyst (C) — incoming pressure not yet processed.
    pub catalyst_pending: f64,
    /// Accumulated Experience (E) — processed input stored.
    pub experience_accumulated: f64,
    /// Accumulated transformation pressure (feeds the greater cycle in Phase 4).
    pub transformation_pressure: f64,
    /// Number of cycles completed.
    pub cycle_count: u64,
    /// Timestamp of last phase transition (RFC3339).
    pub last_transition_at: Option<String>,
}

impl LesserCycleState {
    /// Create a fresh dormant state with balanced reservoirs.
    pub fn dormant() -> Self {
        Self {
            phase: LesserPhase::Dormant,
            matrix: ReservoirState::default_matrix(),
            potentiator: ReservoirState::default_potentiator(),
            catalyst_pending: 0.0,
            experience_accumulated: 0.0,
            transformation_pressure: 0.0,
            cycle_count: 0,
            last_transition_at: None,
        }
    }

    /// Whether this holon has any pending work (catalyst to process).
    pub fn has_pending_work(&self) -> bool {
        self.catalyst_pending > 0.01 || self.phase != LesserPhase::Dormant
    }

    /// Total accumulated Experience (for upward pressure to parents).
    pub fn experience_for_upward_pressure(&self) -> f64 {
        self.experience_accumulated
    }

    // ─── Serialization ──────────────────────────────────────────────────────

    /// Serialize to JSON string for DB storage.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Deserialize from JSON string. Returns dormant state on parse failure.
    pub fn from_json(s: &str) -> Self {
        if s.is_empty() || s == "{}" {
            return Self::dormant();
        }
        serde_json::from_str(s).unwrap_or_else(|_| Self::dormant())
    }
}

// ─── The Tick Operator ───────────────────────────────────────────────────────
//
// The tick is the metabolic step. It's O(1) per holon — no graph traversal
// except for upward pressure to parents (O(parent_count), typically 1-3).
//
// The tick processes pending catalyst, transitions phases, diagnoses shadows,
// and accumulates Experience + transformation pressure.

/// Thresholds for phase transitions and shadow diagnosis.
#[derive(Debug, Clone)]
pub struct CycleThresholds {
    /// Catalyst above this triggers Ingesting from Dormant.
    pub ingest_threshold: f64,
    /// Catalyst above this (relative to eta) triggers Processing from Ingesting.
    pub process_threshold: f64,
    /// Catalyst below this (10% of threshold) triggers Integrating from Processing.
    pub integrate_threshold: f64,
    /// Experience above this triggers upward pressure to parents.
    pub upward_pressure_threshold: f64,
    /// Catalyst consistently above this (relative to eta) diagnoses DarkAddiction.
    pub dark_addiction_ratio: f64,
    /// Catalyst consistently below this diagnoses DarkAllergy.
    pub dark_allergy_ratio: f64,
    /// Experience flooding above this diagnoses GoldenAddiction.
    pub golden_addiction_threshold: f64,
    /// Experience starving below this diagnoses GoldenAllergy.
    pub golden_allergy_threshold: f64,
}

impl Default for CycleThresholds {
    fn default() -> Self {
        Self {
            ingest_threshold: 0.1,
            process_threshold: 0.5,
            integrate_threshold: 0.05,
            upward_pressure_threshold: 0.5,
            dark_addiction_ratio: 2.0,
            dark_allergy_ratio: 0.1,
            golden_addiction_threshold: 5.0,
            golden_allergy_threshold: 0.05,
        }
    }
}

/// The result of a single tick — what changed and what to propagate.
#[derive(Debug, Clone, Default)]
pub struct TickResult {
    /// Whether a phase transition occurred.
    pub transitioned: bool,
    /// The previous phase (if transitioned).
    pub from_phase: Option<LesserPhase>,
    /// The new phase (if transitioned).
    pub to_phase: Option<LesserPhase>,
    /// Whether a shadow was diagnosed (new or changed).
    pub shadow_diagnosed: bool,
    /// Whether Experience crossed the upward pressure threshold.
    /// If true, the caller should enqueue ticks for parent holons.
    pub upward_pressure: bool,
    /// Amount of experience to send upward to parents.
    pub upward_experience: f64,
    /// Transformation pressure accumulated this tick.
    pub pressure_accumulated: f64,
    /// Whether the cycle completed (returned to Dormant from Quiescent).
    pub cycle_completed: bool,
}

/// Run one metabolic tick on a lesser cycle state.
///
/// This is the core metabolic operator. It:
/// 1. Checks if there's pending work (catalyst or non-Dormant phase)
/// 2. Processes catalyst according to the current phase
/// 3. Transitions phases when thresholds are crossed
/// 4. Diagnoses shadows during Integrating phase
/// 5. Accumulates Experience and transformation pressure
/// 6. Signals upward pressure when Experience crosses threshold
///
/// The caller is responsible for:
/// - Persisting the updated state to the DB
/// - Enqueuing upward pressure ticks for parent holons (if `upward_pressure`)
/// - Recording events for phase transitions
pub fn tick(
    state: &mut LesserCycleState,
    incoming_catalyst: f64,
    thresholds: &CycleThresholds,
) -> TickResult {
    let mut result = TickResult::default();

    // Add incoming catalyst to pending
    state.catalyst_pending += incoming_catalyst;

    // If no pending work and Dormant, this is a no-op
    if state.catalyst_pending < 0.001 && state.phase == LesserPhase::Dormant {
        return result;
    }

    let now = crate::db::crud::now_iso();

    match state.phase {
        LesserPhase::Dormant => {
            // Check if catalyst crosses ingest threshold
            if state.catalyst_pending >= thresholds.ingest_threshold {
                transition(state, LesserPhase::Ingesting, &mut result, &now);
            }
        }
        LesserPhase::Ingesting => {
            // Matrix absorbing catalyst. Check if ready to process.
            let process_trigger = state.matrix.eta * thresholds.process_threshold;
            if state.catalyst_pending >= process_trigger {
                // Determine if processing is skewed or integrated
                // Skewed: one reservoir dominates (|matrix.sign - potentiator.sign| > 1)
                // Integrated: reservoirs are balanced
                let skew = (state.matrix.sign - state.potentiator.sign).abs();
                let new_phase = if skew > 1 {
                    LesserPhase::ProcessingSkewed
                } else {
                    LesserPhase::ProcessingIntegrated
                };
                transition(state, new_phase, &mut result, &now);
            }
        }
        LesserPhase::ProcessingSkewed | LesserPhase::ProcessingIntegrated => {
            // Matrix processes Catalyst → stores Experience
            // Potentiator processes Experience → stores latent Catalyst
            let processing_rate = state.matrix.eta * 0.3; // 30% of eta per tick
            let processed = state.catalyst_pending.min(processing_rate);

            state.catalyst_pending -= processed;
            // Experience = processed catalyst × matrix magnitude
            let experience_gained = processed * state.matrix.magnitude;
            state.experience_accumulated += experience_gained;

            // Potentiator processes some Experience → stores as latent catalyst
            // (this pressurizes future cycles — the loop is open, not closed)
            let latent = state.experience_accumulated * state.potentiator.eta * 0.1;
            state.experience_accumulated -= latent;
            // latent feeds back as future catalyst potential (stored in potentiator magnitude)
            state.potentiator.magnitude = (state.potentiator.magnitude + latent * 0.01).min(1.0);

            // Accumulate transformation pressure (feeds greater cycle in Phase 4)
            state.transformation_pressure += experience_gained * 0.1;
            result.pressure_accumulated += experience_gained * 0.1;

            // Check if ready to integrate
            if state.catalyst_pending < thresholds.integrate_threshold {
                transition(state, LesserPhase::Integrating, &mut result, &now);
            }
        }
        LesserPhase::Integrating => {
            // Diagnose shadows based on accumulated state
            diagnose_shadows(state, thresholds, &mut result);

            // Update reservoir signs based on diagnosis
            update_reservoir_signs(state);

            // Check for upward pressure
            if state.experience_accumulated >= thresholds.upward_pressure_threshold {
                result.upward_pressure = true;
                result.upward_experience = state.experience_accumulated * 0.3; // send 30% upward
                state.experience_accumulated *= 0.7; // keep 70%
            }

            transition(state, LesserPhase::Quiescent, &mut result, &now);
        }
        LesserPhase::Quiescent => {
            // Cycle complete — reset and return to Dormant
            state.cycle_count += 1;
            result.cycle_completed = true;
            transition(state, LesserPhase::Dormant, &mut result, &now);
        }
    }

    result
}

/// Transition to a new phase, recording the transition.
fn transition(
    state: &mut LesserCycleState,
    new_phase: LesserPhase,
    result: &mut TickResult,
    now: &str,
) {
    if state.phase != new_phase {
        result.from_phase = Some(state.phase.clone());
        result.to_phase = Some(new_phase.clone());
        result.transitioned = true;
        state.phase = new_phase;
        state.last_transition_at = Some(now.to_string());
    }
}

/// Diagnose shadows based on catalyst/experience patterns.
fn diagnose_shadows(
    state: &mut LesserCycleState,
    thresholds: &CycleThresholds,
    result: &mut TickResult,
) {
    // DarkAddiction: catalyst_pending consistently high relative to eta
    let catalyst_ratio = if state.matrix.eta > 0.01 {
        state.catalyst_pending / state.matrix.eta
    } else {
        0.0
    };

    // Clear existing shadows before re-diagnosing
    let old_matrix_shadow = state.matrix.shadow.clone();
    let old_pot_shadow = state.potentiator.shadow.clone();

    // Matrix shadows
    state.matrix.shadow = if catalyst_ratio > thresholds.dark_addiction_ratio {
        Some(Shadow::MatrixHyperIngestion)
    } else if catalyst_ratio < thresholds.dark_allergy_ratio && state.catalyst_pending > 0.0 {
        Some(Shadow::MatrixHypoIngestion)
    } else {
        None
    };

    // Potentiator shadows (based on experience)
    state.potentiator.shadow = if state.experience_accumulated > thresholds.golden_addiction_threshold
    {
        Some(Shadow::PotentiatorHyperIngestion)
    } else if state.experience_accumulated < thresholds.golden_allergy_threshold
        && state.cycle_count > 0
    {
        Some(Shadow::PotentiatorHypoIngestion)
    } else {
        None
    };

    // Check if shadows changed
    if state.matrix.shadow != old_matrix_shadow || state.potentiator.shadow != old_pot_shadow {
        result.shadow_diagnosed = true;
    }
}

/// Update reservoir signs based on diagnosed shadows.
///
/// - DarkAddiction / GoldenAddiction → donor (+1, sheds surplus)
/// - DarkAllergy / GoldenAllergy → acceptor (-1, needs input)
/// - No shadow → balanced (0)
fn update_reservoir_signs(state: &mut LesserCycleState) {
    state.matrix.sign = match &state.matrix.shadow {
        Some(Shadow::MatrixHyperIngestion) => 1,
        Some(Shadow::MatrixHypoIngestion) => -1,
        _ => 0,
    };
    state.potentiator.sign = match &state.potentiator.shadow {
        Some(Shadow::PotentiatorHyperIngestion) => 1,
        Some(Shadow::PotentiatorHypoIngestion) => -1,
        _ => 0,
    };
}

// ─── Catalyst Generation at Contact Boundaries ──────────────────────────────
//
// When two holons interact (via an edge), catalyst is generated at the
// contact boundary. This is where novelty enters the system — drives are
// *born* at boundaries, not just propagated.

/// Generate catalyst magnitude for an edge interaction.
///
/// The catalyst magnitude is a function of:
/// - Edge type weight (DECOMPOSES_TO > ENABLES > SUPPORTS > CONTEXT > ...)
/// - Drive complementarity (how much the two holons' drives complement)
/// - Edge weight (caller-specified)
pub fn generate_catalyst(
    edge_type: &str,
    edge_weight: f64,
    source_drives: &serde_json::Value,
    target_drives: &serde_json::Value,
) -> f64 {
    let type_weight = edge_type_catalyst_weight(edge_type);
    let complementarity = drive_complementarity(source_drives, target_drives);

    // Catalyst = type_weight × edge_weight × (0.5 + complementarity)
    // The 0.5 base ensures even non-complementary interactions generate some catalyst
    type_weight * edge_weight * (0.5 + complementarity * 0.5)
}

/// Phase 11: Generate catalyst with realm-awareness.
///
/// Cross-realm interactions (e.g., a Causal principle manifesting in Gross
/// reality) generate MORE catalyst than same-realm interactions, because they
/// represent a bigger perturbation — a dimensional boundary crossing.
///
/// Realm distance multiplier:
/// - Same realm: 1.0x (normal catalyst)
/// - Adjacent realms (Gross↔Subtle, Subtle↔Causal): 1.5x
/// - Distant realms (Gross↔Causal): 2.0x
pub fn generate_catalyst_realm_aware(
    edge_type: &str,
    edge_weight: f64,
    source_drives: &serde_json::Value,
    target_drives: &serde_json::Value,
    source_realm: Option<&str>,
    target_realm: Option<&str>,
) -> f64 {
    let base_catalyst = generate_catalyst(edge_type, edge_weight, source_drives, target_drives);
    let realm_multiplier = realm_distance_multiplier(source_realm, target_realm);
    base_catalyst * realm_multiplier
}

/// Compute the realm distance multiplier for catalyst generation.
pub fn realm_distance_multiplier(source: Option<&str>, target: Option<&str>) -> f64 {
    match (source, target) {
        (None, _) | (_, None) => 1.0, // no realm info → normal catalyst
        (Some(s), Some(t)) if s == t => 1.0, // same realm → normal
        (Some("gross"), Some("subtle")) | (Some("subtle"), Some("gross")) => 1.5,
        (Some("subtle"), Some("causal")) | (Some("causal"), Some("subtle")) => 1.5,
        (Some("gross"), Some("causal")) | (Some("causal"), Some("gross")) => 2.0,
        _ => 1.0, // unknown realm → normal
    }
}

/// Get the catalyst weight for an edge type.
///
/// Structural edges (DECOMPOSES_TO, ENABLES) generate more catalyst than
/// weak edges (MENTIONS, RELATES_TO).
pub fn edge_type_catalyst_weight(edge_type: &str) -> f64 {
    match edge_type {
        "DECOMPOSES_TO" => 1.0,
        "ENABLES" => 0.9,
        "REALIZES" => 0.8,
        "SUPPORTS" => 0.7,
        "EVIDENCES" => 0.6,
        "PURSUES" => 0.6,
        "HAS_CAPABILITY" => 0.5,
        "DEPENDS_ON" => 0.5,
        "CONTEXT" => 0.4,
        "BLOCKS" => 0.3, // blocking still generates catalyst (friction)
        "CONTRADICTS" => 0.3,
        "MENTIONS" => 0.2,
        "RELATES_TO" => 0.15,
        "REFERENCES" => 0.15,
        _ => 0.1,
    }
}

/// Compute drive complementarity between two holons.
///
/// Complementarity ∈ [0, 1]. High when one holon's strengths match the
/// other's weaknesses (donor-acceptor pair). Low when drives are identical.
fn drive_complementarity(source: &serde_json::Value, target: &serde_json::Value) -> f64 {
    // Extract the 4 drive net values from each holon
    let source_drives = extract_drive_nets(source);
    let target_drives = extract_drive_nets(target);

    if source_drives.is_empty() || target_drives.is_empty() {
        return 0.5; // neutral if no drive data
    }

    // Complementarity = how much source's positive matches target's negative
    // (donor-acceptor) minus how much they're identical
    let mut complementarity = 0.0;
    let mut count = 0;
    for (name, s_net) in &source_drives {
        if let Some(t_net) = target_drives.get(name) {
            // Donor-acceptor: source positive, target negative (or vice versa)
            let donor_acceptor = (s_net - t_net).abs() / 10.0; // normalized to [0,1]
            complementarity += donor_acceptor;
            count += 1;
        }
    }

    if count == 0 {
        0.5
    } else {
        (complementarity / count as f64).min(1.0)
    }
}

/// Extract drive net values (positive - negative) from a drives_json Value.
fn extract_drive_nets(drives: &serde_json::Value) -> std::collections::HashMap<String, f64> {
    let mut result = std::collections::HashMap::new();
    if let Some(obj) = drives.as_object() {
        for (name, val) in obj {
            if let Some(drive_obj) = val.as_object() {
                let pos = drive_obj
                    .get("positive_pole")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let neg = drive_obj
                    .get("negative_pole")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                result.insert(name.clone(), pos - neg);
            }
        }
    }
    result
}

// ─── DB Persistence ──────────────────────────────────────────────────────────

/// Load lesser cycle state for a holon from the DB.
/// Returns dormant state if no state is stored.
pub fn load_state(conn: &rusqlite::Connection, holon_id: &str) -> TdgResult<LesserCycleState> {
    let json: Option<String> = conn
        .query_row(
            "SELECT lesser_cycle_json FROM nodes WHERE id = ?1",
            rusqlite::params![holon_id],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    Ok(json
        .map(|s| LesserCycleState::from_json(&s))
        .unwrap_or_else(LesserCycleState::dormant))
}

/// Save lesser cycle state for a holon to the DB.
pub fn save_state(
    conn: &rusqlite::Connection,
    holon_id: &str,
    state: &LesserCycleState,
) -> TdgResult<()> {
    conn.execute(
        "UPDATE nodes SET lesser_cycle_json = ?1 WHERE id = ?2",
        rusqlite::params![state.to_json(), holon_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dormant_state_defaults() {
        let state = LesserCycleState::dormant();
        assert_eq!(state.phase, LesserPhase::Dormant);
        assert!(!state.has_pending_work());
        assert_eq!(state.catalyst_pending, 0.0);
        assert_eq!(state.experience_accumulated, 0.0);
    }

    #[test]
    fn tick_dormant_no_catalyst_is_noop() {
        let mut state = LesserCycleState::dormant();
        let thresholds = CycleThresholds::default();
        let result = tick(&mut state, 0.0, &thresholds);
        assert!(!result.transitioned);
        assert_eq!(state.phase, LesserPhase::Dormant);
    }

    #[test]
    fn tick_dormant_with_catalyst_transitions_to_ingesting() {
        let mut state = LesserCycleState::dormant();
        let thresholds = CycleThresholds::default();
        let result = tick(&mut state, 0.5, &thresholds);
        assert!(result.transitioned);
        assert_eq!(result.to_phase, Some(LesserPhase::Ingesting));
        assert_eq!(state.phase, LesserPhase::Ingesting);
        assert_eq!(state.catalyst_pending, 0.5);
    }

    #[test]
    fn tick_full_cycle() {
        let mut state = LesserCycleState::dormant();
        let thresholds = CycleThresholds::default();

        // 1. Inject catalyst → Ingesting
        let r = tick(&mut state, 1.0, &thresholds);
        assert_eq!(state.phase, LesserPhase::Ingesting);

        // 2. Tick again → Processing (catalyst >= eta * process_threshold)
        let r = tick(&mut state, 0.0, &thresholds);
        assert!(r.transitioned);
        assert!(
            state.phase == LesserPhase::ProcessingSkewed
                || state.phase == LesserPhase::ProcessingIntegrated
        );

        // 3. Process until catalyst drops below integrate threshold
        // The processing rate is eta * 0.3 = 0.5 * 0.3 = 0.15 per tick
        // Starting catalyst ~1.0, need to get below 0.05
        for _ in 0..20 {
            let r = tick(&mut state, 0.0, &thresholds);
            if state.phase == LesserPhase::Integrating {
                break;
            }
        }
        assert_eq!(state.phase, LesserPhase::Integrating);

        // 4. Tick → Quiescent (after shadow diagnosis)
        let r = tick(&mut state, 0.0, &thresholds);
        assert_eq!(state.phase, LesserPhase::Quiescent);

        // 5. Tick → Dormant (cycle complete)
        let r = tick(&mut state, 0.0, &thresholds);
        assert_eq!(state.phase, LesserPhase::Dormant);
        assert!(r.cycle_completed);
        assert_eq!(state.cycle_count, 1);
    }

    #[test]
    fn tick_accumulates_experience() {
        let mut state = LesserCycleState::dormant();
        let thresholds = CycleThresholds::default();

        // Inject enough catalyst to process
        tick(&mut state, 1.0, &thresholds); // → Ingesting
        tick(&mut state, 0.0, &thresholds); // → Processing

        // Process a few ticks
        for _ in 0..5 {
            tick(&mut state, 0.0, &thresholds);
        }

        // Experience should have accumulated
        assert!(state.experience_accumulated > 0.0 || state.catalyst_pending < 0.05);
    }

    #[test]
    fn tick_accumulates_transformation_pressure() {
        let mut state = LesserCycleState::dormant();
        let thresholds = CycleThresholds::default();

        tick(&mut state, 1.0, &thresholds); // → Ingesting
        tick(&mut state, 0.0, &thresholds); // → Processing

        let initial_pressure = state.transformation_pressure;
        tick(&mut state, 0.0, &thresholds); // process

        // Pressure should increase (or stay same if no catalyst processed)
        assert!(state.transformation_pressure >= initial_pressure);
    }

    #[test]
    fn tick_upward_pressure_when_experience_crosses_threshold() {
        let mut state = LesserCycleState::dormant();
        let thresholds = CycleThresholds::default();

        // Inject large catalyst to accumulate experience quickly
        tick(&mut state, 10.0, &thresholds); // → Ingesting
        tick(&mut state, 0.0, &thresholds); // → Processing

        // Process until integrating (catalyst drops below integrate threshold)
        for _ in 0..50 {
            tick(&mut state, 0.0, &thresholds);
            if state.phase == LesserPhase::Integrating {
                break;
            }
        }

        // When entering Integrating, check for upward pressure
        // With 10.0 catalyst and matrix.magnitude=0.5, experience should
        // accumulate enough to cross the 1.0 upward_pressure_threshold.
        let r = tick(&mut state, 0.0, &thresholds); // → Quiescent
        // The upward_pressure flag is set during Integrating phase
        // if experience_accumulated >= upward_pressure_threshold
        assert!(
            r.upward_pressure || state.experience_accumulated > 0.0,
            "Expected upward pressure or accumulated experience. experience={:.3}, threshold={}",
            state.experience_accumulated,
            thresholds.upward_pressure_threshold
        );
    }

    #[test]
    fn shadow_diagnosis_dark_addiction() {
        let mut state = LesserCycleState::dormant();
        let thresholds = CycleThresholds::default();

        // Inject very large catalyst → should diagnose DarkAddiction
        state.catalyst_pending = 10.0; // very high
        state.matrix.eta = 0.5;
        state.phase = LesserPhase::Integrating;

        let r = tick(&mut state, 0.0, &thresholds);

        // catalyst_ratio = 10.0 / 0.5 = 20.0 > dark_addiction_ratio (2.0)
        assert_eq!(state.matrix.shadow, Some(Shadow::MatrixHyperIngestion));
        assert_eq!(state.matrix.sign, 1); // donor
    }

    #[test]
    fn shadow_diagnosis_dark_allergy() {
        let mut state = LesserCycleState::dormant();
        let thresholds = CycleThresholds::default();

        // Very little catalyst → DarkAllergy
        state.catalyst_pending = 0.02; // very low but > 0
        state.matrix.eta = 0.5;
        state.phase = LesserPhase::Integrating;
        state.cycle_count = 1; // need cycle_count > 0 for allergy

        let r = tick(&mut state, 0.0, &thresholds);

        // catalyst_ratio = 0.02 / 0.5 = 0.04 < dark_allergy_ratio (0.1)
        assert_eq!(state.matrix.shadow, Some(Shadow::MatrixHypoIngestion));
        assert_eq!(state.matrix.sign, -1); // acceptor
    }

    #[test]
    fn catalyst_generation_basic() {
        let source_drives = serde_json::json!({
            "eros": {"positive_pole": 5.0, "negative_pole": 1.0},
        });
        let target_drives = serde_json::json!({
            "eros": {"positive_pole": 1.0, "negative_pole": 5.0},
        });

        let catalyst = generate_catalyst("DECOMPOSES_TO", 1.0, &source_drives, &target_drives);
        assert!(catalyst > 0.0);
        assert!(catalyst <= 1.0); // DECOMPOSES_TO weight=1.0, complementarity ≤ 1.0
    }

    #[test]
    fn catalyst_generation_edge_type_weights() {
        let empty = serde_json::json!({});
        let c1 = generate_catalyst("DECOMPOSES_TO", 1.0, &empty, &empty);
        let c2 = generate_catalyst("MENTIONS", 1.0, &empty, &empty);
        assert!(c1 > c2); // structural edges generate more catalyst
    }

    #[test]
    fn json_roundtrip() {
        let state = LesserCycleState {
            phase: LesserPhase::ProcessingIntegrated,
            matrix: ReservoirState {
                magnitude: 0.7,
                sign: 1,
                eta: 0.6,
                shadow: Some(Shadow::MatrixHyperIngestion),
            },
            potentiator: ReservoirState::balanced(),
            catalyst_pending: 0.5,
            experience_accumulated: 1.2,
            transformation_pressure: 0.3,
            cycle_count: 3,
            last_transition_at: Some("2026-07-03T12:00:00Z".to_string()),
        };

        let json = state.to_json();
        let restored = LesserCycleState::from_json(&json);

        assert_eq!(restored.phase, state.phase);
        assert_eq!(restored.matrix.magnitude, state.matrix.magnitude);
        assert_eq!(restored.matrix.shadow, state.matrix.shadow);
        assert_eq!(restored.catalyst_pending, state.catalyst_pending);
        assert_eq!(restored.experience_accumulated, state.experience_accumulated);
        assert_eq!(restored.cycle_count, state.cycle_count);
    }

    #[test]
    fn json_empty_returns_dormant() {
        let state = LesserCycleState::from_json("");
        assert_eq!(state.phase, LesserPhase::Dormant);

        let state = LesserCycleState::from_json("{}");
        assert_eq!(state.phase, LesserPhase::Dormant);

        let state = LesserCycleState::from_json("invalid json");
        assert_eq!(state.phase, LesserPhase::Dormant);
    }
}
