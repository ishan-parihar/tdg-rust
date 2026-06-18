//! Node grammar: catalyst→node blueprint mapping and upward pattern inference.
//!
//! Ported from Python `tdg_node_grammar.py` (194 lines).

use std::collections::HashMap;

use rusqlite::Connection;

use crate::db::crud;
use crate::error::TdgResult;
use crate::grammar::auto_wire::auto_wire_edges;
use crate::models::{NewNode, Node};
use crate::schema::CatalystType;

/// Blueprint for creating a node from a catalyst.
#[derive(Debug, Clone)]
pub struct NodeBlueprint {
    pub node_type: String,
    pub t_level: String,
    pub title_template: String,
    pub quadrant: Option<String>,
    pub default_stage: Option<i32>,
    pub require_parent: bool,
    pub require_evidence: bool,
}

/// Build the CATALYST_GRAMMAR mapping.
fn catalyst_grammar() -> HashMap<CatalystType, NodeBlueprint> {
    let mut m = HashMap::new();

    m.insert(
        CatalystType::ExternalSuccess,
        NodeBlueprint {
            node_type: "observation".into(),
            t_level: "T4".into(),
            title_template: "Observed: {description}".into(),
            quadrant: None,
            default_stage: Some(1),
            require_parent: false,
            require_evidence: false,
        },
    );
    m.insert(
        CatalystType::ExternalFailure,
        NodeBlueprint {
            node_type: "observation".into(),
            t_level: "T4".into(),
            title_template: "Failure observed: {description}".into(),
            quadrant: None,
            default_stage: Some(1),
            require_parent: false,
            require_evidence: false,
        },
    );
    m.insert(
        CatalystType::ExternalResponse,
        NodeBlueprint {
            node_type: "observation".into(),
            t_level: "T4".into(),
            title_template: "Response received: {description}".into(),
            quadrant: None,
            default_stage: Some(1),
            require_parent: false,
            require_evidence: false,
        },
    );
    m.insert(
        CatalystType::InternalCompletion,
        NodeBlueprint {
            node_type: "action".into(),
            t_level: "T4".into(),
            title_template: "Completed: {description}".into(),
            quadrant: None,
            default_stage: Some(2),
            require_parent: true,
            require_evidence: false,
        },
    );
    m.insert(
        CatalystType::InternalDiscovery,
        NodeBlueprint {
            node_type: "discovery".into(),
            t_level: "T3".into(),
            title_template: "Discovered: {description}".into(),
            quadrant: None,
            default_stage: Some(2),
            require_parent: false,
            require_evidence: false,
        },
    );
    m.insert(
        CatalystType::ConstraintSurfaced,
        NodeBlueprint {
            node_type: "constraint".into(),
            t_level: "T3".into(),
            title_template: "Constraint: {description}".into(),
            quadrant: None,
            default_stage: Some(1),
            require_parent: false,
            require_evidence: false,
        },
    );
    m.insert(
        CatalystType::OpportunityDetected,
        NodeBlueprint {
            node_type: "discovery".into(),
            t_level: "T3".into(),
            title_template: "Opportunity: {description}".into(),
            quadrant: None,
            default_stage: Some(1),
            require_parent: false,
            require_evidence: false,
        },
    );
    m.insert(
        CatalystType::RoutineObservation,
        NodeBlueprint {
            node_type: "observation".into(),
            t_level: "T4".into(),
            title_template: "{description}".into(),
            quadrant: None,
            default_stage: Some(1),
            require_parent: false,
            require_evidence: false,
        },
    );
    m.insert(
        CatalystType::SkillMastered,
        NodeBlueprint {
            node_type: "skill".into(),
            t_level: "T3".into(),
            title_template: "Skill: {description}".into(),
            quadrant: None,
            default_stage: Some(3),
            require_parent: false,
            require_evidence: false,
        },
    );
    m.insert(
        CatalystType::ProjectCreated,
        NodeBlueprint {
            node_type: "project".into(),
            t_level: "T2".into(),
            title_template: "Project: {description}".into(),
            quadrant: None,
            default_stage: Some(1),
            require_parent: false,
            require_evidence: false,
        },
    );

    m
}

/// Grammar engine for creating nodes from catalysts.
pub struct NodeGrammar;

impl NodeGrammar {
    /// Create a node from a catalyst type with the given description.
    ///
    /// Returns the created node and the number of auto-wired edges.
    pub fn create_for_catalyst(
        conn: &Connection,
        catalyst: CatalystType,
        description: &str,
        parent_ids: Option<Vec<String>>,
        source_node_id: Option<&str>,
    ) -> TdgResult<(Node, usize)> {
        let grammar = catalyst_grammar();
        let blueprint = grammar.get(&catalyst).ok_or_else(|| {
            crate::error::TdgError::Custom(format!("Unknown catalyst type: {:?}", catalyst))
        })?;

        // Build title from template
        let title = blueprint
            .title_template
            .replace("{description}", description);

        let new_node = NewNode {
            node_type: blueprint.node_type.clone(),
            name: title,
            description: Some(description.to_string()),
            properties: None,
            quadrants: blueprint
                .quadrant
                .as_ref()
                .map(|q| serde_json::json!({"primary": q})),
            drives: None,
            lifecycle_state: None,
            teleological_level: Some(blueprint.t_level.clone()),
            developmental_stage: blueprint.default_stage,
            confidence: None,
            source: source_node_id.map(|s| s.to_string()),
            parent_ids: parent_ids.clone(),
            agent_id: None,
        };

        let node = crud::add_node(conn, &new_node)?;

        // Auto-wire edges to parents
        let parents = parent_ids.unwrap_or_default();
        let edges_created = auto_wire_edges(conn, &node.id, &node.node_type, &parents)?;

        Ok((node, edges_created))
    }

    /// Infer upward patterns: scan T4 observations, create T3 hypotheses from
    /// patterns with 3+ occurrences.
    ///
    /// Returns created hypothesis nodes.
    pub fn infer_upward_pattern(conn: &Connection) -> TdgResult<Vec<Node>> {
        // Find all active T4 observations
        let observations: Vec<Node> = {
            let mut stmt = conn.prepare(
                "SELECT id, node_type, name, description, properties_json, quadrants_json,
                 drives_json, lifecycle_state, teleological_level, developmental_stage,
                 confidence, source, parent_ids, agent_path, created_at, updated_at,
                 valid_from, valid_to, helpful_count, retrieval_count, agent_id
                 FROM nodes
                 WHERE teleological_level = 'T4'
                   AND node_type = 'observation'
                   AND lifecycle_state = 'active'",
            )?;

            let rows = stmt.query_map([], |row| {
                Ok(Node {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    properties: serde_json::from_str(&row.get::<_, String>(4)?)
                        .unwrap_or(serde_json::json!({})),
                    quadrants: serde_json::from_str(&row.get::<_, String>(5)?)
                        .unwrap_or(serde_json::json!({})),
                    drives: serde_json::from_str(&row.get::<_, String>(6)?)
                        .unwrap_or(serde_json::json!({})),
                    lifecycle_state: row.get(7)?,
                    teleological_level: row.get(8)?,
                    developmental_stage: row.get(9)?,
                    confidence: row.get(10)?,
                    source: row.get(11)?,
                    parent_ids: serde_json::from_str(&row.get::<_, String>(12)?)
                        .unwrap_or_default(),
                    agent_path: row.get(13)?,
                    created_at: row.get(14)?,
                    updated_at: row.get(15)?,
                    valid_from: row.get(16)?,
                    valid_to: row.get(17)?,
                    helpful_count: row.get(18)?,
                    retrieval_count: row.get(19)?,
                    agent_id: row.get(20)?,
                })
            })?;

            rows.filter_map(|r| r.ok()).collect()
        };

        if observations.is_empty() {
            return Ok(vec![]);
        }

        // Group observations by name pattern (simplified: use name as proxy)
        let mut pattern_counts: HashMap<String, Vec<&Node>> = HashMap::new();
        for obs in &observations {
            // Normalize name for pattern detection
            let key = obs.name.to_lowercase().chars().take(50).collect::<String>();
            pattern_counts.entry(key).or_default().push(obs);
        }

        let mut hypotheses = Vec::new();

        // Create hypothesis for patterns with 3+ occurrences
        for (pattern, obs_list) in &pattern_counts {
            if obs_list.len() < 3 {
                continue;
            }

            let parent_ids: Vec<String> = obs_list.iter().map(|o| o.id.clone()).collect();
            let description = format!(
                "Pattern detected: '{}' ({} observations)",
                pattern,
                obs_list.len()
            );

            let (hypothesis, _) = Self::create_for_catalyst(
                conn,
                CatalystType::InternalDiscovery,
                &description,
                Some(parent_ids),
                None,
            )?;

            hypotheses.push(hypothesis);
        }

        Ok(hypotheses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_schema;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn test_create_observation_from_catalyst() {
        let conn = setup_db();
        let (node, edges) = NodeGrammar::create_for_catalyst(
            &conn,
            CatalystType::ExternalSuccess,
            "User reported positive feedback",
            None,
            None,
        )
        .unwrap();

        assert_eq!(node.node_type, "observation");
        assert_eq!(node.teleological_level, Some("T4".into()));
        assert!(node.name.contains("User reported"));
        assert_eq!(edges, 0); // No parents, no edges
    }

    #[test]
    fn test_create_action_with_parent() {
        let conn = setup_db();

        // Create a telos first
        let telos = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".into(),
                name: "Improve performance".into(),
                description: None,
                properties: None,
                quadrants: None,
                drives: None,
                lifecycle_state: None,
                teleological_level: Some("T2".into()),
                developmental_stage: None,
                confidence: None,
                source: None,
                parent_ids: None,
                agent_id: None,
            },
        )
        .unwrap();

        let (action, edges) = NodeGrammar::create_for_catalyst(
            &conn,
            CatalystType::InternalCompletion,
            "Optimized query cache",
            Some(vec![telos.id.clone()]),
            None,
        )
        .unwrap();

        assert_eq!(action.node_type, "action");
        assert_eq!(edges, 1); // DECOMPOSES_TO telos
    }

    #[test]
    fn test_catalyst_type_parsing() {
        use std::str::FromStr;
        assert_eq!(
            CatalystType::from_str("external_success").ok(),
            Some(CatalystType::ExternalSuccess)
        );
        assert_eq!(
            "EXTERNAL_SUCCESS".parse::<CatalystType>().ok(),
            Some(CatalystType::ExternalSuccess)
        );
        assert_eq!(
            "skill_mastered".parse::<CatalystType>().ok(),
            Some(CatalystType::SkillMastered)
        );
        assert!("unknown".parse::<CatalystType>().is_err());
    }

    #[test]
    fn test_infer_upward_pattern_no_observations() {
        let conn = setup_db();
        let hypotheses = NodeGrammar::infer_upward_pattern(&conn).unwrap();
        assert!(hypotheses.is_empty());
    }
}
