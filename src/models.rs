use serde::{Deserialize, Serialize};

// ─── Phase 1.1: Synthesis Status Ladder ─────────────────────────────────────
//
// The epistemic status ladder from TDG theory (HoloOS _THEORY/01_Epistemology/
// 0_Method_of_Holonic_Inquiry.md §5). Every artifact carries a synthesis_status.
// All agent outputs start at AiDraft; elevation above AiDraft is human-only.
//
//   ai-draft → canonical-hypothesis → canonical → superseded

/// Epistemic status of a node (how much trust it has earned).
///
/// Enforces the TDG status ladder: all AI-produced content starts at
/// `AiDraft` and can only be elevated by human authorization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SynthesisStatus {
    /// Constructed but not red-teamed. All agent outputs start here.
    #[default]
    AiDraft,
    /// Derived from anchor, internally consistent, key joints unvalidated.
    CanonicalHypothesis,
    /// Derived, red-teamed, load-bearing joints validated. Human-only elevation.
    Canonical,
    /// Retired; kept as a tombstone for provenance.
    Superseded,
}

impl SynthesisStatus {
    /// Convert to the string stored in the `synthesis_status` column.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AiDraft => "ai-draft",
            Self::CanonicalHypothesis => "canonical-hypothesis",
            Self::Canonical => "canonical",
            Self::Superseded => "superseded",
        }
    }

    /// Parse from a string (case-insensitive, accepts kebab-case and snake_case).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().trim_end_matches('_') {
            "ai-draft" | "ai_draft" | "draft" => Some(Self::AiDraft),
            "canonical-hypothesis" | "canonical_hypothesis" | "hypothesis" => {
                Some(Self::CanonicalHypothesis)
            }
            "canonical" => Some(Self::Canonical),
            "superseded" | "retired" => Some(Self::Superseded),
            _ => None,
        }
    }

    /// Check if elevation from `self` to `target` is a valid ladder transition.
    pub fn can_elevate_to(&self, target: &Self) -> bool {
        matches!(
            (self, target),
            (Self::AiDraft, Self::CanonicalHypothesis)
                | (Self::CanonicalHypothesis, Self::Canonical)
                | (Self::Canonical, Self::Superseded)
                | (Self::AiDraft, Self::Superseded)
                | (Self::CanonicalHypothesis, Self::Superseded)
        )
    }
}

impl std::fmt::Display for SynthesisStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Node types allowed in the graph.
pub const NODE_TYPES: &[&str] = &[
    "observation",
    "telos",
    "skill",
    "capability",
    "action",
    "people",
    "artifact",
    "hypothesis",
    "constraint",
    "discovery",
    "project",
    "trajectory",
    "synthesis",
    // v4.0: Social + Sensor types
    "being",
    "communication",
    "event",
    "insight",
    "question",
    // v4.1: Holonic types
    "value",
    "bond",
    "narrative",
];

/// Edge types allowed in the graph.
pub const EDGE_TYPES: &[&str] = &[
    "DECOMPOSES_TO",
    "OWNS",
    "EXPERIENCES",
    "PURSUES",
    "HAS_CAPABILITY",
    "ENABLES",
    "CONTEXT",
    "BLOCKS",
    "SUPPORTS",
    "CONTRADICTS",
    "EVIDENCES",
    "SYNTHESIZES",
    "DEPENDS_ON",
    "RELATES_TO",
    "REFERENCES",
    "REALIZES",
    "PRECEDES",
    "ALTERNATIVE_TO",
    "OWNED_BY",
    "MEASURED_BY",
    "AFFECTS_QUADRANT",
    "MENTIONS",
    "DIGESTS_TO",
    "PROMOTES_TO",
    // v4.0: Social + Sensor edge types
    "SENT",
    "RECEIVED",
    "TRIGGERED",
    "DETECTED",
    "ILLUMINATES",
    "OPENS",
    "SEEKS",
    "CREATES",
    "ADVANCES",
    "APPEALS_TO",
    "REPLIES",
    "CONTINUES",
];

/// A graph node, matching Python wire format exactly.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Node {
    pub id: String,
    pub node_type: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_properties")]
    pub properties: serde_json::Value,
    #[serde(default = "default_properties")]
    pub quadrants: serde_json::Value,
    #[serde(default = "default_properties")]
    pub drives: serde_json::Value,
    #[serde(default = "default_lifecycle")]
    pub lifecycle_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teleological_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub developmental_stage: Option<i32>,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub parent_ids: Vec<String>,
    #[serde(default)]
    pub agent_path: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    #[serde(default)]
    pub helpful_count: i32,
    #[serde(default)]
    pub retrieval_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    // ─── Phase 1.2: Holonic scaffolding fields ───────────────────────────────
    /// Epistemic status on the TDG ladder. Default: "ai-draft".
    #[serde(default = "default_synthesis_status")]
    pub synthesis_status: String,
    /// Organisational scale code (e.g. "S11"). None = unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_code: Option<String>,
    /// Tetra-Axes UL coordinate (1-19). None = unassigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tetra_ul: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tetra_ur: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tetra_ll: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tetra_lr: Option<i32>,
    /// Octave identifier ("N", "N-1", ...). None = current octave.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub octave_id: Option<String>,
}

fn default_synthesis_status() -> String {
    "ai-draft".to_string()
}

fn default_properties() -> serde_json::Value {
    serde_json::json!({})
}

fn default_lifecycle() -> String {
    "active".to_string()
}

fn default_confidence() -> f64 {
    1.0
}

/// A graph edge, matching Python wire format exactly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub edge_type: String,
    #[serde(default = "default_weight")]
    pub weight: f64,
    #[serde(default = "default_properties")]
    pub properties: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

fn default_weight() -> f64 {
    1.0
}

/// An event in the event store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    pub event_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// A node embedding (vector).
#[derive(Debug, Clone)]
pub struct Embedding {
    pub node_id: String,
    pub vector: Vec<f32>,
    pub model: String,
    pub updated_at: String,
}

/// Drive state for a single drive dimension.
///
/// NOTE: This struct is retained for backward compatibility but is NOT used
/// in production. The canonical drive representation is `flow::DualPoleDrive`
/// (with `positive_pole`, `negative_pole`, `availability`, `blind_spot` fields)
/// and `flow::FlowDriveState` (the 4-drive aggregate). See `src/flow.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveState {
    pub name: String,
    pub value: f64,
    #[serde(default)]
    pub polarity: f64,
}

/// Node creation parameters (what the caller provides).
#[derive(Debug, Clone, Default)]
pub struct NewNode {
    pub node_type: String,
    pub name: String,
    pub description: Option<String>,
    pub properties: Option<serde_json::Value>,
    pub quadrants: Option<serde_json::Value>,
    pub drives: Option<serde_json::Value>,
    pub lifecycle_state: Option<String>,
    pub teleological_level: Option<String>,
    pub developmental_stage: Option<i32>,
    pub confidence: Option<f64>,
    pub source: Option<String>,
    pub parent_ids: Option<Vec<String>>,
    pub agent_id: Option<String>,
    // ─── Phase 1.2: Holonic scaffolding fields (all optional, backward-compatible) ───
    pub synthesis_status: Option<String>,
    pub scale_code: Option<String>,
    pub tetra_ul: Option<i32>,
    pub tetra_ur: Option<i32>,
    pub tetra_ll: Option<i32>,
    pub tetra_lr: Option<i32>,
    pub octave_id: Option<String>,
}

/// Edge creation parameters.
#[derive(Debug, Clone, Default)]
pub struct NewEdge {
    pub source_id: String,
    pub target_id: String,
    pub edge_type: String,
    pub weight: Option<f64>,
    pub properties: Option<serde_json::Value>,
    pub agent_id: Option<String>,
}

/// Query parameters for filtering nodes.
#[derive(Debug, Clone, Default)]
pub struct NodeQuery {
    pub node_type: Option<String>,
    pub lifecycle_state: Option<String>,
    pub source: Option<String>,
    pub teleological_level: Option<String>,
    pub developmental_stage: Option<i32>,
    pub quadrant: Option<String>,
    pub agent_id: Option<String>,
    pub include_deleted: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_serialization_roundtrip() {
        let node = Node {
            id: "n1234567890ab".to_string(),
            node_type: "observation".to_string(),
            name: "Test Node".to_string(),
            description: "A test".to_string(),
            properties: serde_json::json!({"key": "value"}),
            quadrants: serde_json::json!({}),
            drives: serde_json::json!({}),
            lifecycle_state: "active".to_string(),
            teleological_level: Some("L2".to_string()),
            developmental_stage: Some(3),
            confidence: 0.85,
            source: "test".to_string(),
            parent_ids: vec!["n_parent".to_string()],
            agent_path: "/test".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
            valid_from: None,
            valid_to: None,
            helpful_count: 0,
            retrieval_count: 0,
            agent_id: None,
            ..Default::default()
        };

        let json = serde_json::to_string(&node).unwrap();
        let parsed: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, node.id);
        assert_eq!(parsed.node_type, "observation");
        assert_eq!(parsed.confidence, 0.85);
    }

    #[test]
    fn node_types_count() {
        assert_eq!(NODE_TYPES.len(), 21);
    }

    #[test]
    fn edge_types_count() {
        assert!(EDGE_TYPES.len() > 30);
    }
}
