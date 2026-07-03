//! 22 Named Archetypes — the typological library.
//!
//! Source: HoloOS `_THEORY/02_Ontology/03.2_22_Named_Archetypes_Index.md`
//! (ai-draft)
//!
//! The 22 named archetypes are domain elaborations of the 8 functional roles
//! across three complexes + the Choice meta-pivot.
//!
//! **7 roles × 3 complexes = 21 + Choice = 22**
//!
//! The 8 functional roles (M·P·C·E·S·T·G·Ch) are the **operators**; the 22
//! named archetypes are the **operands** — they give concrete expression to
//! the roles within each complex (Mind, Body, Spirit).
//!
//! ## Complexes
//!
//! | Complex | Domain | Description |
//! |---------|--------|-------------|
//! | Mind | Cognitive | Consciousness, thought, mental patterns |
//! | Body | Physical | Action, behavior, physical manifestation |
//! | Spirit | Transcendent | Integration, meaning, evolutionary purpose |
//!
//! ## The 8 Functional Roles
//!
//! | Role | Symbol | Function |
//! |------|--------|----------|
//! | Matrix | M | Current-state organizer (conserved structure) |
//! | Potentiator | P | Latent-state generator (possibility space) |
//! | Catalyst | C | Boundary-crossing pressure (input) |
//! | Experience | E | Processed input (output/learning) |
//! | Significator | S | Persistent identity-pattern |
//! | Transformation | T | Threshold restructuring event |
//! | Great Way | G | Operating environment |
//! | Choice | Ch | Directional commitment (meta-pivot) |

use serde::{Deserialize, Serialize};

// ─── Types ───────────────────────────────────────────────────────────────────

/// The 3 complexes (Mind, Body, Spirit).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Complex {
    Mind,
    Body,
    Spirit,
    /// The Choice meta-pivot — not part of any complex.
    Pivot,
}

impl Complex {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mind => "mind",
            Self::Body => "body",
            Self::Spirit => "spirit",
            Self::Pivot => "pivot",
        }
    }
}

/// The 8 functional roles (M·P·C·E·S·T·G·Ch).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Matrix,
    Potentiator,
    Catalyst,
    Experience,
    Significator,
    Transformation,
    GreatWay,
    Choice,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Matrix => "M",
            Self::Potentiator => "P",
            Self::Catalyst => "C",
            Self::Experience => "E",
            Self::Significator => "S",
            Self::Transformation => "T",
            Self::GreatWay => "G",
            Self::Choice => "Ch",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Matrix => "Matrix",
            Self::Potentiator => "Potentiator",
            Self::Catalyst => "Catalyst",
            Self::Experience => "Experience",
            Self::Significator => "Significator",
            Self::Transformation => "Transformation",
            Self::GreatWay => "Great Way",
            Self::Choice => "Choice",
        }
    }
}

/// One of the 22 named archetypes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Archetype {
    /// Number 1-22.
    pub number: u8,
    /// Archetype name (e.g. "Matrix of the Mind").
    pub name: &'static str,
    /// Which complex this archetype belongs to.
    pub complex: Complex,
    /// Which functional role this archetype expresses.
    pub role: Role,
    /// Short description.
    pub description: &'static str,
}

/// Get all 22 named archetypes.
pub fn all_archetypes() -> &'static [Archetype] {
    &ARCHETYPES
}

/// Get an archetype by number (1-22).
pub fn archetype_by_number(number: u8) -> Option<&'static Archetype> {
    ARCHETYPES.iter().find(|a| a.number == number)
}

/// Get archetypes by complex.
pub fn archetypes_by_complex(complex: &Complex) -> Vec<&'static Archetype> {
    ARCHETYPES.iter().filter(|a| &a.complex == complex).collect()
}

/// Get archetypes by role.
pub fn archetypes_by_role(role: &Role) -> Vec<&'static Archetype> {
    ARCHETYPES.iter().filter(|a| &a.role == role).collect()
}

/// Get an archetype by complex + role.
pub fn archetype_by_complex_role(complex: &Complex, role: &Role) -> Option<&'static Archetype> {
    ARCHETYPES
        .iter()
        .find(|a| &a.complex == complex && &a.role == role)
}

// ─── The 22 Named Archetypes ─────────────────────────────────────────────────

pub static ARCHETYPES: &[Archetype] = &[
    // ─── Mind Complex (1-7) ──────────────────────────────────────────────────
    Archetype {
        number: 1,
        name: "Matrix of the Mind",
        complex: Complex::Mind,
        role: Role::Matrix,
        description: "Current-state mental organizer — conserved cognitive structure, mental models, beliefs",
    },
    Archetype {
        number: 2,
        name: "Potentiator of the Mind",
        complex: Complex::Mind,
        role: Role::Potentiator,
        description: "Latent mental possibilities — imagination, envisioned futures, cognitive potential",
    },
    Archetype {
        number: 3,
        name: "Catalyst of the Mind",
        complex: Complex::Mind,
        role: Role::Catalyst,
        description: "Boundary-crossing mental pressure — new ideas, information, intellectual challenges",
    },
    Archetype {
        number: 4,
        name: "Experience of the Mind",
        complex: Complex::Mind,
        role: Role::Experience,
        description: "Processed mental input — learning, understanding, cognitive adaptation",
    },
    Archetype {
        number: 5,
        name: "Significator of the Mind",
        complex: Complex::Mind,
        role: Role::Significator,
        description: "Persistent mental identity — self-concept, intellectual continuity",
    },
    Archetype {
        number: 6,
        name: "Transformation of the Mind",
        complex: Complex::Mind,
        role: Role::Transformation,
        description: "Mental restructuring event — paradigm shift, cognitive breakthrough",
    },
    Archetype {
        number: 7,
        name: "Great Way of the Mind",
        complex: Complex::Mind,
        role: Role::GreatWay,
        description: "Mental operating environment — intellectual community, knowledge domain",
    },
    // ─── Body Complex (8-14) ─────────────────────────────────────────────────
    Archetype {
        number: 8,
        name: "Matrix of the Body",
        complex: Complex::Body,
        role: Role::Matrix,
        description: "Current-state physical organizer — body structure, physical habits, motor patterns",
    },
    Archetype {
        number: 9,
        name: "Potentiator of the Body",
        complex: Complex::Body,
        role: Role::Potentiator,
        description: "Latent physical possibilities — physical potential, unused capabilities",
    },
    Archetype {
        number: 10,
        name: "Catalyst of the Body",
        complex: Complex::Body,
        role: Role::Catalyst,
        description: "Boundary-crossing physical pressure — physical stimuli, environmental demands",
    },
    Archetype {
        number: 11,
        name: "Experience of the Body",
        complex: Complex::Body,
        role: Role::Experience,
        description: "Processed physical input — physical learning, muscle memory, adaptation",
    },
    Archetype {
        number: 12,
        name: "Significator of the Body",
        complex: Complex::Body,
        role: Role::Significator,
        description: "Persistent physical identity — body image, physical continuity",
    },
    Archetype {
        number: 13,
        name: "Transformation of the Body",
        complex: Complex::Body,
        role: Role::Transformation,
        description: "Physical restructuring event — physical breakthrough, body transformation",
    },
    Archetype {
        number: 14,
        name: "Great Way of the Body",
        complex: Complex::Body,
        role: Role::GreatWay,
        description: "Physical operating environment — physical environment, material context",
    },
    // ─── Spirit Complex (15-21) ──────────────────────────────────────────────
    Archetype {
        number: 15,
        name: "Matrix of the Spirit",
        complex: Complex::Spirit,
        role: Role::Matrix,
        description: "Current-state spiritual organizer — spiritual structure, meaning framework",
    },
    Archetype {
        number: 16,
        name: "Potentiator of the Spirit",
        complex: Complex::Spirit,
        role: Role::Potentiator,
        description: "Latent spiritual possibilities — spiritual potential, unrealized purpose",
    },
    Archetype {
        number: 17,
        name: "Catalyst of the Spirit",
        complex: Complex::Spirit,
        role: Role::Catalyst,
        description: "Boundary-crossing spiritual pressure — spiritual challenges, calling, inspiration",
    },
    Archetype {
        number: 18,
        name: "Experience of the Spirit",
        complex: Complex::Spirit,
        role: Role::Experience,
        description: "Processed spiritual input — spiritual growth, wisdom, integration",
    },
    Archetype {
        number: 19,
        name: "Significator of the Spirit",
        complex: Complex::Spirit,
        role: Role::Significator,
        description: "Persistent spiritual identity — soul, spiritual continuity, essential self",
    },
    Archetype {
        number: 20,
        name: "Transformation of the Spirit",
        complex: Complex::Spirit,
        role: Role::Transformation,
        description: "Spiritual restructuring event — spiritual awakening, transformation",
    },
    Archetype {
        number: 21,
        name: "Great Way of the Spirit",
        complex: Complex::Spirit,
        role: Role::GreatWay,
        description: "Spiritual operating environment — spiritual community, evolutionary context",
    },
    // ─── Choice Meta-Pivot (22) ──────────────────────────────────────────────
    Archetype {
        number: 22,
        name: "Choice",
        complex: Complex::Pivot,
        role: Role::Choice,
        description: "The meta-pivot — directional commitment that closes the greater cycle. Uniquely valence-free; re-opens sinkholes and confirms graduations.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archetype_count_is_22() {
        assert_eq!(all_archetypes().len(), 22);
    }

    #[test]
    fn archetype_numbers_are_sequential() {
        for (i, archetype) in all_archetypes().iter().enumerate() {
            assert_eq!(archetype.number, (i + 1) as u8, "Archetype {} has wrong number", i);
        }
    }

    #[test]
    fn mind_complex_has_7() {
        let mind = archetypes_by_complex(&Complex::Mind);
        assert_eq!(mind.len(), 7);
    }

    #[test]
    fn body_complex_has_7() {
        let body = archetypes_by_complex(&Complex::Body);
        assert_eq!(body.len(), 7);
    }

    #[test]
    fn spirit_complex_has_7() {
        let spirit = archetypes_by_complex(&Complex::Spirit);
        assert_eq!(spirit.len(), 7);
    }

    #[test]
    fn pivot_has_1() {
        let pivot = archetypes_by_complex(&Complex::Pivot);
        assert_eq!(pivot.len(), 1);
        assert_eq!(pivot[0].name, "Choice");
    }

    #[test]
    fn each_role_appears_3_times_plus_choice() {
        // Each of the 7 non-Choice roles appears 3 times (Mind + Body + Spirit)
        for role in &[
            Role::Matrix,
            Role::Potentiator,
            Role::Catalyst,
            Role::Experience,
            Role::Significator,
            Role::Transformation,
            Role::GreatWay,
        ] {
            let archetypes = archetypes_by_role(role);
            assert_eq!(archetypes.len(), 3, "Role {:?} should appear 3 times", role);
        }
        // Choice appears once
        let choice = archetypes_by_role(&Role::Choice);
        assert_eq!(choice.len(), 1);
    }

    #[test]
    fn get_archetype_by_number() {
        assert_eq!(archetype_by_number(1).unwrap().name, "Matrix of the Mind");
        assert_eq!(archetype_by_number(22).unwrap().name, "Choice");
        assert_eq!(archetype_by_number(8).unwrap().name, "Matrix of the Body");
        assert_eq!(archetype_by_number(15).unwrap().name, "Matrix of the Spirit");
        assert!(archetype_by_number(0).is_none());
        assert!(archetype_by_number(23).is_none());
    }

    #[test]
    fn get_archetype_by_complex_role() {
        let arch = archetype_by_complex_role(&Complex::Mind, &Role::Matrix).unwrap();
        assert_eq!(arch.number, 1);
        assert_eq!(arch.name, "Matrix of the Mind");

        let arch = archetype_by_complex_role(&Complex::Spirit, &Role::Choice);
        assert!(arch.is_none()); // Choice is in Pivot, not Spirit

        let arch = archetype_by_complex_role(&Complex::Pivot, &Role::Choice).unwrap();
        assert_eq!(arch.number, 22);
    }

    #[test]
    fn role_symbols() {
        assert_eq!(Role::Matrix.as_str(), "M");
        assert_eq!(Role::Potentiator.as_str(), "P");
        assert_eq!(Role::Catalyst.as_str(), "C");
        assert_eq!(Role::Experience.as_str(), "E");
        assert_eq!(Role::Significator.as_str(), "S");
        assert_eq!(Role::Transformation.as_str(), "T");
        assert_eq!(Role::GreatWay.as_str(), "G");
        assert_eq!(Role::Choice.as_str(), "Ch");
    }

    #[test]
    fn complex_strings() {
        assert_eq!(Complex::Mind.as_str(), "mind");
        assert_eq!(Complex::Body.as_str(), "body");
        assert_eq!(Complex::Spirit.as_str(), "spirit");
        assert_eq!(Complex::Pivot.as_str(), "pivot");
    }

    #[test]
    fn choice_is_meta_pivot() {
        let choice = archetype_by_number(22).unwrap();
        assert_eq!(choice.complex, Complex::Pivot);
        assert_eq!(choice.role, Role::Choice);
        assert!(choice.description.contains("meta-pivot"));
    }
}
