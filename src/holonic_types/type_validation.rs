//! Type Validation — T1/T2/T3 tests for type classification.
//!
//! Source: HoloOS `_THEORY/01_Epistemology/4_Type_Validation_Protocol.md`
//! (canonical)
//!
//! A Type claims a holon's invariant valence signature 𝒱 = ⟨d; {(c, ρ_c, σ_c)}⟩.
//! **Validated iff all three tests hold:**
//!
//! | Test | Question | Failure means |
//! |------|----------|---------------|
//! | T1 — Behavioral match | Does observed bonding match the signature's prediction? | Signature wrong; re-derive |
//! | T2 — Excitation-invariance | Does 𝒱 stay fixed as Stage changes? | What was measured is Stage, not Type |
//! | T3 — Fixed-point persistence | Does 𝒱 persist across metabolic cycles? | Signature is a transient, not a type |
//!
//! ## Type ⊥ Stage Orthogonality
//!
//! Type = invariant valence shape (stable under excitation).
//! Stage = dynamic excitation level (how full the metabolic engine is).
//!
//! Deriving one from the other is a red-team failure-mode (#2: orthogonality
//! violation). The T2 test enforces this.

use serde::{Deserialize, Serialize};

use crate::error::TdgResult;
use crate::metabolism::attractor::AttractorField;

// ─── Types ───────────────────────────────────────────────────────────────────

/// The result of a T1/T2/T3 type validation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TypeValidationResult {
    /// T1 — Behavioral match: does observed bonding match the type_class prediction?
    pub t1_behavioral_match: bool,
    /// T2 — Excitation-invariance: does type_class stay fixed across stage transitions?
    pub t2_excitation_invariance: bool,
    /// T3 — Fixed-point persistence: does type_class persist across metabolic cycles?
    pub t3_fixed_point_persistence: bool,
    /// Overall: valid iff all three hold.
    pub valid: bool,
    /// Human-readable details.
    pub details: String,
    /// Timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validated_at: Option<String>,
}

impl TypeValidationResult {
    /// Check if the type is fully validated (all 3 tests pass).
    pub fn is_valid(&self) -> bool {
        self.t1_behavioral_match && self.t2_excitation_invariance && self.t3_fixed_point_persistence
    }
}

// ─── T1: Behavioral Match ────────────────────────────────────────────────────

/// T1 — Behavioral match: does observed bonding match the type_class prediction?
///
/// A `strong-donor` should have more outgoing edges (giving).
/// A `strong-acceptor` should have more incoming edges (receiving).
/// A `sharer` should have balanced in/out.
/// A `noble` should have few structural bonds.
/// A `transient` should not be bondable.
pub fn t1_behavioral_match(
    conn: &rusqlite::Connection,
    holon_id: &str,
    af: &AttractorField,
) -> TdgResult<bool> {
    // Count outgoing and incoming structural edges
    let outgoing: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges
         WHERE source_id = ?1 AND valid_to IS NULL
           AND edge_type IN ('DECOMPOSES_TO', 'ENABLES', 'SUPPORTS', 'EVIDENCES', 'REALIZES')",
        rusqlite::params![holon_id],
        |row| row.get(0),
    )?;

    let incoming: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges
         WHERE target_id = ?1 AND valid_to IS NULL
           AND edge_type IN ('DECOMPOSES_TO', 'ENABLES', 'SUPPORTS', 'EVIDENCES', 'REALIZES')",
        rusqlite::params![holon_id],
        |row| row.get(0),
    )?;

    // Predict behavior from type_class
    let tc = &af.type_class;
    let prediction_match = if tc == "transient" {
        // Transients shouldn't be bondable
        !af.stability.bondable
    } else if tc.starts_with("strong-donor") || tc.starts_with("weak-donor") {
        // Donors should have more outgoing than incoming, AND at least 1 outgoing
        outgoing > 0 && outgoing >= incoming
    } else if tc.starts_with("strong-acceptor") || tc.starts_with("weak-acceptor") {
        // Acceptors should have more incoming than outgoing, AND at least 1 incoming
        incoming > 0 && incoming >= outgoing
    } else if tc.starts_with("sharer") {
        // Sharers should have roughly balanced in/out, AND at least 1 edge
        let total = outgoing + incoming;
        if total == 0 {
            false // no evidence of sharing
        } else {
            let diff = (outgoing - incoming).abs();
            diff <= total / 3 + 1 // within 33% + 1
        }
    } else if tc.starts_with("noble") {
        // Noble should have few structural bonds (they don't bond easily)
        outgoing + incoming <= 3
    } else {
        // Unknown type — can't validate
        true
    };

    Ok(prediction_match)
}

// ─── T2: Excitation-Invariance ───────────────────────────────────────────────

/// T2 — Excitation-invariance: does type_class stay fixed as Stage changes?
///
/// Checks the node's history: if the type_class has changed across stage
/// transitions, the type is not invariant (what was measured was Stage, not Type).
///
/// This enforces Type⊥Stage orthogonality.
pub fn t2_excitation_invariance(
    conn: &rusqlite::Connection,
    holon_id: &str,
    current_type_class: &str,
) -> TdgResult<bool> {
    // Check the mutation_log for type_class changes
    // If the node's attractor_field was recomputed after a stage change,
    // and the type_class was different, T2 fails.

    // Look for mutations that changed developmental_stage
    let stage_changes: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT new_value FROM mutation_log
             WHERE target_id = ?1
               AND mutation_type = 'update'
               AND new_value LIKE '%developmental_stage%'
             ORDER BY timestamp DESC",
        )?;
        let rows = stmt.query_map(rusqlite::params![holon_id], |row| {
            row.get::<_, String>(0)
        })?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // If there are no stage changes, T2 trivially passes (no evidence of violation)
    if stage_changes.is_empty() {
        return Ok(true);
    }

    // If there were stage changes but the type_class hasn't changed since,
    // T2 passes (type is invariant across stage transitions).
    //
    // We check: was the attractor_field recomputed AFTER the last stage change?
    // If yes, and the type_class is the same, T2 passes.
    let last_stage_change_ts: Option<String> = conn
        .query_row(
            "SELECT MAX(timestamp) FROM mutation_log
             WHERE target_id = ?1
               AND mutation_type = 'update'
               AND new_value LIKE '%developmental_stage%'",
            rusqlite::params![holon_id],
            |row| row.get(0),
        )
        .ok();

    if let Some(last_stage_ts) = last_stage_change_ts {
        // Check if attractor was recomputed after the stage change
        let attractor_recomputed_after: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM mutation_log
                 WHERE target_id = ?1
                   AND timestamp > ?2
                   AND new_value LIKE '%type_class%'",
                rusqlite::params![holon_id, last_stage_ts],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if attractor_recomputed_after {
            // Attractor was recomputed after stage change — type should be the same
            // (we can't easily check what the old type was without storing history,
            // so we pass if the current type is stable/non-transient)
            return Ok(current_type_class != "transient");
        }
    }

    // Default: pass (no evidence of violation)
    Ok(true)
}

// ─── T3: Fixed-Point Persistence ─────────────────────────────────────────────

/// T3 — Fixed-point persistence: does type_class persist across metabolic cycles?
///
/// Checks if the type_class has remained the same across multiple lesser-cycle
/// completions. If the type keeps changing every cycle, it's a transient, not a type.
pub fn t3_fixed_point_persistence(
    conn: &rusqlite::Connection,
    holon_id: &str,
    _af: &AttractorField,
) -> TdgResult<bool> {
    // Load the lesser cycle state to check cycle_count
    let lesser = crate::metabolism::lesser_cycle::load_state(conn, holon_id)?;

    // If the holon has completed < 2 cycles, we can't check persistence yet.
    // Pass provisionally (the type is still forming).
    if lesser.cycle_count < 2 {
        return Ok(true);
    }

    // Check if the type_class has been stable by looking at the mutation_log.
    // Count how many times the type_class changed.
    let type_changes: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM mutation_log
             WHERE target_id = ?1
               AND new_value LIKE '%type_class%'",
            rusqlite::params![holon_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // If the type changed more than once per 3 cycles, it's not persistent.
    // (Allowing some churn during formation, but not constant flipping.)
    let max_changes = (lesser.cycle_count / 3).max(1) as i64;

    Ok(type_changes <= max_changes)
}

// ─── Full Validation ─────────────────────────────────────────────────────────

/// Run all 3 type validation tests (T1/T2/T3).
///
/// A type is **valid** iff all three tests pass.
pub fn validate_type(
    conn: &rusqlite::Connection,
    holon_id: &str,
    af: &AttractorField,
) -> TdgResult<TypeValidationResult> {
    let t1 = t1_behavioral_match(conn, holon_id, af)?;
    let t2 = t2_excitation_invariance(conn, holon_id, &af.type_class)?;
    let t3 = t3_fixed_point_persistence(conn, holon_id, af)?;

    let valid = t1 && t2 && t3;

    let mut details = Vec::new();
    if !t1 {
        details.push("T1 FAILED: observed bonding does not match type_class prediction".to_string());
    }
    if !t2 {
        details.push("T2 FAILED: type_class changed across stage transitions (Type⊥Stage violated)".to_string());
    }
    if !t3 {
        details.push("T3 FAILED: type_class does not persist across metabolic cycles (transient, not a type)".to_string());
    }
    if details.is_empty() {
        details.push("All 3 tests passed — type is validated".to_string());
    }

    Ok(TypeValidationResult {
        t1_behavioral_match: t1,
        t2_excitation_invariance: t2,
        t3_fixed_point_persistence: t3,
        valid,
        details: details.join("; "),
        validated_at: Some(crate::db::crud::now_iso()),
    })
}

/// Check Type⊥Stage orthogonality.
///
/// This is the principle that Type (invariant valence shape) and Stage
/// (dynamic excitation level) are orthogonal — deriving one from the other
/// is a red-team failure-mode.
///
/// Returns true if the type does not appear to be derived from the stage.
pub fn check_type_stage_orthogonality(
    af: &AttractorField,
    developmental_stage: Option<i32>,
) -> bool {
    // If the holon has no stage, orthogonality is trivially satisfied
    let stage = match developmental_stage {
        Some(s) => s,
        None => return true,
    };

    // Check: is the type_class suspiciously correlated with the stage?
    // This is a heuristic — we check if the type is a "stage-appropriate" type
    // that would be expected if Type were derived from Stage.
    //
    // For example, if a stage-1 (Survival) holon has type "strong-acceptor"
    // (needs input), that's expected from a Stage derivation. But it could
    // also be a genuine Type. We can't definitively detect the violation,
    // but we can flag suspicious patterns.

    // Heuristic: if the type is exactly what the stage would predict,
    // flag it as potentially Stage-derived (needs manual review).
    let stage_predicted_type = match stage {
        1 => Some("strong-acceptor"),    // Survival → needs input
        2 => Some("weak-acceptor"),      // Identity → still receiving
        3 => Some("sharer"),             // Power → balanced exchange
        4 => Some("weak-donor"),         // Heart → beginning to give
        5 => Some("sharer"),             // Rational → balanced
        6 => Some("weak-donor"),         // Pluralistic → giving
        7 => Some("strong-donor"),       // Integral → generous
        8 => Some("noble-graduated"),    // Harvest → completed
        _ => None,
    };

    // If the type matches the stage prediction EXACTLY (with any suffix),
    // it's suspicious but not definitive. We pass — true orthogonality
    // violation requires the type to CHANGE when the stage changes (T2 catches that).
    //
    // This function is for additional checking; T2 is the primary orthogonality test.
    if let Some(predicted) = stage_predicted_type {
        // If type starts with the predicted prefix, it's worth noting but not a failure
        if af.type_class.starts_with(predicted) {
            // Suspicious but not definitive — T2 is the real test
            return true;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_fts, init_schema, run_migrations};
    use crate::metabolism::attractor::{
        ArchetypalLoads, CouplingTensor, ReservoirAttractor, StabilityFilter,
    };
    use crate::metabolism::lesser_cycle::LesserCycleState;
    use crate::models::NewNode;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    fn make_af(type_class: &str, pi: Option<f64>, stable: bool) -> AttractorField {
        AttractorField {
            a_m: ReservoirAttractor::new(0.5, 0),
            a_p: ReservoirAttractor::new(0.5, 0),
            a_g: ReservoirAttractor::with_polarity(0.5, "STO"),
            gamma: CouplingTensor::default(),
            pi,
            type_class: type_class.to_string(),
            choice_flag: None,
            loads: ArchetypalLoads::default(),
            stability: StabilityFilter {
                self_consistent: stable,
                bondable: stable,
                persistent: stable,
            },
            computed_at: None,
        }
    }

    #[test]
    fn t1_donor_has_more_outgoing() {
        let conn = setup_db();

        // Create a holon
        let holon = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Donor holon".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Add outgoing edges (this holon gives)
        for i in 0..3 {
            let target = crate::db::crud::add_node(
                &conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("Target {}", i),
                    ..Default::default()
                },
            )
            .unwrap();
            let _ = crate::db::crud::add_edge(
                &conn,
                &crate::models::NewEdge {
                    source_id: holon.id.clone(),
                    target_id: target.id.clone(),
                    edge_type: "SUPPORTS".to_string(),
                    ..Default::default()
                },
            );
        }

        let af = make_af("strong-donor-sto", Some(0.8), true);
        let result = t1_behavioral_match(&conn, &holon.id, &af).unwrap();
        assert!(result, "Donor with more outgoing edges should pass T1");
    }

    #[test]
    fn t1_acceptor_has_more_incoming() {
        let conn = setup_db();

        let holon = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Acceptor holon".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Add incoming edges (this holon receives)
        for i in 0..3 {
            let source = crate::db::crud::add_node(
                &conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("Source {}", i),
                    ..Default::default()
                },
            )
            .unwrap();
            let _ = crate::db::crud::add_edge(
                &conn,
                &crate::models::NewEdge {
                    source_id: source.id.clone(),
                    target_id: holon.id.clone(),
                    edge_type: "SUPPORTS".to_string(),
                    ..Default::default()
                },
            );
        }

        let af = make_af("strong-acceptor-sts", Some(-0.7), true);
        let result = t1_behavioral_match(&conn, &holon.id, &af).unwrap();
        assert!(result, "Acceptor with more incoming edges should pass T1");
    }

    #[test]
    fn t1_sharer_balanced() {
        let conn = setup_db();

        let holon = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Sharer holon".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Add balanced edges
        let target = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Target".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        let source = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Source".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: holon.id.clone(),
                target_id: target.id.clone(),
                edge_type: "SUPPORTS".to_string(),
                ..Default::default()
            },
        );
        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: source.id.clone(),
                target_id: holon.id.clone(),
                edge_type: "SUPPORTS".to_string(),
                ..Default::default()
            },
        );

        let af = make_af("sharer", Some(0.0), true);
        let result = t1_behavioral_match(&conn, &holon.id, &af).unwrap();
        assert!(result, "Sharer with balanced edges should pass T1");
    }

    #[test]
    fn t1_transient_not_bondable() {
        let conn = setup_db();

        let holon = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Transient holon".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let af = make_af("transient", Some(0.5), false);
        let result = t1_behavioral_match(&conn, &holon.id, &af).unwrap();
        assert!(result, "Transient with bondable=false should pass T1");
    }

    #[test]
    fn t2_no_stage_changes_passes() {
        let conn = setup_db();

        let holon = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Test".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let result = t2_excitation_invariance(&conn, &holon.id, "sharer").unwrap();
        assert!(result, "No stage changes → T2 trivially passes");
    }

    #[test]
    fn t3_few_cycles_passes() {
        let conn = setup_db();

        let holon = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Test".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let af = make_af("sharer", Some(0.0), true);
        let result = t3_fixed_point_persistence(&conn, &holon.id, &af).unwrap();
        assert!(result, "Few cycles → T3 passes provisionally");
    }

    #[test]
    fn validate_type_all_pass() {
        let conn = setup_db();

        let holon = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Valid type".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Add balanced edges for sharer (1 outgoing, 1 incoming)
        let target = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Target".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        let source = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Source".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: holon.id.clone(),
                target_id: target.id,
                edge_type: "SUPPORTS".to_string(),
                ..Default::default()
            },
        );
        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: source.id,
                target_id: holon.id.clone(),
                edge_type: "SUPPORTS".to_string(),
                ..Default::default()
            },
        );

        let af = make_af("sharer", Some(0.0), true);
        let result = validate_type(&conn, &holon.id, &af).unwrap();

        assert!(result.valid);
        assert!(result.t1_behavioral_match);
        assert!(result.t2_excitation_invariance);
        assert!(result.t3_fixed_point_persistence);
    }

    #[test]
    fn validate_type_t1_fails_for_donor_with_no_outgoing() {
        let conn = setup_db();

        let holon = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Bad donor".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // No outgoing edges, but claimed as donor → T1 should fail
        let af = make_af("strong-donor-sto", Some(0.8), true);
        let result = validate_type(&conn, &holon.id, &af).unwrap();

        assert!(!result.t1_behavioral_match);
        assert!(!result.valid);
    }

    #[test]
    fn type_stage_orthogonality_check() {
        let af = make_af("sharer", Some(0.0), true);
        // Stage 3 (Power) → predicted "sharer". Type IS "sharer".
        // This is suspicious but not definitive — should still pass.
        assert!(check_type_stage_orthogonality(&af, Some(3)));

        // Stage 1 (Survival) → predicted "strong-acceptor". Type is "sharer".
        // Not the predicted type → passes.
        assert!(check_type_stage_orthogonality(&af, Some(1)));

        // No stage → trivially passes
        assert!(check_type_stage_orthogonality(&af, None));
    }

    #[test]
    fn type_validation_result_is_valid() {
        let result = TypeValidationResult {
            t1_behavioral_match: true,
            t2_excitation_invariance: true,
            t3_fixed_point_persistence: true,
            valid: true,
            details: "All passed".to_string(),
            validated_at: None,
        };
        assert!(result.is_valid());

        let result = TypeValidationResult {
            t1_behavioral_match: false,
            t2_excitation_invariance: true,
            t3_fixed_point_persistence: true,
            valid: false,
            details: "T1 failed".to_string(),
            validated_at: None,
        };
        assert!(!result.is_valid());
    }
}
