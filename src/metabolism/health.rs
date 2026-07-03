//! Health Metrics — G_z, P_z, and Resonance.
//!
//! Sources:
//! - HoloOS `_THEORY/02_Ontology/02.1` §6.2 (G_z formula — canonical)
//! - HoloOS `_THEORY/02_Ontology/08.3_Gz_Pz_Deepened_Articulation.md`
//!   (deepened mechanism — canonical-hypothesis)
//! - HoloOS `_THEORY/02_Ontology/08.1` §8 (Resonance — canonical-hypothesis)
//!
//! ## G_z (Agape / Integrative Efficiency)
//!
//! ```text
//! G_z = 100 · (A_z/100 · C_z/100 · B_H · B_V)^(1/4)
//! ```
//!
//! Geometric mean of 4 factors. **Rewards balance.** Any single factor near 0
//! collapses G_z. >70 optimal, 30–70 sub-optimal, <30 collapse.
//!
//! ## P_z (Eros / Transcendental Tension)
//!
//! ```text
//! P_z = 100 · ∇Ψ · cos(θ_alignment)
//! ```
//!
//! **Rewards commitment, not balance.** Neutrality is the pathology. >50
//! optimal, <10 sinkhole of indifference.
//!
//! ## Total Health = G_z · P_z
//!
//! A holon can be efficient yet depolarized (the sinkhole). Both required.
//!
//! ## Resonance R(H1, H2)
//!
//! ```text
//! R = register_complementarity · coupling_tensor_compatibility · great_way_intersection
//! ```
//!
//! R > 0.7: strong bond. 0.3–0.7: moderate. <0.3: weak.

use serde::{Deserialize, Serialize};

use crate::error::TdgResult;
use crate::metabolism::attractor::AttractorField;
use crate::metabolism::lesser_cycle::LesserCycleState;

// ─── Health ──────────────────────────────────────────────────────────────────

const EPSILON: f64 = 1e-10;

/// The complete health metrics for a holon.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Health {
    /// Integrative efficiency ∈ [0, 100]. Rewards balance.
    pub g_z: f64,
    /// Transcendental tension ∈ [0, 100]. Rewards commitment.
    pub p_z: f64,
    /// Total health = G_z · P_z.
    pub total: f64,
    /// Matrix-side boundary resistance ∈ [0, 100].
    pub a_z: f64,
    /// Potentiator-side field conductance ∈ [0, 100].
    pub c_z: f64,
    /// Horizontal balance (Agency ↔ Communion).
    pub b_h: f64,
    /// Vertical balance (Eros ↔ Agape).
    pub b_v: f64,
    /// Structural potential gradient ∇Ψ = |P − M| / (P + M + ε).
    pub grad_psi: f64,
    /// Alignment angle in radians (0 = aligned, π/2 = neutral, π = anti-aligned).
    pub theta_alignment: f64,
    /// State classification.
    pub state: HealthState,
    /// Timestamp of computation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computed_at: Option<String>,
}

/// Health state classification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum HealthState {
    /// G_z > 70 AND P_z > 50 — optimal metabolic throughput + commitment.
    #[default]
    Optimal,
    /// G_z 30–70 OR P_z 10–50 — sub-optimal, active addictions/allergies.
    SubOptimal,
    /// G_z < 30 — severe boundary distortion.
    Collapse,
    /// P_z < 10 — depolarized: no tension between what-is and what-could-be.
    /// (formerly Sinkhole — renamed per HoloOS universal semantics protocol)
    #[serde(alias = "sinkhole", alias = "Sinkhole")]
    Depolarized,
}

impl HealthState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Optimal => "optimal",
            Self::SubOptimal => "sub-optimal",
            Self::Collapse => "collapse",
            Self::Depolarized => "depolarized",
        }
    }
}

impl Health {
    /// Compute health from lesser cycle state + attractor field.
    pub fn compute(lesser: &LesserCycleState, af: &AttractorField) -> Self {
        let m = lesser.matrix.magnitude;
        let p = lesser.potentiator.magnitude;
        let eta_m = lesser.matrix.eta;
        let eta_p = lesser.potentiator.eta;
        let c = lesser.catalyst_pending;
        let e = lesser.experience_accumulated;

        // ─── A_z: Agency coefficient ────────────────────────────────────────
        // A_z = 100 · exp(−|ln(Ω_A)|), where Ω_A = (M · η_M) / (|C| + ε)
        let omega_a = (m * eta_m) / (c.abs() + EPSILON);
        let a_z = if omega_a <= 0.0 {
            0.0
        } else {
            100.0 * (-omega_a.ln().abs()).exp()
        };

        // ─── C_z: Communion coefficient ────────────────────────────────────
        // C_z = 100 · exp(−|ln(σ_C)|), where σ_C = (P · η_P) / (|E| + ε)
        let sigma_c = (p * eta_p) / (e.abs() + EPSILON);
        let c_z = if sigma_c <= 0.0 {
            0.0
        } else {
            100.0 * (-sigma_c.ln().abs()).exp()
        };

        // ─── B_H: horizontal balance (A_z ↔ C_z) ───────────────────────────
        let b_h = if a_z > c_z {
            c_z / a_z
        } else {
            a_z / c_z
        };

        // ─── B_V: vertical balance (Eros ↔ Agape) ──────────────────────────
        let eros = af.gamma.er;
        let agape = af.gamma.agp;
        let b_v = if eros > agape {
            agape / eros
        } else {
            eros / agape
        };

        // ─── G_z: integrative efficiency (geometric mean of 4 factors) ─────
        let product = (a_z / 100.0) * (c_z / 100.0) * b_h * b_v;
        let g_z = if product <= 0.0 {
            0.0
        } else {
            100.0 * product.powf(0.25) // 4th root
        };
        let g_z = g_z.clamp(0.0, 100.0);

        // ─── ∇Ψ: structural potential gradient ─────────────────────────────
        let grad_psi = (p - m).abs() / (p + m + EPSILON);

        // ─── θ_alignment: alignment of behavioral output with polar archetype ─
        // If π aligns with A_G polarity → θ=0 (aligned, cos=1)
        // If neutral/depoled → θ=π/2 (neutral, cos=0 — sinkhole)
        // If anti-aligned → θ=π (clamped to cos=0)
        let theta_alignment = compute_theta_alignment(af);
        let cos_theta = theta_alignment.cos();

        // ─── P_z: transcendental tension ────────────────────────────────────
        let p_z = 100.0 * grad_psi * cos_theta;
        let p_z = p_z.max(0.0).min(100.0);

        // ─── Total health ───────────────────────────────────────────────────
        let total = g_z * p_z;

        // ─── State classification ───────────────────────────────────────────
        let state = if g_z < 30.0 {
            HealthState::Collapse
        } else if p_z < 10.0 {
            HealthState::Depolarized
        } else if g_z > 70.0 && p_z > 50.0 {
            HealthState::Optimal
        } else {
            HealthState::SubOptimal
        };

        Self {
            g_z,
            p_z,
            total,
            a_z,
            c_z,
            b_h,
            b_v,
            grad_psi,
            theta_alignment,
            state,
            computed_at: Some(crate::db::crud::now_iso()),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn from_json(s: &str) -> Option<Self> {
        if s.is_empty() || s == "{}" {
            return None;
        }
        serde_json::from_str(s).ok()
    }
}

/// Compute the alignment angle between the behavioral output vector (π)
/// and the core polar archetype (A_G polarity).
fn compute_theta_alignment(af: &AttractorField) -> f64 {
    match (&af.pi, &af.a_g.polarity) {
        (None, _) => std::f64::consts::FRAC_PI_2, // noble → neutral → P_z = 0
        (Some(_), None) => std::f64::consts::FRAC_PI_2, // no polarity → neutral
        (Some(pi), Some(polarity)) => {
            let polar_sign: f64 = match polarity.as_str() {
                "STO" => 1.0,
                "STS" => -1.0,
                _ => return std::f64::consts::FRAC_PI_2, // neutral → π/2
            };
            if pi.signum() == polar_sign.signum() && pi.abs() > 0.05 {
                0.0 // aligned → cos = 1
            } else if pi.abs() < 0.05 {
                std::f64::consts::FRAC_PI_2 // neutral → cos = 0
            } else {
                std::f64::consts::PI // anti-aligned → cos = -1 (clamped to 0 in P_z)
            }
        }
    }
}

// ─── Resonance ───────────────────────────────────────────────────────────────

/// Compute resonance R(H1, H2) ∈ [0, 1] between two attractor fields.
///
/// R = register_complementarity · coupling_tensor_compatibility · great_way_intersection
///
/// - R > 0.7: strong bond
/// - R 0.3–0.7: moderate
/// - R < 0.3: weak
///
/// Transients and noble holons don't resonate (R = 0).
/// The individual resonance components (Phase 21 F5 fix).
#[derive(Debug, Clone, Default)]
pub struct ResonanceComponents {
    pub resonance: f64,
    pub complementarity: f64,
    pub gamma_compat: f64,
    pub great_way_intersect: f64,
}

/// Compute resonance with individual components returned separately.
/// F5 fix: callers should store components in their respective columns.
pub fn resonance_with_components(h1: &AttractorField, h2: &AttractorField) -> ResonanceComponents {
    if !h1.is_stable() || !h2.is_stable() {
        return ResonanceComponents::default();
    }

    let comp = register_complementarity(&h1.pi, &h2.pi);
    let gamma_compat = coupling_tensor_compatibility(&h1.gamma, &h2.gamma);
    let gw = great_way_intersection(&h1.a_g, &h2.a_g);
    let r = (comp * gamma_compat * gw * 10_000.0).round() / 10_000.0;

    ResonanceComponents {
        resonance: r,
        complementarity: comp,
        gamma_compat,
        great_way_intersect: gw,
    }
}

pub fn resonance(h1: &AttractorField, h2: &AttractorField) -> f64 {
    resonance_with_components(h1, h2).resonance
}

/// Factor 1: Register complementarity.
///
/// - Donor (+) ↔ Acceptor (−): complementary → high
/// - Sharer ↔ Sharer (same sign, close values): moderate
/// - Noble (None): doesn't bond → 0
/// - Same-sign far apart: low (0.3)
fn register_complementarity(pi1: &Option<f64>, pi2: &Option<f64>) -> f64 {
    match (pi1, pi2) {
        (None, _) | (_, None) => 0.0, // noble doesn't bond
        (Some(p1), Some(p2)) => {
            // Donor-acceptor: opposite signs
            if (p1 > &0.0 && p2 < &0.0) || (p1 < &0.0 && p2 > &0.0) {
                p1.abs().min(p2.abs()) // complementarity = min of magnitudes
            } else if (p1 - p2).abs() < 0.2 {
                // Sharer-sharer: close values
                1.0 - (p1 - p2).abs()
            } else {
                0.3 // same sign, far apart
            }
        }
    }
}

/// Factor 2: Coupling-tensor compatibility (cosine similarity, clamped ≥ 0).
fn coupling_tensor_compatibility(g1: &crate::metabolism::attractor::CouplingTensor, g2: &crate::metabolism::attractor::CouplingTensor) -> f64 {
    let v1 = g1.as_vec();
    let v2 = g2.as_vec();
    let dot = v1[0] * v2[0] + v1[1] * v2[1] + v1[2] * v2[2] + v1[3] * v2[3];
    let norm1 = (v1[0].powi(2) + v1[1].powi(2) + v1[2].powi(2) + v1[3].powi(2)).sqrt();
    let norm2 = (v2[0].powi(2) + v2[1].powi(2) + v2[2].powi(2) + v2[3].powi(2)).sqrt();
    if norm1 < EPSILON || norm2 < EPSILON {
        return 0.0;
    }
    (dot / (norm1 * norm2)).max(0.0)
}

/// Factor 3: Great-Way intersection.
///
/// - Same polarity → 1.0
/// - One neutral → 0.6
/// - STO-STS pair → 0.2
/// - Other → 0.3
fn great_way_intersection(
    a_g1: &crate::metabolism::attractor::ReservoirAttractor,
    a_g2: &crate::metabolism::attractor::ReservoirAttractor,
) -> f64 {
    match (a_g1.polarity.as_deref(), a_g2.polarity.as_deref()) {
        (Some(p1), Some(p2)) if p1 == p2 => 1.0,
        (p1, p2) if p1 == Some("neutral") || p2 == Some("neutral") => 0.6,
        (Some("STO"), Some("STS")) | (Some("STS"), Some("STO")) => 0.2,
        _ => 0.3,
    }
}

/// Interpret a resonance score.
pub fn interpret_resonance(r: f64) -> &'static str {
    if r > 0.7 {
        "strong"
    } else if r > 0.3 {
        "moderate"
    } else {
        "weak"
    }
}

// ─── DB Persistence ──────────────────────────────────────────────────────────

/// Load health for a holon from the DB.
pub fn load(conn: &rusqlite::Connection, holon_id: &str) -> TdgResult<Option<Health>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT health_json FROM nodes WHERE id = ?1",
            rusqlite::params![holon_id],
            |row| row.get(0),
        )
        .ok()
        .flatten();
    Ok(json.and_then(|s| Health::from_json(&s)))
}

/// Save health for a holon to the DB.
pub fn save(conn: &rusqlite::Connection, holon_id: &str, health: &Health) -> TdgResult<()> {
    conn.execute(
        "UPDATE nodes SET health_json = ?1, health_dirty = 0 WHERE id = ?2",
        rusqlite::params![health.to_json(), holon_id],
    )?;
    Ok(())
}

/// Mark a holon's health as dirty (needs recomputation).
pub fn mark_dirty(conn: &rusqlite::Connection, holon_id: &str) -> TdgResult<()> {
    conn.execute(
        "UPDATE nodes SET health_dirty = 1 WHERE id = ?1",
        rusqlite::params![holon_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metabolism::attractor::{
        ArchetypalLoads, CouplingTensor, ReservoirAttractor, StabilityFilter,
    };
    use crate::metabolism::lesser_cycle::{LesserCycleState, ReservoirState, Shadow};

    fn make_balanced_lesser() -> LesserCycleState {
        let mut s = LesserCycleState::dormant();
        s.matrix = ReservoirState {
            magnitude: 0.5,
            sign: 0,
            eta: 0.5,
            shadow: None,
        };
        s.potentiator = ReservoirState {
            magnitude: 0.5,
            sign: 0,
            eta: 0.5,
            shadow: None,
        };
        s.catalyst_pending = 1.0;
        s.experience_accumulated = 1.0;
        s
    }

    fn make_balanced_af() -> AttractorField {
        AttractorField {
            a_m: ReservoirAttractor::new(0.5, 0),
            a_p: ReservoirAttractor::new(0.5, 0),
            a_g: ReservoirAttractor::with_polarity(0.5, "STO"),
            gamma: CouplingTensor {
                ag: 0.5,
                cm: 0.5,
                er: 0.5,
                agp: 0.5,
            },
            pi: Some(0.3),
            type_class: "sharer-sto".to_string(),
            choice_flag: None,
            loads: ArchetypalLoads::default(),
            stability: StabilityFilter {
                self_consistent: true,
                bondable: true,
                persistent: true,
            },
            computed_at: None,
        }
    }

    #[test]
    fn health_compute_basic() {
        let lesser = make_balanced_lesser();
        let af = make_balanced_af();
        let health = Health::compute(&lesser, &af);

        assert!(health.g_z >= 0.0 && health.g_z <= 100.0);
        assert!(health.p_z >= 0.0 && health.p_z <= 100.0);
        assert!(health.total >= 0.0);
    }

    #[test]
    fn g_z_rewards_balance() {
        let lesser = make_balanced_lesser();
        let af = make_balanced_af();
        let health = Health::compute(&lesser, &af);

        // With balanced reservoirs and drives, G_z should be moderate-high
        assert!(health.g_z > 30.0, "G_z should be > 30 for balanced state, got {}", health.g_z);
    }

    #[test]
    fn g_z_collapses_on_extreme_imbalance() {
        let mut lesser = make_balanced_lesser();
        // Extreme imbalance: huge catalyst, tiny matrix
        lesser.catalyst_pending = 100.0;
        lesser.matrix.magnitude = 0.01;
        lesser.matrix.eta = 0.01;

        let af = make_balanced_af();
        let health = Health::compute(&lesser, &af);

        // A_z should be near 0 (omega_a = 0.01*0.01 / 100 ≈ 0 → ln → -inf → exp → 0)
        assert!(health.a_z < 10.0, "A_z should collapse, got {}", health.a_z);
        assert!(health.g_z < 30.0, "G_z should collapse, got {}", health.g_z);
    }

    #[test]
    fn p_z_rewards_commitment() {
        let mut lesser = make_balanced_lesser();
        // High tension: large P, small M
        lesser.potentiator.magnitude = 0.9;
        lesser.matrix.magnitude = 0.1;
        lesser.transformation_pressure = 2.0;

        let mut af = make_balanced_af();
        af.pi = Some(0.8); // strong alignment
        af.a_g = ReservoirAttractor::with_polarity(0.8, "STO");

        let health = Health::compute(&lesser, &af);

        // grad_psi = |0.9 - 0.1| / (0.9 + 0.1) = 0.8
        assert!(health.grad_psi > 0.7, "grad_psi should be high, got {}", health.grad_psi);
        assert!(health.p_z > 10.0, "P_z should be > 10 with high tension + alignment, got {}", health.p_z);
    }

    #[test]
    fn p_z_sinkhole_when_neutral() {
        let lesser = make_balanced_lesser();
        let mut af = make_balanced_af();
        af.pi = Some(0.0); // neutral
        af.a_g = ReservoirAttractor::with_polarity(0.5, "neutral");

        let health = Health::compute(&lesser, &af);

        // Neutral → θ = π/2 → cos = 0 → P_z = 0 (sinkhole)
        assert!(health.p_z < 1.0, "P_z should be ~0 (sinkhole), got {}", health.p_z);
        assert_eq!(health.state, HealthState::Depolarized);
    }

    #[test]
    fn health_state_optimal() {
        let mut lesser = make_balanced_lesser();
        lesser.potentiator.magnitude = 0.8;
        lesser.matrix.magnitude = 0.3;
        lesser.catalyst_pending = 0.5;
        lesser.experience_accumulated = 0.5;

        let mut af = make_balanced_af();
        af.pi = Some(0.7);
        af.a_g = ReservoirAttractor::with_polarity(0.8, "STO");
        af.gamma = CouplingTensor {
            ag: 0.6,
            cm: 0.4,
            er: 0.6,
            agp: 0.4,
        };

        let health = Health::compute(&lesser, &af);

        // Should not be collapse or sinkhole
        assert_ne!(health.state, HealthState::Collapse);
        assert_ne!(health.state, HealthState::Depolarized);
    }

    #[test]
    fn resonance_donor_acceptor_strong() {
        let af1 = AttractorField {
            a_m: ReservoirAttractor::new(0.7, 1),
            a_p: ReservoirAttractor::new(0.5, 0),
            a_g: ReservoirAttractor::with_polarity(0.6, "STO"),
            gamma: CouplingTensor {
                ag: 0.7,
                cm: 0.3,
                er: 0.6,
                agp: 0.4,
            },
            pi: Some(0.8), // strong donor
            type_class: "strong-donor-sto".to_string(),
            choice_flag: None,
            loads: ArchetypalLoads::default(),
            stability: StabilityFilter {
                self_consistent: true,
                bondable: true,
                persistent: true,
            },
            computed_at: None,
        };

        let af2 = AttractorField {
            a_m: ReservoirAttractor::new(0.6, -1),
            a_p: ReservoirAttractor::new(0.5, 0),
            a_g: ReservoirAttractor::with_polarity(0.5, "STS"),
            gamma: CouplingTensor {
                ag: 0.3,
                cm: 0.7,
                er: 0.4,
                agp: 0.6,
            },
            pi: Some(-0.7), // strong acceptor
            type_class: "strong-acceptor-sts".to_string(),
            choice_flag: None,
            loads: ArchetypalLoads::default(),
            stability: StabilityFilter {
                self_consistent: true,
                bondable: true,
                persistent: true,
            },
            computed_at: None,
        };

        let r = resonance(&af1, &af2);
        // Donor-acceptor complementarity = min(0.8, 0.7) = 0.7
        // But GW intersection: STO-STS = 0.2
        // So R = 0.7 * gamma_compat * 0.2
        assert!(r > 0.0, "Resonance should be > 0 for donor-acceptor, got {}", r);
        assert!(r < 0.7, "STO-STS pair should have reduced resonance, got {}", r);
    }

    #[test]
    fn resonance_same_polarity_moderate() {
        let af1 = AttractorField {
            a_m: ReservoirAttractor::new(0.7, 1),
            a_p: ReservoirAttractor::new(0.5, 0),
            a_g: ReservoirAttractor::with_polarity(0.6, "STO"),
            gamma: CouplingTensor {
                ag: 0.6,
                cm: 0.4,
                er: 0.5,
                agp: 0.5,
            },
            pi: Some(0.1), // sharer
            type_class: "sharer-sto".to_string(),
            choice_flag: None,
            loads: ArchetypalLoads::default(),
            stability: StabilityFilter {
                self_consistent: true,
                bondable: true,
                persistent: true,
            },
            computed_at: None,
        };

        let mut af2 = af1.clone();
        af2.pi = Some(0.15); // close sharer

        let r = resonance(&af1, &af2);
        // Sharer-sharer, same polarity (STO-STO = 1.0), close values
        assert!(r > 0.3, "Same-polarity sharers should have moderate+ resonance, got {}", r);
    }

    #[test]
    fn resonance_transient_is_zero() {
        let mut af1 = make_balanced_af();
        af1.stability.bondable = false; // transient

        let af2 = make_balanced_af();

        let r = resonance(&af1, &af2);
        assert_eq!(r, 0.0, "Transients should not resonate");
    }

    #[test]
    fn resonance_noble_is_zero() {
        let mut af1 = make_balanced_af();
        af1.pi = None; // noble

        let af2 = make_balanced_af();

        let r = resonance(&af1, &af2);
        assert_eq!(r, 0.0, "Noble should not resonate");
    }

    #[test]
    fn interpret_resonance_thresholds() {
        assert_eq!(interpret_resonance(0.8), "strong");
        assert_eq!(interpret_resonance(0.5), "moderate");
        assert_eq!(interpret_resonance(0.1), "weak");
    }

    #[test]
    fn json_roundtrip() {
        let health = Health {
            g_z: 65.0,
            p_z: 40.0,
            total: 2600.0,
            a_z: 70.0,
            c_z: 60.0,
            b_h: 0.85,
            b_v: 0.90,
            grad_psi: 0.4,
            theta_alignment: 0.0,
            state: HealthState::SubOptimal,
            computed_at: Some("2026-07-03T12:00:00Z".to_string()),
        };

        let json = health.to_json();
        let restored = Health::from_json(&json).unwrap();

        assert_eq!(restored.g_z, health.g_z);
        assert_eq!(restored.p_z, health.p_z);
        assert_eq!(restored.state, health.state);
    }
}
