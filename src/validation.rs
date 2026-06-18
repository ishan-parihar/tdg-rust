use std::collections::HashMap;

use crate::models::{EDGE_TYPES, NODE_TYPES};

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

/// Validate a node creation request.
///
/// Returns `Ok(())` if valid, or `Err(message)` with details about what's missing.
pub fn validate_node_creation(
    node_type: &str,
    fields: &HashMap<String, serde_json::Value>,
) -> Result<(), String> {
    if !NODE_TYPES.contains(&node_type) {
        return Err(format!("Unknown node type: '{node_type}'"));
    }

    let contracts = node_contracts();
    if let Some(contract) = contracts.get(node_type) {
        // Check required fields
        let mut missing_required = Vec::new();
        for field in &contract.required {
            if !fields.contains_key(*field) || fields.get(*field).is_some_and(|v| v.is_null()) {
                missing_required.push(*field);
            }
        }

        if !missing_required.is_empty() {
            let mut msg = format!("Missing required fields for '{node_type}':");
            for field in &missing_required {
                let guidance = contract
                    .contextual_guidance
                    .get(field)
                    .unwrap_or(&"Required field missing");
                msg.push_str(&format!("\n  - {field}: {guidance}"));
            }
            return Err(msg);
        }

        // Check strongly recommended fields (warnings)
        let mut missing_recommended = Vec::new();
        for field in &contract.strongly_recommended {
            if !fields.contains_key(*field) || fields.get(*field).is_some_and(|v| v.is_null()) {
                missing_recommended.push(*field);
            }
        }

        if !missing_recommended.is_empty() {
            let mut msg = format!("Warning: recommended fields missing for '{node_type}':");
            for field in &missing_recommended {
                msg.push_str(&format!("\n  - {field}"));
            }
            // Return Ok with a warning (caller can log it)
            // For strict mode, this could be an error
            tracing::warn!("{}", msg);
        }
    }

    Ok(())
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

/// Validate text length.
pub fn validate_text(text: &str, field_name: &str) -> Result<(), String> {
    if text.len() > MAX_TEXT_LENGTH {
        return Err(format!(
            "{field_name} exceeds maximum length of {MAX_TEXT_LENGTH} characters (got {})",
            text.len()
        ));
    }
    Ok(())
}

/// Validate node ID length.
pub fn validate_node_id(id: &str) -> Result<(), String> {
    if id.len() > MAX_NODE_ID_LENGTH {
        return Err(format!(
            "Node ID exceeds maximum length of {MAX_NODE_ID_LENGTH} characters (got {})",
            id.len()
        ));
    }
    if id.is_empty() {
        return Err("Node ID cannot be empty".to_string());
    }
    Ok(())
}

/// Validate limit parameter.
pub fn validate_limit(limit: i64) -> Result<(), String> {
    if limit < 0 {
        return Err("Limit cannot be negative".to_string());
    }
    if limit > MAX_LIMIT {
        return Err(format!("Limit {limit} exceeds maximum of {MAX_LIMIT}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validate_observation_requires_name() {
        let mut fields = HashMap::new();
        let result = validate_node_creation("observation", &fields);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("name"));

        fields.insert("name".to_string(), json!("test"));
        assert!(validate_node_creation("observation", &fields).is_ok());
    }

    #[test]
    fn validate_action_requires_parent_ids() {
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), json!("do something"));
        let result = validate_node_creation("action", &fields);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("parent_ids"));
    }

    #[test]
    fn validate_unknown_node_type() {
        let fields = HashMap::new();
        let result = validate_node_creation("bogus", &fields);
        assert!(result.is_err());
    }

    #[test]
    fn validate_edge_types() {
        assert!(validate_edge_creation("telos", "telos", "DECOMPOSES_TO").is_ok());
        assert!(validate_edge_creation("telos", "action", "DECOMPOSES_TO").is_ok());
        assert!(validate_edge_creation("observation", "hypothesis", "EVIDENCES").is_ok());
        assert!(validate_edge_creation("constraint", "action", "BLOCKS").is_ok());
    }

    #[test]
    fn validate_text_length_ok() {
        assert!(validate_text("hello", "name").is_ok());
    }

    #[test]
    fn validate_text_length_exceeded() {
        let long_text = "x".repeat(MAX_TEXT_LENGTH + 1);
        assert!(validate_text(&long_text, "name").is_err());
    }

    #[test]
    fn validate_node_id_empty() {
        assert!(validate_node_id("").is_err());
    }

    #[test]
    fn test_validate_limit() {
        assert!(validate_limit(0).is_ok());
        assert!(validate_limit(500).is_ok());
        assert!(validate_limit(MAX_LIMIT).is_ok());
        assert!(validate_limit(MAX_LIMIT + 1).is_err());
        assert!(validate_limit(-1).is_err());
    }
}
