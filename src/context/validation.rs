//! 5-Gate Validation — epistemic gates for synthesis submission.
//!
//! Source: HoloOS `_THEORY/01_Epistemology/` (Grounding Discipline, Red-Team
//! Protocol, Derivation Patterns, Type Validation Protocol)
//!
//! Every synthesis submitted by an AI agent must pass 5 gates before it can
//! be elevated above `ai-draft`. The gates enforce the TDG epistemological
//! standard.
//!
//! ## The 5 Gates
//!
//! 1. **Grounding** — The synthesis must cite at least one canonical node
//!    (the "anchor"). This ensures the synthesis is grounded in validated
//!    knowledge, not floating.
//!
//! 2. **Failure-mode** — The synthesis must not contain any of the 5 QIM
//!    failure modes + humanistic reduction:
//!    - Borrowed rigor (math symbols without derivation)
//!    - Orthogonality violation (Type derived from Stage)
//!    - Numerology (cardinality matching without isomorphism)
//!    - Misplaced invariant (invariant attached to wrong feature)
//!    - Unexamined flagship analogy (central analogy untested)
//!    - Humanistic reduction (cosmological function collapsed to psychology)
//!
//! 3. **Joint validation** — Open joints must be labeled. A synthesis claiming
//!    `canonical` status cannot have unvalidated load-bearing joints.
//!
//! 4. **Cosmological scope** — Invariant claims must cite ≥2 scales (atom AND
//!    galaxy). Single-scale claims must be relabeled as decoration.
//!
//! 5. **Provenance completeness** — Required provenance fields must be present:
//!    agent name, source, derivation pattern.

use serde::{Deserialize, Serialize};

use crate::error::TdgResult;

// ─── Types ───────────────────────────────────────────────────────────────────

/// The result of a 5-gate validation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationReport {
    /// The synthesis being validated.
    pub synthesis_id: String,
    /// Overall status: "blocked", "passed", or "failed".
    pub overall_status: String,
    /// What status the synthesis can be elevated to (always ≤ ai-draft for AI).
    pub can_elevate_to: String,
    /// Individual gate results.
    pub gates: Vec<GateResult>,
    /// Timestamp of validation.
    pub validated_at: String,
}

/// The result of a single gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    /// Gate name: "grounding", "failure_mode", "joint_validation",
    /// "cosmological_scope", "provenance_completeness".
    pub gate: String,
    /// Whether the gate passed.
    pub passed: bool,
    /// Whether the gate blocked (failure stops elevation).
    pub blocked: bool,
    /// Human-readable message.
    pub message: String,
}

impl GateResult {
    fn passed(gate: &str, message: &str) -> Self {
        Self {
            gate: gate.to_string(),
            passed: true,
            blocked: false,
            message: message.to_string(),
        }
    }

    fn blocked(gate: &str, message: &str) -> Self {
        Self {
            gate: gate.to_string(),
            passed: false,
            blocked: true,
            message: message.to_string(),
        }
    }
}

/// The 5 QIM failure modes + humanistic reduction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureMode {
    BorrowedRigor,
    OrthogonalityViolation,
    NumerologyNotIsomorphism,
    MisplacedInvariant,
    UnexaminedFlagshipAnalogy,
    HumanisticReduction,
}

impl FailureMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BorrowedRigor => "borrowed_rigor",
            Self::OrthogonalityViolation => "orthogonality_violation",
            Self::NumerologyNotIsomorphism => "numerology_not_isomorphism",
            Self::MisplacedInvariant => "misplaced_invariant",
            Self::UnexaminedFlagshipAnalogy => "unexamined_flagship_analogy",
            Self::HumanisticReduction => "humanistic_reduction",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::BorrowedRigor => "Math/formalism symbols present without derivation pattern",
            Self::OrthogonalityViolation => "Type derived from Stage (Type⊥Stage violated)",
            Self::NumerologyNotIsomorphism => "Cardinality matching without structure-preserving map",
            Self::MisplacedInvariant => "Invariant attached to wrong feature of the witness",
            Self::UnexaminedFlagshipAnalogy => "Central analogy assumed rather than tested at breaking points",
            Self::HumanisticReduction => "Cosmological function collapsed to human-psychological feature",
        }
    }
}

/// Provenance metadata for a synthesis submission.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SynthesisProvenance {
    /// The agent that produced this synthesis.
    pub agent_name: String,
    /// The source (e.g. "reflect_tool", "tdg_submit_synthesis").
    pub source: String,
    /// The derivation pattern: "structural-mirroring", "fractal-recursion",
    /// "invariant-vs-decoration", "witness-corroboration", or "none".
    pub derivation_pattern: String,
    /// Whether the synthesis claims to be an invariant (scale-free).
    pub invariant_claimed: bool,
    /// Whether the synthesis acknowledges its decorations (scale-local).
    pub decorations_acknowledged: bool,
    /// Whether the synthesis has open joints (unvalidated load-bearing claims).
    pub has_open_joints: bool,
    /// Target synthesis status the synthesis claims.
    pub target_status: String,
}

// ─── Validation ──────────────────────────────────────────────────────────────

/// Run the 5-gate validation on a synthesis.
///
/// # Arguments
/// * `conn` - SQLite connection
/// * `synthesis_id` - The node ID of the synthesis being validated
/// * `provenance` - Provenance metadata
/// * `synthesis_text` - The text content of the synthesis (for failure-mode checks)
pub fn validate(
    conn: &rusqlite::Connection,
    synthesis_id: &str,
    provenance: &SynthesisProvenance,
    synthesis_text: &str,
) -> TdgResult<ValidationReport> {
    let mut gates = Vec::new();

    // Gate 1: Grounding
    gates.push(gate1_grounding(conn, synthesis_id)?);

    // Gate 2: Failure-mode
    gates.push(gate2_failure_mode(provenance, synthesis_text));

    // Gate 3: Joint validation
    gates.push(gate3_joint_validation(provenance));

    // Gate 4: Cosmological scope
    gates.push(gate4_cosmological_scope(conn, synthesis_id, provenance)?);

    // Gate 5: Provenance completeness
    gates.push(gate5_provenance_completeness(provenance));

    // Determine overall status
    let any_blocked = gates.iter().any(|g| g.blocked);
    let all_passed = gates.iter().all(|g| g.passed);

    let (overall_status, can_elevate_to) = if any_blocked {
        ("blocked".to_string(), "ai-draft".to_string())
    } else if all_passed {
        ("passed".to_string(), "ai-draft".to_string())
    } else {
        ("failed".to_string(), "ai-draft".to_string())
    };

    Ok(ValidationReport {
        synthesis_id: synthesis_id.to_string(),
        overall_status,
        can_elevate_to,
        gates,
        validated_at: crate::db::crud::now_iso(),
    })
}

/// Gate 1: Grounding — the synthesis must cite at least one canonical node.
///
/// Checks if the synthesis has EVIDENCES edges pointing to canonical nodes.
fn gate1_grounding(conn: &rusqlite::Connection, synthesis_id: &str) -> TdgResult<GateResult> {
    // Check if the synthesis cites any canonical node via EVIDENCES edges
    let canonical_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges e
         JOIN nodes n ON n.id = e.target_id
         WHERE e.source_id = ?1
           AND e.edge_type = 'EVIDENCES'
           AND e.valid_to IS NULL
           AND n.synthesis_status = 'canonical'",
        rusqlite::params![synthesis_id],
        |row| row.get(0),
    )?;

    // Also check for canonical-hypothesis (still grounded, just not fully validated)
    let hypothesis_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges e
         JOIN nodes n ON n.id = e.target_id
         WHERE e.source_id = ?1
           AND e.edge_type = 'EVIDENCES'
           AND e.valid_to IS NULL
           AND n.synthesis_status = 'canonical-hypothesis'",
        rusqlite::params![synthesis_id],
        |row| row.get(0),
    )?;

    if canonical_count > 0 {
        Ok(GateResult::passed(
            "grounding",
            &format!("Cites {} canonical node(s)", canonical_count),
        ))
    } else if hypothesis_count > 0 {
        Ok(GateResult::passed(
            "grounding",
            &format!("Cites {} canonical-hypothesis node(s)", hypothesis_count),
        ))
    } else {
        Ok(GateResult::blocked(
            "grounding",
            "Synthesis does not cite any canonical or canonical-hypothesis nodes via EVIDENCES edges",
        ))
    }
}

/// Gate 2: Failure-mode — check for QIM failure modes + humanistic reduction.
fn gate2_failure_mode(provenance: &SynthesisProvenance, text: &str) -> GateResult {
    let detected = detect_failure_modes(provenance, text);

    if detected.is_empty() {
        GateResult::passed("failure_mode", "No QIM failure modes detected")
    } else {
        let messages: Vec<String> = detected
            .iter()
            .map(|m| format!("{}: {}", m.as_str(), m.description()))
            .collect();
        GateResult::blocked(
            "failure_mode",
            &format!("Detected: {}", messages.join("; ")),
        )
    }
}

/// Detect QIM failure modes in the synthesis text + provenance.
fn detect_failure_modes(provenance: &SynthesisProvenance, text: &str) -> Vec<FailureMode> {
    let mut modes = Vec::new();
    let lower = text.to_lowercase();

    // 1. Borrowed rigor: math symbols present but derivation_pattern is "none"
    let has_math = lower.contains("∫") || lower.contains("∑") || lower.contains("∇")
        || lower.contains("∂") || lower.contains("α") || lower.contains("β")
        || lower.contains("→") || lower.contains("⇒");
    if has_math && provenance.derivation_pattern == "none" {
        modes.push(FailureMode::BorrowedRigor);
    }

    // 2. Orthogonality violation: claims type is determined by stage
    if lower.contains("type is determined by stage")
        || lower.contains("type_class depends on stage")
        || lower.contains("type from stage")
    {
        modes.push(FailureMode::OrthogonalityViolation);
    }

    // 3. Numerology: "matches" or "equals" with cardinality but no isomorphism
    if (lower.contains("matches") || lower.contains("equals"))
        && (lower.contains("elements") || lower.contains("orbitals") || lower.contains("archetypes"))
        && !lower.contains("isomorphism")
        && !lower.contains("structure-preserving")
    {
        modes.push(FailureMode::NumerologyNotIsomorphism);
    }

    // 4. Misplaced invariant: (hard to detect programmatically — heuristic)
    // Check if the text claims an invariant but attaches it to a specific number
    if lower.contains("invariant") && provenance.invariant_claimed {
        // Check for specific numbers that look like they're doing load-bearing work
        let has_specific_numbers = lower.contains("exactly ") || lower.contains("precisely ");
        let has_derivation = provenance.derivation_pattern != "none";
        if has_specific_numbers && !has_derivation {
            modes.push(FailureMode::MisplacedInvariant);
        }
    }

    // 5. Unexamined flagship analogy: (heuristic — check if "like" or "analogous" used without "test")
    if (lower.contains("just like") || lower.contains("analogous to") || lower.contains("similar to"))
        && !lower.contains("test") && !lower.contains("breaking point")
    {
        modes.push(FailureMode::UnexaminedFlagshipAnalogy);
    }

    // 6. Humanistic reduction: cosmological terms reduced to psychology
    let cosmological_terms = lower.contains("cosmic") || lower.contains("universal")
        || lower.contains("cosmological") || lower.contains("galactic");
    let psychological_terms = lower.contains("ego") || lower.contains("trauma")
        || lower.contains("psychotherapy") || lower.contains("rumination");
    if cosmological_terms && psychological_terms && provenance.invariant_claimed {
        modes.push(FailureMode::HumanisticReduction);
    }

    modes
}

/// Gate 3: Joint validation — open joints must be labeled.
fn gate3_joint_validation(provenance: &SynthesisProvenance) -> GateResult {
    if provenance.has_open_joints && provenance.target_status == "canonical" {
        GateResult::blocked(
            "joint_validation",
            "Cannot claim canonical status with open joints. Use canonical-hypothesis instead.",
        )
    } else if provenance.has_open_joints {
        GateResult::passed(
            "joint_validation",
            "Open joints present — synthesis correctly claims hypothesis-graded status",
        )
    } else {
        GateResult::passed(
            "joint_validation",
            "No open joints — all load-bearing claims validated",
        )
    }
}

/// Gate 4: Cosmological scope — invariant claims must cite ≥2 scales.
fn gate4_cosmological_scope(
    conn: &rusqlite::Connection,
    synthesis_id: &str,
    provenance: &SynthesisProvenance,
) -> TdgResult<GateResult> {
    if !provenance.invariant_claimed {
        return Ok(GateResult::passed(
            "cosmological_scope",
            "No invariant claimed — gate not applicable",
        ));
    }

    // Check the scale codes of cited nodes (via EVIDENCES edges)
    let mut stmt = conn.prepare(
        "SELECT DISTINCT n.scale_code
         FROM edges e
         JOIN nodes n ON n.id = e.target_id
         WHERE e.source_id = ?1
           AND e.edge_type = 'EVIDENCES'
           AND e.valid_to IS NULL
           AND n.scale_code IS NOT NULL",
    )?;

    let scales: Vec<String> = stmt
        .query_map(rusqlite::params![synthesis_id], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    if scales.len() >= 2 {
        Ok(GateResult::passed(
            "cosmological_scope",
            &format!("Invariant cites {} scales: {}", scales.len(), scales.join(", ")),
        ))
    } else {
        Ok(GateResult::blocked(
            "cosmological_scope",
            &format!(
                "Invariant claim cites only {} scale(s). Must cite ≥2 scales (atom AND galaxy). Relabel as decoration.",
                scales.len()
            ),
        ))
    }
}

/// Gate 5: Provenance completeness — required fields present.
fn gate5_provenance_completeness(provenance: &SynthesisProvenance) -> GateResult {
    let mut missing = Vec::new();

    if provenance.agent_name.is_empty() {
        missing.push("agent_name");
    }
    if provenance.source.is_empty() {
        missing.push("source");
    }
    if provenance.derivation_pattern.is_empty() {
        missing.push("derivation_pattern");
    }

    if missing.is_empty() {
        GateResult::passed(
            "provenance_completeness",
            "All required provenance fields present",
        )
    } else {
        GateResult::blocked(
            "provenance_completeness",
            &format!("Missing: {}", missing.join(", ")),
        )
    }
}

// ─── DB Persistence ──────────────────────────────────────────────────────────

/// Save a validation report to the DB.
///
/// Stores the report as JSON in the synthesis_provenance table.
pub fn save_report(
    conn: &rusqlite::Connection,
    report: &ValidationReport,
) -> TdgResult<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS synthesis_validation (
            synthesis_id TEXT PRIMARY KEY,
            overall_status TEXT NOT NULL,
            can_elevate_to TEXT NOT NULL,
            gates_json TEXT NOT NULL,
            validated_at TEXT NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "INSERT OR REPLACE INTO synthesis_validation
            (synthesis_id, overall_status, can_elevate_to, gates_json, validated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            report.synthesis_id,
            report.overall_status,
            report.can_elevate_to,
            serde_json::to_string(&report.gates).unwrap_or_default(),
            report.validated_at,
        ],
    )?;

    Ok(())
}

/// Load a validation report from the DB.
pub fn load_report(
    conn: &rusqlite::Connection,
    synthesis_id: &str,
) -> TdgResult<Option<ValidationReport>> {
    let result = conn.query_row(
        "SELECT synthesis_id, overall_status, can_elevate_to, gates_json, validated_at
         FROM synthesis_validation
         WHERE synthesis_id = ?1",
        rusqlite::params![synthesis_id],
        |row| {
            let synthesis_id: String = row.get(0)?;
            let overall_status: String = row.get(1)?;
            let can_elevate_to: String = row.get(2)?;
            let gates_json: String = row.get(3)?;
            let validated_at: String = row.get(4)?;

            let gates: Vec<GateResult> =
                serde_json::from_str(&gates_json).unwrap_or_default();

            Ok(ValidationReport {
                synthesis_id,
                overall_status,
                can_elevate_to,
                gates,
                validated_at,
            })
        },
    );

    match result {
        Ok(report) => Ok(Some(report)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_fts, init_schema, run_migrations};
    use crate::models::NewNode;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    fn make_provenance() -> SynthesisProvenance {
        SynthesisProvenance {
            agent_name: "test_agent".to_string(),
            source: "test".to_string(),
            derivation_pattern: "structural-mirroring".to_string(),
            invariant_claimed: false,
            decorations_acknowledged: true,
            has_open_joints: false,
            target_status: "ai-draft".to_string(),
        }
    }

    #[test]
    fn validate_passes_all_gates() {
        let conn = setup_db();

        // Create a canonical anchor
        let anchor = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "discovery".to_string(),
                name: "Canonical anchor".to_string(),
                synthesis_status: Some("canonical".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // Create the synthesis
        let synthesis = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "synthesis".to_string(),
                name: "Test synthesis".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Link synthesis to anchor via EVIDENCES
        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: synthesis.id.clone(),
                target_id: anchor.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        );

        let provenance = make_provenance();
        let report = validate(&conn, &synthesis.id, &provenance, "A grounded synthesis").unwrap();

        assert_eq!(report.overall_status, "passed");
        assert_eq!(report.can_elevate_to, "ai-draft");
        assert_eq!(report.gates.len(), 5);
        assert!(report.gates.iter().all(|g| g.passed));
    }

    #[test]
    fn validate_blocks_on_no_grounding() {
        let conn = setup_db();

        let synthesis = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "synthesis".to_string(),
                name: "Ungrounded synthesis".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let provenance = make_provenance();
        let report = validate(&conn, &synthesis.id, &provenance, "Ungrounded").unwrap();

        assert_eq!(report.overall_status, "blocked");
        let grounding_gate = report.gates.iter().find(|g| g.gate == "grounding").unwrap();
        assert!(grounding_gate.blocked);
    }

    #[test]
    fn validate_blocks_on_borrowed_rigor() {
        let conn = setup_db();

        let anchor = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "discovery".to_string(),
                name: "Anchor".to_string(),
                synthesis_status: Some("canonical".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let synthesis = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "synthesis".to_string(),
                name: "Math synthesis".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: synthesis.id.clone(),
                target_id: anchor.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        );

        // Provenance with derivation_pattern = "none" + math symbols → borrowed rigor
        let mut provenance = make_provenance();
        provenance.derivation_pattern = "none".to_string();

        let report = validate(&conn, &synthesis.id, &provenance, "The integral ∫ of the field").unwrap();

        assert_eq!(report.overall_status, "blocked");
        let fm_gate = report.gates.iter().find(|g| g.gate == "failure_mode").unwrap();
        assert!(fm_gate.blocked);
        assert!(fm_gate.message.contains("borrowed_rigor"));
    }

    #[test]
    fn validate_blocks_on_orthogonality_violation() {
        let conn = setup_db();

        let anchor = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "discovery".to_string(),
                name: "Anchor".to_string(),
                synthesis_status: Some("canonical".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let synthesis = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "synthesis".to_string(),
                name: "Bad synthesis".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: synthesis.id.clone(),
                target_id: anchor.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        );

        let provenance = make_provenance();
        let report = validate(
            &conn,
            &synthesis.id,
            &provenance,
            "The type is determined by stage progression",
        )
        .unwrap();

        let fm_gate = report.gates.iter().find(|g| g.gate == "failure_mode").unwrap();
        assert!(fm_gate.blocked);
        assert!(fm_gate.message.contains("orthogonality_violation"));
    }

    #[test]
    fn validate_blocks_on_humanistic_reduction() {
        let conn = setup_db();

        let anchor = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "discovery".to_string(),
                name: "Anchor".to_string(),
                synthesis_status: Some("canonical".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let synthesis = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "synthesis".to_string(),
                name: "Reductionist synthesis".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: synthesis.id.clone(),
                target_id: anchor.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        );

        let mut provenance = make_provenance();
        provenance.invariant_claimed = true;

        let report = validate(
            &conn,
            &synthesis.id,
            &provenance,
            "The cosmic principle is really about ego and trauma",
        )
        .unwrap();

        let fm_gate = report.gates.iter().find(|g| g.gate == "failure_mode").unwrap();
        assert!(fm_gate.blocked);
        assert!(fm_gate.message.contains("humanistic_reduction"));
    }

    #[test]
    fn validate_blocks_on_canonical_with_open_joints() {
        let conn = setup_db();

        let anchor = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "discovery".to_string(),
                name: "Anchor".to_string(),
                synthesis_status: Some("canonical".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let synthesis = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "synthesis".to_string(),
                name: "Synthesis".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: synthesis.id.clone(),
                target_id: anchor.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        );

        let mut provenance = make_provenance();
        provenance.has_open_joints = true;
        provenance.target_status = "canonical".to_string();

        let report = validate(&conn, &synthesis.id, &provenance, "Synthesis with open joints").unwrap();

        let joint_gate = report.gates.iter().find(|g| g.gate == "joint_validation").unwrap();
        assert!(joint_gate.blocked);
    }

    #[test]
    fn validate_blocks_on_single_scale_invariant() {
        let conn = setup_db();

        // Create anchor with scale_code S40 (Individual)
        let anchor = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "discovery".to_string(),
                name: "Anchor".to_string(),
                synthesis_status: Some("canonical".to_string()),
                scale_code: Some("S40".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let synthesis = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "synthesis".to_string(),
                name: "Invariant synthesis".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: synthesis.id.clone(),
                target_id: anchor.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        );

        let mut provenance = make_provenance();
        provenance.invariant_claimed = true;

        let report = validate(&conn, &synthesis.id, &provenance, "A scale-free invariant").unwrap();

        let scope_gate = report.gates.iter().find(|g| g.gate == "cosmological_scope").unwrap();
        assert!(scope_gate.blocked);
    }

    #[test]
    fn validate_passes_with_two_scales() {
        let conn = setup_db();

        // Create anchors at different scales
        let anchor1 = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "discovery".to_string(),
                name: "Individual anchor".to_string(),
                synthesis_status: Some("canonical".to_string()),
                scale_code: Some("S40".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let anchor2 = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "discovery".to_string(),
                name: "Civilizational anchor".to_string(),
                synthesis_status: Some("canonical".to_string()),
                scale_code: Some("S11".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let synthesis = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "synthesis".to_string(),
                name: "Multi-scale synthesis".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: synthesis.id.clone(),
                target_id: anchor1.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        );

        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: synthesis.id.clone(),
                target_id: anchor2.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        );

        let mut provenance = make_provenance();
        provenance.invariant_claimed = true;

        let report = validate(&conn, &synthesis.id, &provenance, "A scale-free invariant").unwrap();

        let scope_gate = report.gates.iter().find(|g| g.gate == "cosmological_scope").unwrap();
        assert!(scope_gate.passed);
    }

    #[test]
    fn validate_blocks_on_missing_provenance() {
        let conn = setup_db();

        let synthesis = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "synthesis".to_string(),
                name: "Synthesis".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let provenance = SynthesisProvenance {
            agent_name: "".to_string(), // missing
            source: "".to_string(),     // missing
            derivation_pattern: "none".to_string(),
            ..Default::default()
        };

        let report = validate(&conn, &synthesis.id, &provenance, "text").unwrap();

        let prov_gate = report
            .gates
            .iter()
            .find(|g| g.gate == "provenance_completeness")
            .unwrap();
        assert!(prov_gate.blocked);
        assert!(prov_gate.message.contains("agent_name"));
        assert!(prov_gate.message.contains("source"));
    }

    #[test]
    fn save_and_load_report() {
        let conn = setup_db();

        let report = ValidationReport {
            synthesis_id: "test-synth".to_string(),
            overall_status: "passed".to_string(),
            can_elevate_to: "ai-draft".to_string(),
            gates: vec![GateResult::passed("grounding", "ok")],
            validated_at: "2026-07-03T12:00:00Z".to_string(),
        };

        save_report(&conn, &report).unwrap();

        let loaded = load_report(&conn, "test-synth").unwrap().unwrap();
        assert_eq!(loaded.synthesis_id, "test-synth");
        assert_eq!(loaded.overall_status, "passed");
        assert_eq!(loaded.gates.len(), 1);
    }
}
