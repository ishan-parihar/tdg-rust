//! Attractor Field — A(H) = ⟨A_M, A_P, A_G, Γ⟩
//!
//! Source: HoloOS `_THEORY/02_Ontology/08.1_Attractor_Field_Model.md`
//! (canonical-hypothesis, Doc 08.1)
//!
//! The attractor field is the unified operational object tying together the
//! metabolic engine (lesser cycle), archetype anatomy, and typology derivator.
//! It is computed from the holon's lesser cycle state + drives + edges.
//!
//! ## Components
//!
//! | Component | Source | What it encodes |
//! |-----------|--------|-----------------|
//! | A_M | Matrix reservoir (lesser cycle) | Current homeostatic basin |
//! | A_P | Potentiator reservoir (lesser cycle) | Latent basin (reachable states) |
//! | A_G | Great-Way reservoir (edges + greater cycle) | Environmental basin |
//! | Γ | Coupling tensor (drives) | Transmission profile (on 2-torus) |
//!
//! ## Read-outs
//!
//! - **π** (polarity disposition): sgn(α_M·A_M + α_P·A_P + α_G·A_G)
//!   - +1 → donor (STO-leaning), -1 → acceptor (STS-leaning), 0 → balanced/sharer
//!   - None → noble (closed valence; Choice disambiguates)
//! - **type_class**: e.g. "strong-donor-sto", "sharer", "noble-graduated"
//! - **choice_flag**: None | Graduated | Sinkhole | Reopened (disambiguates noble)
//! - **archetypal_loads**: 8-role vector ℓ = (M, P, C, E, S, T, G, Ch)
//! - **stability**: self_consistent AND bondable AND persistent

use serde::{Deserialize, Serialize};

use crate::error::TdgResult;
use crate::metabolism::lesser_cycle::{LesserCycleState, Shadow};

// ─── Types ───────────────────────────────────────────────────────────────────

/// A reservoir attractor (A_M, A_P, or A_G).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReservoirAttractor {
    /// Basin depth ∈ [0, 1] — how consolidated/latent/environmental.
    pub magnitude: f64,
    /// -1 (acceptor/deficit), 0 (balanced), +1 (donor/surplus).
    pub sign: i8,
    /// Only for A_G: "STO", "STS", or "neutral".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polarity: Option<String>,
}

impl ReservoirAttractor {
    pub fn new(magnitude: f64, sign: i8) -> Self {
        Self {
            magnitude: magnitude.clamp(0.0, 1.0),
            sign,
            polarity: None,
        }
    }

    pub fn with_polarity(magnitude: f64, polarity: &str) -> Self {
        Self {
            magnitude: magnitude.clamp(0.0, 1.0),
            sign: match polarity {
                "STO" => 1,
                "STS" => -1,
                _ => 0,
            },
            polarity: Some(polarity.to_string()),
        }
    }
}

/// The coupling tensor Γ — lives on a 2-torus (not a 4-cube).
///
/// Horizontal drives (Ag, Cm) are anti-correlated; vertical drives (Er, Agp)
/// are anti-correlated. The `enforce_torus_constraints` method enforces this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouplingTensor {
    pub ag: f64,  // Agency [0, 1]
    pub cm: f64,  // Communion [0, 1]
    pub er: f64,  // Eros [0, 1]
    pub agp: f64, // Agape [0, 1]
}

impl Default for CouplingTensor {
    fn default() -> Self {
        Self {
            ag: 0.5,
            cm: 0.5,
            er: 0.5,
            agp: 0.5,
        }
    }
}

impl CouplingTensor {
    /// Enforce 2-torus constraints: horizontal and vertical pairs anti-correlated.
    /// If Ag + Cm > 1.0, scale down proportionally. Same for Er + Agp.
    pub fn enforce_torus_constraints(&mut self) {
        let h_sum = self.ag + self.cm;
        if h_sum > 1.0 {
            let scale = 1.0 / h_sum;
            self.ag *= scale;
            self.cm *= scale;
        }
        let v_sum = self.er + self.agp;
        if v_sum > 1.0 {
            let scale = 1.0 / v_sum;
            self.er *= scale;
            self.agp *= scale;
        }
        // Clamp all to [0, 1]
        self.ag = self.ag.clamp(0.0, 1.0);
        self.cm = self.cm.clamp(0.0, 1.0);
        self.er = self.er.clamp(0.0, 1.0);
        self.agp = self.agp.clamp(0.0, 1.0);
    }

    /// Horizontal balance: 0.5 when Ag == Cm.
    pub fn horizontal_balance(&self) -> f64 {
        self.ag - self.cm + 0.5
    }

    /// Balance multiplier: 1 - mean(|dev from 0.5|).
    pub fn balance_multiplier(&self) -> f64 {
        let devs = [
            (self.ag - 0.5).abs(),
            (self.cm - 0.5).abs(),
            (self.er - 0.5).abs(),
            (self.agp - 0.5).abs(),
        ];
        1.0 - (devs.iter().sum::<f64>() / 4.0)
    }

    /// Create a 4-vector for cosine similarity.
    pub fn as_vec(&self) -> [f64; 4] {
        [self.ag, self.cm, self.er, self.agp]
    }
}

/// The 8-role archetypal load vector ℓ = (M, P, C, E, S, T, G, Ch).
///
/// Loop A (Lesser, continuous): M → C → P → E → M
/// Loop B (Greater, discontinuous): S → T → G → Ch → S
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArchetypalLoads {
    pub m: f64, // Matrix
    pub p: f64, // Potentiator
    pub c: f64, // Catalyst
    pub e: f64, // Experience
    pub s: f64, // Significator
    pub t: f64, // Transformation
    pub g: f64, // Great Way
    pub ch: f64, // Choice
}

impl ArchetypalLoads {
    /// Check if all loads are in [0, 1].
    pub fn is_valid(&self) -> bool {
        [self.m, self.p, self.c, self.e, self.s, self.t, self.g, self.ch]
            .iter()
            .all(|v| (0.0..=1.0).contains(v))
    }
}

/// The stability filter — 3 conditions for a stable type.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StabilityFilter {
    /// ℓ_T < θ_T (0.7) AND π stable.
    pub self_consistent: bool,
    /// A_G.magnitude > 0.1 (can bond with environment).
    pub bondable: bool,
    /// |C − E| < 0.5 (intake/output not wildly imbalanced).
    pub persistent: bool,
}

impl StabilityFilter {
    pub fn is_stable_type(&self) -> bool {
        self.self_consistent && self.bondable && self.persistent
    }
}

/// The Choice flag — disambiguates noble (closed valence).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ChoiceFlag {
    Graduated,
    Sinkhole,
    Reopened,
}

impl ChoiceFlag {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Graduated => "graduated",
            Self::Sinkhole => "sinkhole",
            Self::Reopened => "reopened",
        }
    }
}

/// The complete attractor field A(H) = ⟨A_M, A_P, A_G, Γ⟩.
///
/// The Significator is implicit — it's the time-integral of the field,
/// not a fifth component.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AttractorField {
    pub a_m: ReservoirAttractor,
    pub a_p: ReservoirAttractor,
    pub a_g: ReservoirAttractor,
    pub gamma: CouplingTensor,
    /// Polarity disposition ∈ [-1, +1]. None = noble (closed valence).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pi: Option<f64>,
    /// Type class string, e.g. "strong-donor-sto", "sharer", "noble-graduated".
    #[serde(default)]
    pub type_class: String,
    /// Choice flag — only set when π is None (noble).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice_flag: Option<ChoiceFlag>,
    pub loads: ArchetypalLoads,
    pub stability: StabilityFilter,
    /// Timestamp of computation (RFC3339).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computed_at: Option<String>,
}

impl AttractorField {
    /// Create a default (empty) attractor field for a dormant holon.
    pub fn dormant() -> Self {
        Self {
            a_m: ReservoirAttractor::new(0.5, 0),
            a_p: ReservoirAttractor::new(0.3, 0),
            a_g: ReservoirAttractor::new(0.0, 0),
            gamma: CouplingTensor::default(),
            pi: Some(0.0),
            type_class: "dormant".to_string(),
            choice_flag: None,
            loads: ArchetypalLoads::default(),
            stability: StabilityFilter {
                self_consistent: true,
                bondable: false,
                persistent: true,
            },
            computed_at: None,
        }
    }

    /// Whether this attractor field represents a stable, bondable type.
    pub fn is_stable(&self) -> bool {
        self.stability.is_stable_type()
    }

    /// Whether this holon is noble (closed valence — π is None).
    pub fn is_noble(&self) -> bool {
        self.pi.is_none()
    }

    // ─── Serialization ──────────────────────────────────────────────────────

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

// ─── Computation ─────────────────────────────────────────────────────────────

/// Compute the attractor field from a holon's lesser cycle state, drives,
/// edge count, and (optionally) greater cycle pressure.
///
/// This is called by the metabolism worker when the lesser cycle reaches
/// the Integrating phase (dirty flag set).
///
/// # Arguments
/// * `lesser` - The holon's lesser cycle state (M, P, C, E + shadows)
/// * `drives_json` - The holon's drives_json value (eros, agape, agency, communion)
/// * `edge_count` - Number of active edges (for A_G magnitude)
/// * `transformation_pressure` - Accumulated pressure (for A_G polarity)
pub fn compute(
    lesser: &LesserCycleState,
    drives_json: &serde_json::Value,
    edge_count: i64,
    transformation_pressure: f64,
) -> AttractorField {
    // ─── A_M: Matrix attractor ──────────────────────────────────────────────
    // Magnitude from Matrix reservoir magnitude.
    // Sign from Matrix shadow: addiction → +1 (donor), allergy → -1 (acceptor).
    let a_m_sign = match &lesser.matrix.shadow {
        Some(Shadow::MatrixHyperIngestion) => 1,
        Some(Shadow::MatrixHypoIngestion) => -1,
        _ => 0,
    };
    let a_m = ReservoirAttractor::new(lesser.matrix.magnitude, a_m_sign);

    // ─── A_P: Potentiator attractor ─────────────────────────────────────────
    // Magnitude from Potentiator reservoir magnitude.
    // Sign from Potentiator shadow: golden-addiction → +1, golden-allergy → -1.
    let a_p_sign = match &lesser.potentiator.shadow {
        Some(Shadow::PotentiatorHyperIngestion) => 1,
        Some(Shadow::PotentiatorHypoIngestion) => -1,
        _ => 0,
    };
    let a_p = ReservoirAttractor::new(lesser.potentiator.magnitude, a_p_sign);

    // ─── A_G: Great-Way attractor ───────────────────────────────────────────
    // Magnitude from edge count (coupling breadth): more edges = more environmental coupling.
    // Capped at 1.0 (10+ edges = full coupling).
    let a_g_magnitude = (edge_count as f64 / 10.0).min(1.0);
    // Polarity from transformation_pressure sign: positive → STO, negative → STS.
    let a_g_polarity = if transformation_pressure > 0.1 {
        "STO"
    } else if transformation_pressure < -0.1 {
        "STS"
    } else {
        "neutral"
    };
    let a_g = ReservoirAttractor::with_polarity(a_g_magnitude, a_g_polarity);

    // ─── Γ: Coupling tensor from drives ─────────────────────────────────────
    let mut gamma = extract_coupling_tensor(drives_json);
    gamma.enforce_torus_constraints();

    // ─── Archetypal loads ───────────────────────────────────────────────────
    let loads = ArchetypalLoads {
        m: lesser.matrix.magnitude,
        p: lesser.potentiator.magnitude,
        c: (lesser.catalyst_pending / 5.0).min(1.0), // normalized catalyst load
        e: (lesser.experience_accumulated / 5.0).min(1.0), // normalized experience
        s: (lesser.matrix.magnitude + lesser.experience_accumulated * 0.1).min(1.0),
        t: (lesser.transformation_pressure / 5.0).min(1.0),
        g: a_g_magnitude,
        ch: gamma.horizontal_balance(), // Choice ≈ horizontal balance
    };

    // ─── π: polarity disposition ────────────────────────────────────────────
    // π = (A_M.sign + A_P.sign + A_G_polarity_sign) / 3.0
    let a_g_sign = match a_g.polarity.as_deref() {
        Some("STO") => 1.0,
        Some("STS") => -1.0,
        _ => 0.0,
    };
    let pi_raw = (a_m.sign as f64 + a_p.sign as f64 + a_g_sign) / 3.0;

    // Noble check: all reservoirs balanced AND |π| < 0.05
    let all_balanced = a_m.sign == 0 && a_p.sign == 0 && a_g_sign == 0.0;
    let pi = if all_balanced && pi_raw.abs() < 0.05 {
        None // noble
    } else {
        Some(pi_raw.clamp(-1.0, 1.0))
    };

    // ─── Stability filter ───────────────────────────────────────────────────
    let stability = StabilityFilter {
        self_consistent: loads.t < 0.7, // ℓ_T < θ_T
        bondable: a_g.magnitude > 0.1,
        persistent: (loads.c - loads.e).abs() < 0.5,
    };

    // ─── Type classification ────────────────────────────────────────────────
    let type_class = classify_type(&pi, &a_g, &stability);

    // ─── Choice flag (only for noble) ───────────────────────────────────────
    let choice_flag = if pi.is_none() {
        // Disambiguate: if bondable and self_consistent → graduated, else sinkhole.
        // This is a heuristic; real disambiguation requires the greater cycle (Phase 4).
        if stability.bondable && stability.self_consistent {
            Some(ChoiceFlag::Graduated)
        } else {
            Some(ChoiceFlag::Sinkhole)
        }
    } else {
        None
    };

    AttractorField {
        a_m,
        a_p,
        a_g,
        gamma,
        pi,
        type_class,
        choice_flag,
        loads,
        stability,
        computed_at: Some(crate::db::crud::now_iso()),
    }
}

/// Extract the coupling tensor from a drives_json Value.
///
/// Expects the flow.rs DualPoleDrive format:
/// `{ "eros": {"positive_pole": 6.0, "negative_pole": 1.0, ...}, ... }`
///
/// Each drive is normalized to [0, 1] from the positive_pole (clamped to [0, 10]).
fn extract_coupling_tensor(drives: &serde_json::Value) -> CouplingTensor {
    let get_pos = |name: &str| -> f64 {
        drives
            .get(name)
            .and_then(|d| d.get("positive_pole"))
            .and_then(|v| v.as_f64())
            .unwrap_or(5.0) // default to 0.5 (neutral) = 5.0/10.0
            .clamp(0.0, 10.0)
            / 10.0
    };

    CouplingTensor {
        ag: get_pos("agency"),
        cm: get_pos("communion"),
        er: get_pos("eros"),
        agp: get_pos("agape"),
    }
}

/// Classify the type from π, A_G polarity, and stability.
fn classify_type(pi: &Option<f64>, a_g: &ReservoirAttractor, stability: &StabilityFilter) -> String {
    // Not stable → transient
    if !stability.is_stable_type() {
        return "transient".to_string();
    }

    // Noble (π is None)
    if pi.is_none() {
        return match a_g.polarity.as_deref() {
            Some("STO") => "noble-sto".to_string(),
            Some("STS") => "noble-sts".to_string(),
            _ => "noble-ambiguous".to_string(),
        };
    }

    let pi_val = pi.unwrap();

    // Prefix from π
    let prefix = if pi_val > 0.5 {
        "strong-donor"
    } else if pi_val > 0.1 {
        "weak-donor"
    } else if pi_val < -0.5 {
        "strong-acceptor"
    } else if pi_val < -0.1 {
        "weak-acceptor"
    } else {
        "sharer"
    };

    // Suffix from A_G polarity
    let suffix = match a_g.polarity.as_deref() {
        Some("STO") => "-sto",
        Some("STS") => "-sts",
        _ => "",
    };

    format!("{}{}", prefix, suffix)
}

// ─── DB Persistence ──────────────────────────────────────────────────────────

/// Load attractor field for a holon from the DB.
/// Returns None if no field is stored.
pub fn load(conn: &rusqlite::Connection, holon_id: &str) -> TdgResult<Option<AttractorField>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT attractor_field_json FROM nodes WHERE id = ?1",
            rusqlite::params![holon_id],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    Ok(json.and_then(|s| AttractorField::from_json(&s)))
}

/// Save attractor field for a holon to the DB.
pub fn save(
    conn: &rusqlite::Connection,
    holon_id: &str,
    field: &AttractorField,
) -> TdgResult<()> {
    conn.execute(
        "UPDATE nodes SET attractor_field_json = ?1, attractor_dirty = 0 WHERE id = ?2",
        rusqlite::params![field.to_json(), holon_id],
    )?;
    Ok(())
}

/// Check if a holon's attractor field needs recomputation (dirty flag).
pub fn is_dirty(conn: &rusqlite::Connection, holon_id: &str) -> TdgResult<bool> {
    let dirty: Option<i64> = conn
        .query_row(
            "SELECT attractor_dirty FROM nodes WHERE id = ?1",
            rusqlite::params![holon_id],
            |row| row.get(0),
        )
        .ok();
    Ok(dirty.unwrap_or(0) != 0)
}

/// Mark a holon's attractor field as dirty (needs recomputation).
pub fn mark_dirty(conn: &rusqlite::Connection, holon_id: &str) -> TdgResult<()> {
    conn.execute(
        "UPDATE nodes SET attractor_dirty = 1 WHERE id = ?1",
        rusqlite::params![holon_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metabolism::lesser_cycle::{LesserCycleState, ReservoirState, Shadow};

    #[test]
    fn dormant_attractor_field() {
        let af = AttractorField::dormant();
        assert_eq!(af.type_class, "dormant");
        assert!(!af.is_stable()); // not bondable
        assert!(!af.is_noble());
    }

    #[test]
    fn compute_basic_attractor_field() {
        let mut lesser = LesserCycleState::dormant();
        lesser.matrix.magnitude = 0.7;
        lesser.matrix.shadow = Some(Shadow::MatrixHyperIngestion); // → donor
        lesser.potentiator.magnitude = 0.4;
        lesser.catalyst_pending = 1.0;
        lesser.experience_accumulated = 2.0;
        lesser.transformation_pressure = 1.5;

        let drives = serde_json::json!({
            "eros": {"positive_pole": 7.0, "negative_pole": 2.0},
            "agape": {"positive_pole": 5.0, "negative_pole": 3.0},
            "agency": {"positive_pole": 6.0, "negative_pole": 1.0},
            "communion": {"positive_pole": 4.0, "negative_pole": 2.0},
        });

        let af = compute(&lesser, &drives, 5, 1.5);

        assert_eq!(af.a_m.sign, 1); // DarkAddiction → donor
        assert!(af.a_m.magnitude > 0.6);
        assert_eq!(af.a_g.polarity.as_deref(), Some("STO")); // positive pressure
        assert!(af.a_g.magnitude > 0.4); // 5 edges / 10 = 0.5
        assert!(af.pi.is_some());
        assert!(af.pi.unwrap() > 0.0); // donor-leaning
        assert!(!af.type_class.is_empty());
    }

    #[test]
    fn compute_noble_attractor_field() {
        let mut lesser = LesserCycleState::dormant();
        lesser.matrix.magnitude = 0.5;
        lesser.matrix.shadow = None; // balanced
        lesser.potentiator.magnitude = 0.5;
        lesser.potentiator.shadow = None; // balanced
        lesser.catalyst_pending = 0.5;
        lesser.experience_accumulated = 0.5;
        lesser.transformation_pressure = 0.0; // neutral

        let drives = serde_json::json!({
            "eros": {"positive_pole": 5.0, "negative_pole": 5.0},
            "agape": {"positive_pole": 5.0, "negative_pole": 5.0},
            "agency": {"positive_pole": 5.0, "negative_pole": 5.0},
            "communion": {"positive_pole": 5.0, "negative_pole": 5.0},
        });

        let af = compute(&lesser, &drives, 5, 0.0);

        // All balanced → noble
        assert!(af.is_noble());
        assert!(af.type_class.starts_with("noble"));
        assert!(af.choice_flag.is_some());
    }

    #[test]
    fn coupling_tensor_torus_constraints() {
        let mut gamma = CouplingTensor {
            ag: 0.8,
            cm: 0.7, // sum = 1.5 > 1.0
            er: 0.6,
            agp: 0.5,
        };
        gamma.enforce_torus_constraints();
        // After scaling: ag = 0.8/1.5 ≈ 0.533, cm = 0.7/1.5 ≈ 0.467
        assert!(gamma.ag + gamma.cm <= 1.01); // allow float tolerance
        assert!(gamma.er + gamma.agp <= 1.01);
    }

    #[test]
    fn type_classification_donor_sto() {
        let pi = Some(0.7); // strong donor
        let a_g = ReservoirAttractor::with_polarity(0.5, "STO");
        let stability = StabilityFilter {
            self_consistent: true,
            bondable: true,
            persistent: true,
        };
        let tc = classify_type(&pi, &a_g, &stability);
        assert_eq!(tc, "strong-donor-sto");
    }

    #[test]
    fn type_classification_acceptor_sts() {
        let pi = Some(-0.6); // strong acceptor
        let a_g = ReservoirAttractor::with_polarity(0.5, "STS");
        let stability = StabilityFilter {
            self_consistent: true,
            bondable: true,
            persistent: true,
        };
        let tc = classify_type(&pi, &a_g, &stability);
        assert_eq!(tc, "strong-acceptor-sts");
    }

    #[test]
    fn type_classification_sharer() {
        let pi = Some(0.0); // balanced
        let a_g = ReservoirAttractor::with_polarity(0.5, "neutral");
        let stability = StabilityFilter {
            self_consistent: true,
            bondable: true,
            persistent: true,
        };
        let tc = classify_type(&pi, &a_g, &stability);
        assert_eq!(tc, "sharer");
    }

    #[test]
    fn type_classification_transient_when_unstable() {
        let pi = Some(0.7);
        let a_g = ReservoirAttractor::with_polarity(0.5, "STO");
        let stability = StabilityFilter {
            self_consistent: true,
            bondable: false, // not bondable → transient
            persistent: true,
        };
        let tc = classify_type(&pi, &a_g, &stability);
        assert_eq!(tc, "transient");
    }

    #[test]
    fn json_roundtrip() {
        let af = AttractorField {
            a_m: ReservoirAttractor::new(0.7, 1),
            a_p: ReservoirAttractor::new(0.5, 0),
            a_g: ReservoirAttractor::with_polarity(0.6, "STO"),
            gamma: CouplingTensor {
                ag: 0.6,
                cm: 0.4,
                er: 0.7,
                agp: 0.3,
            },
            pi: Some(0.61),
            type_class: "strong-donor-sto".to_string(),
            choice_flag: None,
            loads: ArchetypalLoads {
                m: 0.7,
                p: 0.5,
                c: 0.3,
                e: 0.4,
                s: 0.8,
                t: 0.2,
                g: 0.6,
                ch: 0.6,
            },
            stability: StabilityFilter {
                self_consistent: true,
                bondable: true,
                persistent: true,
            },
            computed_at: Some("2026-07-03T12:00:00Z".to_string()),
        };

        let json = af.to_json();
        let restored = AttractorField::from_json(&json).unwrap();

        assert_eq!(restored.a_m.magnitude, af.a_m.magnitude);
        assert_eq!(restored.a_m.sign, af.a_m.sign);
        assert_eq!(restored.pi, af.pi);
        assert_eq!(restored.type_class, af.type_class);
        assert_eq!(restored.gamma.ag, af.gamma.ag);
        assert_eq!(restored.loads.m, af.loads.m);
        assert!(restored.is_stable());
    }

    #[test]
    fn extract_coupling_tensor_from_drives() {
        let drives = serde_json::json!({
            "eros": {"positive_pole": 8.0, "negative_pole": 1.0},
            "agape": {"positive_pole": 4.0, "negative_pole": 2.0},
            "agency": {"positive_pole": 6.0, "negative_pole": 3.0},
            "communion": {"positive_pole": 5.0, "negative_pole": 1.0},
        });

        let gamma = extract_coupling_tensor(&drives);
        assert_eq!(gamma.er, 0.8); // 8.0/10.0
        assert_eq!(gamma.agp, 0.4); // 4.0/10.0
        assert_eq!(gamma.ag, 0.6); // 6.0/10.0
        assert_eq!(gamma.cm, 0.5); // 5.0/10.0
    }

    #[test]
    fn extract_coupling_tensor_defaults() {
        let drives = serde_json::json!({});
        let gamma = extract_coupling_tensor(&drives);
        // All default to 5.0/10.0 = 0.5
        assert_eq!(gamma.er, 0.5);
        assert_eq!(gamma.agp, 0.5);
        assert_eq!(gamma.ag, 0.5);
        assert_eq!(gamma.cm, 0.5);
    }
}
