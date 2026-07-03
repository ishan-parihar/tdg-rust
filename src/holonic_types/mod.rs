//! Holonic Types — the 22 archetypes + type validation (Phase 6).
//!
//! This module provides the typological library and the T1/T2/T3 type
//! validation tests that enforce Type⊥Stage orthogonality.
//!
//! ## 22 Named Archetypes
//!
//! 7 functional roles (M·P·C·E·S·T·G) × 3 complexes (Mind, Body, Spirit) = 21
//! + 1 Choice meta-pivot = 22.
//!
//! The 8 functional roles are the **operators**; the 22 named archetypes are
//! the **operands** — they give concrete expression to the roles within each
//! complex.
//!
//! ## T1/T2/T3 Type Validation
//!
//! A Type claims a holon's invariant valence signature. Validated iff:
//!
//! | Test | Question |
//! |------|----------|
//! | T1 | Does observed bonding match the type's prediction? |
//! | T2 | Does type_class stay fixed across stage transitions? |
//! | T3 | Does type_class persist across metabolic cycles? |

pub mod archetypes;
pub mod type_validation;

pub use archetypes::{
    all_archetypes, archetype_by_complex_role, archetype_by_number, archetypes_by_complex,
    archetypes_by_role, Archetype, Complex, Role,
};
pub use type_validation::{
    check_type_stage_orthogonality, t1_behavioral_match, t2_excitation_invariance,
    t3_fixed_point_persistence, validate_type, TypeValidationResult,
};
