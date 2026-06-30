use std::collections::HashMap;

use crate::models::EDGE_TYPES;

/// Maximum text length for node descriptions.
pub const MAX_TEXT_LENGTH: usize = 50_000;
/// Maximum node ID length.
pub const MAX_NODE_ID_LENGTH: usize = 256;
/// Maximum number of aliases per node.
pub const MAX_ALIASES: usize = 100;
/// Maximum limit for query pagination.
pub const MAX_LIMIT: i64 = 1000;
/// Maximum conversation turns.
pub const MAX_TURNS: usize = 500;
/// Maximum nodes per bulk operation.
pub const MAX_BULK_NODES: usize = 500;

/// Validation contract for a node type.
#[derive(Debug, Clone)]
pub struct NodeContract {
    pub required: Vec<&'static str>,
    pub strongly_recommended: Vec<&'static str>,
    pub contextual_guidance: HashMap<&'static str, &'static str>,
    pub auto_wire_on_parent: Vec<&'static str>,
}

/// Build the full NODE_CONTRACTS map.
pub fn node_contracts() -> HashMap<&'static str, NodeContract> {
    let mut m: HashMap<&'static str, NodeContract> = HashMap::new();

    m.insert(
        "observation",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description", "source", "quadrant"],
            contextual_guidance: HashMap::from([
                ("source", "Observations without a source lose provenance"),
                (
                    "quadrant",
                    "Quadrant helps classify where the observation lives",
                ),
            ]),
            auto_wire_on_parent: vec!["EVIDENCES"],
        },
    );

    m.insert(
        "action",
        NodeContract {
            required: vec!["name", "parent_ids"],
            strongly_recommended: vec!["description", "quadrant"],
            contextual_guidance: HashMap::from([(
                "parent_ids",
                "An action without parent_ids is disconnected from purpose",
            )]),
            auto_wire_on_parent: vec!["DECOMPOSES_TO"],
        },
    );

    m.insert(
        "trajectory",
        NodeContract {
            required: vec!["name", "parent_ids"],
            strongly_recommended: vec!["description"],
            contextual_guidance: HashMap::from([(
                "parent_ids",
                "Trajectories must connect to a telos",
            )]),
            auto_wire_on_parent: vec!["DECOMPOSES_TO"],
        },
    );

    m.insert(
        "telos",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description", "quadrant"],
            contextual_guidance: HashMap::new(),
            auto_wire_on_parent: vec![],
        },
    );

    m.insert(
        "people",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description"],
            contextual_guidance: HashMap::new(),
            auto_wire_on_parent: vec![],
        },
    );

    m.insert(
        "artifact",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description", "source"],
            contextual_guidance: HashMap::new(),
            auto_wire_on_parent: vec![],
        },
    );

    m.insert(
        "hypothesis",
        NodeContract {
            required: vec!["name", "parent_ids"],
            strongly_recommended: vec!["description", "confidence"],
            contextual_guidance: HashMap::from([(
                "parent_ids",
                "Hypotheses should be linked to a telos or observation",
            )]),
            auto_wire_on_parent: vec!["EVIDENCES"],
        },
    );

    m.insert(
        "constraint",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description", "quadrant"],
            contextual_guidance: HashMap::from([(
                "description",
                "A constraint that doesn't block anything is a zombie",
            )]),
            auto_wire_on_parent: vec!["BLOCKS"],
        },
    );

    m.insert(
        "skill",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description"],
            contextual_guidance: HashMap::new(),
            auto_wire_on_parent: vec!["HAS_CAPABILITY"],
        },
    );

    m.insert(
        "discovery",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description", "source"],
            contextual_guidance: HashMap::new(),
            auto_wire_on_parent: vec!["EVIDENCES"],
        },
    );

    m.insert(
        "project",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description", "quadrant"],
            contextual_guidance: HashMap::new(),
            auto_wire_on_parent: vec![],
        },
    );

    m.insert(
        "capability",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description"],
            contextual_guidance: HashMap::new(),
            auto_wire_on_parent: vec![],
        },
    );

    m.insert(
        "synthesis",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description"],
            contextual_guidance: HashMap::new(),
            auto_wire_on_parent: vec!["SYNTHESIZES"],
        },
    );

    m.insert(
        "being",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description"],
            contextual_guidance: HashMap::new(),
            auto_wire_on_parent: vec![],
        },
    );

    m.insert(
        "communication",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description", "source"],
            contextual_guidance: HashMap::new(),
            auto_wire_on_parent: vec!["SENT", "RECEIVED"],
        },
    );

    m.insert(
        "event",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description"],
            contextual_guidance: HashMap::new(),
            auto_wire_on_parent: vec!["TRIGGERED"],
        },
    );

    m.insert(
        "insight",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description", "source"],
            contextual_guidance: HashMap::new(),
            auto_wire_on_parent: vec!["ILLUMINATES"],
        },
    );

    m.insert(
        "question",
        NodeContract {
            required: vec!["name"],
            strongly_recommended: vec!["description"],
            contextual_guidance: HashMap::new(),
            auto_wire_on_parent: vec!["SEEKS"],
        },
    );

    m
}

/// Valid edge patterns: (source_type, target_type) -> allowed edge types.
fn valid_edge_patterns() -> HashMap<(&'static str, &'static str), Vec<&'static str>> {
    let mut m = HashMap::new();
    m.insert(("telos", "telos"), vec!["DECOMPOSES_TO"]);
    m.insert(("telos", "action"), vec!["DECOMPOSES_TO"]);
    m.insert(("telos", "trajectory"), vec!["DECOMPOSES_TO"]);
    m.insert(("action", "action"), vec!["DEPENDS_ON"]);
    m.insert(("observation", "hypothesis"), vec!["EVIDENCES"]);
    m.insert(("observation", "telos"), vec!["EVIDENCES"]);
    m.insert(("hypothesis", "telos"), vec!["EVIDENCES"]);
    m.insert(("constraint", "telos"), vec!["BLOCKS"]);
    m.insert(("constraint", "action"), vec!["BLOCKS"]);
    m.insert(("skill", "telos"), vec!["ENABLES"]);
    m.insert(("skill", "action"), vec!["ENABLES"]);
    m.insert(("people", "skill"), vec!["HAS_CAPABILITY"]);
    m.insert(("people", "telos"), vec!["PURSUES"]);
    m.insert(("project", "telos"), vec!["OWNS"]);
    m.insert(("synthesis", "observation"), vec!["SYNTHESIZES"]);
    m.insert(("synthesis", "hypothesis"), vec!["SYNTHESIZES"]);
    m
}

/// Validate an edge creation request.
///
/// Returns `Ok(())` if valid, or `Err(message)` with details.
pub fn validate_edge_creation(
    source_type: &str,
    target_type: &str,
    edge_type: &str,
) -> Result<(), String> {
    if !EDGE_TYPES.contains(&edge_type) {
        return Err(format!("Unknown edge type: '{edge_type}'"));
    }

    let patterns = valid_edge_patterns();
    let key = (source_type, target_type);

    if let Some(allowed) = patterns.get(&key) {
        if !allowed.contains(&edge_type) {
            return Err(format!(
                "Invalid edge: {source_type} --[{edge_type}]--> {target_type}. \
                 Allowed edge types for this pair: {allowed:?}"
            ));
        }
    }
    // If no pattern defined, allow it (permissive mode for custom edges)

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validate_edge_types() {
        assert!(validate_edge_creation("telos", "telos", "DECOMPOSES_TO").is_ok());
        assert!(validate_edge_creation("telos", "action", "DECOMPOSES_TO").is_ok());
        assert!(validate_edge_creation("observation", "hypothesis", "EVIDENCES").is_ok());
        assert!(validate_edge_creation("constraint", "action", "BLOCKS").is_ok());
    }
}
