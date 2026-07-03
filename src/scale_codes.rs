//! Scale codes — the organisational scale taxonomy from HoloOS.
//!
//! Source: `HoloOS/_INSTRUMENTS/schemas/taxonomy/scale_codes.yaml`
//!
//! Every holon occupies a position on a scalar axis. The scale code
//! determines the holon's organisational level — from cosmic (S00) down
//! to linguistic (S80). This is distinct from `telos_level` (which is
//! developmental) and `developmental_stage` (which is temporal).
//!
//! Scale codes are used by:
//! - Phase 3 (attractor field): scale determines bonding candidates
//! - Phase 5 (ContextPack): `analogues` are cross-domain type-homologues
//!   at the same scale
//! - Phase 6 (type system): type_class is scale-aware
//!
//! ## Tetra-Axes
//!
//! In addition to the organisational S-code, every holon has Tetra-Axes
//! coordinates (UL, UR, LL, LR), each 1-19. This gives 130,321 possible
//! scale positions. The Tetra-Axes follow the Wilber AQAL quadrants:
//!
//! | Axis | Quadrant | Description |
//! |------|----------|-------------|
//! | UL | Interior × Individual | consciousness, subjective experience |
//! | UR | Exterior × Individual | body, brain, technology |
//! | LL | Interior × Collective | culture, shared meaning |
//! | LR | Exterior × Collective | systems, institutions |

/// Organisational scale codes (S00–S80) from HoloOS scale_codes.yaml.
///
/// Each tuple is (code, name, description).
pub const SCALE_CODES: &[(&str, &str, &str)] = &[
    ("S00", "Cosmic", "Universal / cosmic scale"),
    ("S01", "Galactic", "Galactic scale"),
    ("S02", "Stellar", "Stellar / solar system scale"),
    ("S03", "Planetary", "Planetary scale"),
    ("S04", "Biospheric", "Biosphere / ecosphere scale"),
    ("S10", "Civilizational_Bloc", "Multi-civilization bloc (e.g. 'the West', 'Global South')"),
    ("S11", "Civilization", "A single civilization (e.g. India, China, Islam)"),
    ("S20", "Sub_Civilizational", "Sub-civilizational region (e.g. South India, Andalusia)"),
    ("S30", "Organizational", "Formal organization (e.g. a corporation, NGO, state)"),
    ("S31", "Network", "Informal network (e.g. a professional community)"),
    ("S32", "Family", "Family / kinship group"),
    ("S40", "Individual", "A single person or agent"),
    ("S41", "Sub_Individual", "Sub-individual (e.g. an organ, a personality sub-self)"),
    ("S50", "Artifactual", "A human-made artifact (e.g. a book, a tool, a codebase)"),
    ("S60", "Phenomenal", "A natural phenomenon (e.g. a storm, a disease, a market crash)"),
    ("S70", "Conceptual", "A concept or abstraction (e.g. a theory, a law, a meme)"),
    ("S80", "Linguistic", "A linguistic unit (e.g. a word, a phrase, a grammar rule)"),
];

/// Default scale code for a given node type.
///
/// Used when `NewNode.scale_code` is None. Inferred from the node type's
/// typical organisational level. Returns None for types with no clear
/// default (the caller should set it explicitly).
pub fn default_scale_for_type(node_type: &str) -> Option<&'static str> {
    match node_type {
        "observation" | "insight" | "question" => Some("S40"), // Individual
        "people" | "being" => Some("S40"), // Individual
        "skill" | "capability" | "action" => Some("S40"), // Individual
        "project" | "trajectory" => Some("S30"), // Organizational
        "telos" | "value" => Some("S30"), // Organizational
        "hypothesis" | "synthesis" | "discovery" => Some("S70"), // Conceptual
        "constraint" | "narrative" => Some("S70"), // Conceptual
        "artifact" => Some("S50"), // Artifactual
        "event" | "communication" => Some("S40"), // Individual
        "bond" => Some("S31"), // Network
        _ => None,
    }
}

/// Check if a scale code string is valid.
pub fn is_valid_scale(code: &str) -> bool {
    SCALE_CODES.iter().any(|(c, _, _)| *c == code)
}

/// Get the name for a scale code (e.g. "S11" → "Civilization").
pub fn scale_name(code: &str) -> Option<&'static str> {
    SCALE_CODES
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, name, _)| *name)
}

/// Get the description for a scale code.
pub fn scale_description(code: &str) -> Option<&'static str> {
    SCALE_CODES
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, _, desc)| *desc)
}

/// Validate a Tetra-Axes coordinate (must be 1-19, or None for unassigned).
pub fn validate_tetra_coord(coord: Option<i32>) -> Result<Option<i32>, String> {
    match coord {
        None => Ok(None),
        Some(v) if (1..=19).contains(&v) => Ok(Some(v)),
        Some(v) => Err(format!(
            "Tetra-Axes coordinate must be 1-19, got {v}"
        )),
    }
}

/// Validate all four Tetra-Axes coordinates.
pub fn validate_tetra_coords(
    ul: Option<i32>,
    ur: Option<i32>,
    ll: Option<i32>,
    lr: Option<i32>,
) -> Result<(), String> {
    validate_tetra_coord(ul)?;
    validate_tetra_coord(ur)?;
    validate_tetra_coord(ll)?;
    validate_tetra_coord(lr)?;
    Ok(())
}

/// Octave identifiers for cross-octave involution lineage.
///
/// "N" = current octave (default). "N-1", "N-2", etc. = previous octaves
/// (involution). "N+1" = next octave (evolutionary projection).
pub const VALID_OCTAVE_IDS: &[&str] = &["N", "N-1", "N-2", "N-3", "N-4", "N+1"];

/// Validate an octave identifier.
pub fn is_valid_octave(octave: &str) -> bool {
    VALID_OCTAVE_IDS.contains(&octave)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_codes_nonempty() {
        assert!(SCALE_CODES.len() >= 15);
        assert!(SCALE_CODES.iter().any(|(c, _, _)| *c == "S11"));
        assert!(SCALE_CODES.iter().any(|(c, _, _)| *c == "S40"));
    }

    #[test]
    fn scale_validation() {
        assert!(is_valid_scale("S00"));
        assert!(is_valid_scale("S11"));
        assert!(is_valid_scale("S80"));
        assert!(!is_valid_scale("S99"));
        assert!(!is_valid_scale(""));
        assert!(!is_valid_scale("invalid"));
    }

    #[test]
    fn scale_name_lookup() {
        assert_eq!(scale_name("S11"), Some("Civilization"));
        assert_eq!(scale_name("S40"), Some("Individual"));
        assert_eq!(scale_name("S99"), None);
    }

    #[test]
    fn default_scale_inference() {
        assert_eq!(default_scale_for_type("observation"), Some("S40"));
        assert_eq!(default_scale_for_type("project"), Some("S30"));
        assert_eq!(default_scale_for_type("hypothesis"), Some("S70"));
        assert_eq!(default_scale_for_type("artifact"), Some("S50"));
        assert_eq!(default_scale_for_type("unknown_type"), None);
    }

    #[test]
    fn tetra_coord_validation() {
        assert_eq!(validate_tetra_coord(None).unwrap(), None);
        assert_eq!(validate_tetra_coord(Some(1)).unwrap(), Some(1));
        assert_eq!(validate_tetra_coord(Some(19)).unwrap(), Some(19));
        assert!(validate_tetra_coord(Some(0)).is_err());
        assert!(validate_tetra_coord(Some(20)).is_err());
        assert!(validate_tetra_coord(Some(-1)).is_err());
    }

    #[test]
    fn tetra_coords_batch_validation() {
        assert!(validate_tetra_coords(Some(1), Some(2), Some(3), Some(4)).is_ok());
        assert!(validate_tetra_coords(None, None, None, None).is_ok());
        assert!(validate_tetra_coords(Some(0), None, None, None).is_err());
        assert!(validate_tetra_coords(None, Some(20), None, None).is_err());
    }

    #[test]
    fn octave_validation() {
        assert!(is_valid_octave("N"));
        assert!(is_valid_octave("N-1"));
        assert!(is_valid_octave("N-4"));
        assert!(is_valid_octave("N+1"));
        assert!(!is_valid_octave("N-5"));
        assert!(!is_valid_octave("X"));
        assert!(!is_valid_octave(""));
    }
}
