//! Canonical schema constants shared across the TDG system.
//!
//! Ported from Python `canonical_schema.py`.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ── Stage (developmental stage, IntEnum in Python) ──

/// The 8 developmental stages of a telos node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Stage {
    Survival = 1,
    Identity = 2,
    Power = 3,
    Heart = 4,
    Rational = 5,
    Pluralistic = 6,
    Integral = 7,
    Harvest = 8,
}

impl Stage {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Survival),
            2 => Some(Self::Identity),
            3 => Some(Self::Power),
            4 => Some(Self::Heart),
            5 => Some(Self::Rational),
            6 => Some(Self::Pluralistic),
            7 => Some(Self::Integral),
            8 => Some(Self::Harvest),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Survival => "SURVIVAL",
            Self::Identity => "IDENTITY",
            Self::Power => "POWER",
            Self::Heart => "HEART",
            Self::Rational => "RATIONAL",
            Self::Pluralistic => "PLURALISTIC",
            Self::Integral => "INTEGRAL",
            Self::Harvest => "HARVEST",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for Stage {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "SURVIVAL" => Ok(Self::Survival),
            "IDENTITY" => Ok(Self::Identity),
            "POWER" => Ok(Self::Power),
            "HEART" => Ok(Self::Heart),
            "RATIONAL" => Ok(Self::Rational),
            "PLURALISTIC" => Ok(Self::Pluralistic),
            "INTEGRAL" => Ok(Self::Integral),
            "HARVEST" => Ok(Self::Harvest),
            _ => Err(format!("Unknown stage: {}", s)),
        }
    }
}

// ── TelosLevel (StrEnum in Python) ──

/// The 7 telological levels (T0–T6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum TelosLevel {
    T0 = 0,
    T1 = 1,
    T2 = 2,
    T3 = 3,
    T4 = 4,
    T5 = 5,
    T6 = 6,
}

impl TelosLevel {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::T0),
            1 => Some(Self::T1),
            2 => Some(Self::T2),
            3 => Some(Self::T3),
            4 => Some(Self::T4),
            5 => Some(Self::T5),
            6 => Some(Self::T6),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl std::fmt::Display for TelosLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "T{}", *self as u8)
    }
}

impl std::str::FromStr for TelosLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_uppercase();
        let s = s.strip_prefix('T').unwrap_or(&s);
        s.parse::<u8>()
            .map_err(|e| e.to_string())
            .and_then(|v| Self::from_u8(v).ok_or_else(|| format!("Unknown telos level: T{}", v)))
    }
}

// ── CatalystType (StrEnum in Python) ──

/// The 10 catalyst types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CatalystType {
    ExternalSuccess,
    ExternalFailure,
    ExternalResponse,
    InternalCompletion,
    InternalDiscovery,
    ConstraintSurfaced,
    OpportunityDetected,
    RoutineObservation,
    SkillMastered,
    ProjectCreated,
}

impl std::fmt::Display for CatalystType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::ExternalSuccess => "external_success",
            Self::ExternalFailure => "external_failure",
            Self::ExternalResponse => "external_response",
            Self::InternalCompletion => "internal_completion",
            Self::InternalDiscovery => "internal_discovery",
            Self::ConstraintSurfaced => "constraint_surfaced",
            Self::OpportunityDetected => "opportunity_detected",
            Self::RoutineObservation => "routine_observation",
            Self::SkillMastered => "skill_mastered",
            Self::ProjectCreated => "project_created",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for CatalystType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "external_success" => Ok(Self::ExternalSuccess),
            "external_failure" => Ok(Self::ExternalFailure),
            "external_response" => Ok(Self::ExternalResponse),
            "internal_completion" => Ok(Self::InternalCompletion),
            "internal_discovery" => Ok(Self::InternalDiscovery),
            "constraint_surfaced" => Ok(Self::ConstraintSurfaced),
            "opportunity_detected" => Ok(Self::OpportunityDetected),
            "routine_observation" => Ok(Self::RoutineObservation),
            "skill_mastered" => Ok(Self::SkillMastered),
            "project_created" => Ok(Self::ProjectCreated),
            _ => Err(format!("Unknown catalyst type: {}", s)),
        }
    }
}

// ── DigestionStatus (StrEnum in Python) ──

/// Digestion pipeline status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DigestionStatus {
    Raw,
    Classified,
    Linked,
    Evaluated,
    Integrated,
    Archived,
    Discarded,
}

impl std::fmt::Display for DigestionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Raw => "raw",
            Self::Classified => "classified",
            Self::Linked => "linked",
            Self::Evaluated => "evaluated",
            Self::Integrated => "integrated",
            Self::Archived => "archived",
            Self::Discarded => "discarded",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for DigestionStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "raw" => Ok(Self::Raw),
            "classified" => Ok(Self::Classified),
            "linked" => Ok(Self::Linked),
            "evaluated" => Ok(Self::Evaluated),
            "integrated" => Ok(Self::Integrated),
            "archived" => Ok(Self::Archived),
            "discarded" => Ok(Self::Discarded),
            _ => Err(format!("Unknown digestion status: {}", s)),
        }
    }
}

// ── Quadrant (StrEnum in Python) ──

/// The 4 quadrants of the AQAL model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Quadrant {
    UL, // Upper-Left (Intentional/Subjective)
    UR, // Upper-Right (Behavioral/Objective)
    LL, // Lower-Left (Cultural/Inter-subjective)
    LR, // Lower-Right (Social/Inter-objective)
}

impl std::fmt::Display for Quadrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::UL => "UL",
            Self::UR => "UR",
            Self::LL => "LL",
            Self::LR => "LR",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for Quadrant {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "UL" => Ok(Self::UL),
            "UR" => Ok(Self::UR),
            "LL" => Ok(Self::LL),
            "LR" => Ok(Self::LR),
            _ => Err(format!("Unknown quadrant: {}", s)),
        }
    }
}

// ── Constants ──

/// Minimum evidence required to advance from each stage.
pub fn stage_evidence_requirements() -> HashMap<Stage, usize> {
    let mut m = HashMap::new();
    m.insert(Stage::Survival, 0);
    m.insert(Stage::Identity, 5);
    m.insert(Stage::Power, 15);
    m.insert(Stage::Heart, 30);
    m.insert(Stage::Rational, 50);
    m.insert(Stage::Pluralistic, 80);
    m.insert(Stage::Integral, 120);
    m.insert(Stage::Harvest, 200);
    m
}

/// Minimum age in days required before advancing from certain stages.
pub fn stage_age_gates() -> HashMap<Stage, u32> {
    let mut m = HashMap::new();
    m.insert(Stage::Power, 3);
    m.insert(Stage::Heart, 7);
    m.insert(Stage::Rational, 14);
    m.insert(Stage::Pluralistic, 21);
    m.insert(Stage::Integral, 45);
    m.insert(Stage::Harvest, 90);
    m
}

/// Maximum allowed stage difference between parent and child.
pub const MAX_PARENT_CHILD_STAGE_DELTA: u8 = 2;

/// Threshold above which bypass risk is flagged.
pub const BYPASS_RISK_THRESHOLD: f64 = 0.5;

/// Maps telos levels to their required promotion stages.
pub fn tlevel_promotion_stage() -> HashMap<TelosLevel, Stage> {
    let mut m = HashMap::new();
    m.insert(TelosLevel::T4, Stage::Survival);
    m.insert(TelosLevel::T3, Stage::Identity);
    m.insert(TelosLevel::T2, Stage::Power);
    m.insert(TelosLevel::T1, Stage::Heart);
    m.insert(TelosLevel::T0, Stage::Rational);
    m.insert(TelosLevel::T5, Stage::Pluralistic);
    m.insert(TelosLevel::T6, Stage::Harvest);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage_roundtrip() {
        for i in 1..=8u8 {
            let s = Stage::from_u8(i).unwrap();
            assert_eq!(Stage::from_u8(s.as_u8()).unwrap(), s);
            assert_eq!(s.as_u8(), i);
        }
    }

    #[test]
    fn test_tlevel_roundtrip() {
        for i in 0..=6u8 {
            let t = TelosLevel::from_u8(i).unwrap();
            assert_eq!(TelosLevel::from_u8(t.as_u8()).unwrap(), t);
        }
    }

    #[test]
    fn test_catalyst_type_parse() {
        let ct: CatalystType = "external_success".parse().unwrap();
        assert_eq!(ct, CatalystType::ExternalSuccess);
    }

    #[test]
    fn test_digestion_status_parse() {
        let ds: DigestionStatus = "raw".parse().unwrap();
        assert_eq!(ds, DigestionStatus::Raw);
    }

    #[test]
    fn test_stage_evidence_reqs() {
        let reqs = stage_evidence_requirements();
        assert_eq!(reqs[&Stage::Survival], 0);
        assert_eq!(reqs[&Stage::Harvest], 200);
    }

    #[test]
    fn test_tlevel_promotion() {
        let promos = tlevel_promotion_stage();
        assert_eq!(promos[&TelosLevel::T4], Stage::Survival);
        assert_eq!(promos[&TelosLevel::T0], Stage::Rational);
    }
}
