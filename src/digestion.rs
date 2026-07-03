use crate::db::crud;
use crate::error::TdgResult;
use crate::models::{NewEdge, NewNode, Node, NodeQuery};
use crate::schema::{CatalystType, DigestionStatus};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestionEvent {
    pub event_id: String,
    pub catalyst_type: String,
    pub catalyst_source_node: String,
    pub catalyst_description: String,
    pub t_level_processed_at: String,
    pub stage_evidence_contributed: f64,
    pub integration_quality: f64,
    pub affected_telos_nodes: Vec<String>,
    pub new_edges_created: usize,
    pub timestamp: String,
}

pub struct DigestionEngine<'a> {
    conn: &'a Connection,
    min_similar_for_hypothesis: usize,
}

impl<'a> DigestionEngine<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self {
            conn,
            min_similar_for_hypothesis: 3,
        }
    }

    pub fn digest_catalyst(
        &self,
        source_node_id: &str,
        catalyst_type: &CatalystType,
        description: &str,
    ) -> TdgResult<Node> {
        let obs = crud::add_node(
            self.conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: format!("{} observation: {}", catalyst_type, description.chars().take(50).collect::<String>()),
                description: Some(description.to_string()),
                properties: Some(serde_json::json!({
                    "catalyst_type": catalyst_type.to_string(),
                    "status": DigestionStatus::Raw.to_string(),
                })),
                quadrants: None,
                drives: None,
                lifecycle_state: None,
                teleological_level: Some("T4".to_string()),
                developmental_stage: None,
                confidence: Some(0.5),
                source: Some("digestion".to_string()),
                parent_ids: None,
                agent_id: None,
                ..Default::default()
            },
        )?;

        crud::add_edge(
            self.conn,
            &NewEdge {
                source_id: obs.id.clone(),
                target_id: source_node_id.to_string(),
                edge_type: "EVIDENCES".to_string(),
                weight: None,
                properties: None,
                agent_id: Some("digestion".to_string()),
            },
        )?;

        Ok(obs)
    }

    pub fn check_upward_cascade(&self) -> TdgResult<Vec<Node>> {
        let observations = crud::query_nodes(
            self.conn,
            &NodeQuery {
                node_type: Some("observation".to_string()),
                limit: Some(10000),
                ..Default::default()
            },
        )?;

        // Group observations by their shared source — either an EVIDENCES edge
        // target (created by `digest_catalyst`) or a MENTIONS edge target
        // (created by `tdg_observe` via `upsert_entity_and_connect`).
        //
        // The previous implementation ONLY looked at EVIDENCES edges. But
        // `tdg_observe` does NOT create EVIDENCES edges — it creates MENTIONS
        // edges. So for MCP-created observations, `by_source` was always empty
        // and no hypotheses were ever created. The digestion cascade was
        // effectively dead code for the entire MCP write path.
        //
        // We now group by BOTH edge types. When 3+ observations share a source
        // (via either edge type), a hypothesis is created.
        //
        // IMPORTANT: an observation can have BOTH a MENTIONS and an EVIDENCES
        // edge to the same source. The previous implementation would push the
        // same observation twice into by_source[source], inflating obs_list.len()
        // and creating duplicate parent_ids + duplicate SUPPORTS edges. We now
        // dedup by observation ID within each source group.
        let mut by_source: HashMap<String, Vec<&Node>> = HashMap::new();
        for obs in &observations {
            let mut seen_sources: std::collections::HashSet<String> = std::collections::HashSet::new();
            for edge_type in &["MENTIONS", "EVIDENCES"] {
                let edges =
                    crud::get_edges(self.conn, Some(&obs.id), None, Some(edge_type), None, 100)?;
                for e in &edges {
                    // Only push this obs once per source (dedup across edge types)
                    if seen_sources.insert(e.target_id.clone()) {
                        by_source
                            .entry(e.target_id.clone())
                            .or_default()
                            .push(obs);
                    }
                }
            }
        }

        let mut hypotheses = Vec::new();
        for (source_id, obs_list) in &by_source {
            if obs_list.len() >= self.min_similar_for_hypothesis {
                // Dedup: skip if a hypothesis already exists for this source.
                // The previous implementation created a NEW hypothesis on every
                // `tdg_observe` call once the threshold was met, leading to
                // N-2 duplicate hypotheses for N observations about the same entity.
                let existing: i64 = self.conn
                    .query_row(
                        "SELECT COUNT(*) FROM nodes
                         WHERE node_type = 'hypothesis'
                           AND source = 'digestion_cascade'
                           AND valid_to IS NULL
                           AND properties_json LIKE ?1",
                        rusqlite::params![format!("%\"source_node\": \"{}\"%", source_id)],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if existing > 0 {
                    tracing::debug!(
                        "digestion: hypothesis already exists for source {}, skipping",
                        source_id
                    );
                    continue;
                }

                let parent_ids: Vec<String> = obs_list.iter().map(|o| o.id.clone()).collect();
                let h = crud::add_node(
                    self.conn,
                    &NewNode {
                        node_type: "hypothesis".to_string(),
                        name: format!("Hypothesis from {} observations", obs_list.len()),
                        description: Some(format!(
                            "Pattern detected: {} observations mention entity {}",
                            obs_list.len(),
                            source_id
                        )),
                        properties: Some(serde_json::json!({
                            "observation_count": obs_list.len(),
                            "source_node": source_id,
                        })),
                        quadrants: None,
                        drives: None,
                        lifecycle_state: None,
                        teleological_level: Some("T3".to_string()),
                        developmental_stage: None,
                        confidence: Some(0.3),
                        source: Some("digestion_cascade".to_string()),
                        parent_ids: Some(parent_ids),
                        agent_id: None,
                        ..Default::default()
                    },
                )?;

                for obs in obs_list {
                    if let Err(e) = crud::add_edge(
                        self.conn,
                        &NewEdge {
                            source_id: h.id.clone(),
                            target_id: obs.id.clone(),
                            edge_type: "SUPPORTS".to_string(),
                            weight: None,
                            properties: None,
                            agent_id: Some("digestion".to_string()),
                        },
                    ) {
                        tracing::warn!(
                            "digestion: failed to create SUPPORTS edge {} -> {}: {}",
                            h.id, obs.id, e
                        );
                    }
                }

                hypotheses.push(h);
            }
        }

        Ok(hypotheses)
    }

    pub fn promote_hypothesis_to_capability(&self, hypothesis_id: &str) -> TdgResult<Node> {
        let hyp = crud::get_node(self.conn, hypothesis_id)?
            .ok_or_else(|| crate::error::TdgError::NotFound(hypothesis_id.to_string()))?;
        if hyp.node_type != "hypothesis" {
            return Err(crate::error::TdgError::Validation(
                "Node is not a hypothesis".to_string(),
            ));
        }

        let cap = crud::add_node(
            self.conn,
            &NewNode {
                node_type: "capability".to_string(),
                name: format!("Capability from: {}", hyp.name),
                description: Some(hyp.description.clone()),
                properties: Some(hyp.properties.clone()),
                quadrants: Some(hyp.quadrants.clone()),
                drives: Some(hyp.drives.clone()),
                lifecycle_state: None,
                teleological_level: Some("T3".to_string()),
                developmental_stage: None,
                confidence: Some(hyp.confidence),
                source: Some("digestion_promotion".to_string()),
                parent_ids: Some(vec![hypothesis_id.to_string()]),
                agent_id: None,
                ..Default::default()
            },
        )?;

        crud::add_edge(
            self.conn,
            &NewEdge {
                source_id: cap.id.clone(),
                target_id: hypothesis_id.to_string(),
                edge_type: "PROMOTES_TO".to_string(),
                weight: None,
                properties: None,
                agent_id: Some("digestion".to_string()),
            },
        )?;

        Ok(cap)
    }

    pub fn process_digestion_cycle(&self) -> TdgResult<Vec<DigestionEvent>> {
        let mut events = Vec::new();

        let observations = crud::query_nodes(
            self.conn,
            &NodeQuery {
                node_type: Some("observation".to_string()),
                limit: Some(10000),
                ..Default::default()
            },
        )?;
        let mut by_catalyst: HashMap<String, Vec<&Node>> = HashMap::new();
        for obs in &observations {
            if let Some(props) = obs.properties.as_object() {
                if let Some(ct) = props.get("catalyst_type").and_then(|v| v.as_str()) {
                    by_catalyst.entry(ct.to_string()).or_default().push(obs);
                }
            }
        }

        for (ct_str, obs_list) in &by_catalyst {
            if obs_list.len() >= self.min_similar_for_hypothesis {
                let parent_ids: Vec<String> = obs_list.iter().map(|o| o.id.clone()).collect();
                let h = crud::add_node(
                    self.conn,
                    &NewNode {
                        node_type: "hypothesis".to_string(),
                        name: format!("Hypothesis: {} {}s", obs_list.len(), ct_str),
                        description: Some(format!(
                            "Pattern: {} observations of type {}",
                            obs_list.len(),
                            ct_str
                        )),
                        properties: Some(serde_json::json!({
                            "catalyst_type": ct_str,
                            "observation_count": obs_list.len(),
                        })),
                        quadrants: None,
                        drives: None,
                        lifecycle_state: None,
                        teleological_level: Some("T3".to_string()),
                        developmental_stage: None,
                        confidence: Some(0.3),
                        source: Some("digestion_cycle".to_string()),
                        parent_ids: Some(parent_ids),
                        agent_id: None,
                        ..Default::default()
                    },
                )?;

                let mut new_edges = 0;
                for obs in obs_list {
                    crud::add_edge(
                        self.conn,
                        &NewEdge {
                            source_id: h.id.clone(),
                            target_id: obs.id.clone(),
                            edge_type: "SUPPORTS".to_string(),
                            weight: None,
                            properties: None,
                            agent_id: Some("digestion".to_string()),
                        },
                    )?;
                    new_edges += 1;
                }

                events.push(DigestionEvent {
                    event_id: uuid::Uuid::new_v4().to_string(),
                    catalyst_type: ct_str.clone(),
                    catalyst_source_node: String::new(),
                    catalyst_description: format!(
                        "Digestion cycle: {} observations → hypothesis",
                        obs_list.len()
                    ),
                    t_level_processed_at: "T3".to_string(),
                    stage_evidence_contributed: obs_list.len() as f64 * 0.5,
                    integration_quality: 0.5,
                    affected_telos_nodes: vec![h.id.clone()],
                    new_edges_created: new_edges,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                });
            }
        }

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::{init_schema, run_migrations};

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    fn create_source(conn: &Connection) -> Node {
        crud::add_node(
            conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "Source Action".to_string(),
                description: None,
                properties: None,
                quadrants: None,
                drives: None,
                lifecycle_state: None,
                teleological_level: None,
                developmental_stage: None,
                confidence: None,
                source: None,
                parent_ids: None,
                agent_id: None,
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn test_digest_catalyst() {
        let conn = setup_db();
        let engine = DigestionEngine::new(&conn);
        let source = create_source(&conn);

        let obs = engine
            .digest_catalyst(
                &source.id,
                &CatalystType::ExternalSuccess,
                "Something good happened",
            )
            .unwrap();

        assert_eq!(obs.node_type, "observation");
        assert_eq!(obs.teleological_level.as_deref(), Some("T4"));

        let edges =
            crud::get_edges(&conn, Some(&obs.id), None, Some("EVIDENCES"), None, 10).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target_id, source.id);
    }

    #[test]
    fn test_cascade_creates_hypothesis() {
        let conn = setup_db();
        let engine = DigestionEngine::new(&conn);
        let source = create_source(&conn);

        for i in 0..3 {
            engine
                .digest_catalyst(
                    &source.id,
                    &CatalystType::ExternalSuccess,
                    &format!("Observation {}", i),
                )
                .unwrap();
        }

        let hypotheses = engine.check_upward_cascade().unwrap();
        assert_eq!(hypotheses.len(), 1);
        assert_eq!(hypotheses[0].node_type, "hypothesis");
        assert_eq!(hypotheses[0].teleological_level.as_deref(), Some("T3"));
    }

    #[test]
    fn test_promote_hypothesis() {
        let conn = setup_db();
        let engine = DigestionEngine::new(&conn);
        let source = create_source(&conn);

        for i in 0..3 {
            engine
                .digest_catalyst(
                    &source.id,
                    &CatalystType::ExternalSuccess,
                    &format!("Obs {}", i),
                )
                .unwrap();
        }
        let hypotheses = engine.check_upward_cascade().unwrap();
        let cap = engine
            .promote_hypothesis_to_capability(&hypotheses[0].id)
            .unwrap();

        assert_eq!(cap.node_type, "capability");
        let edges =
            crud::get_edges(&conn, Some(&cap.id), None, Some("PROMOTES_TO"), None, 10).unwrap();
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn test_digestion_cycle() {
        let conn = setup_db();
        let engine = DigestionEngine::new(&conn);
        let source = create_source(&conn);

        for i in 0..5 {
            engine
                .digest_catalyst(
                    &source.id,
                    &CatalystType::InternalDiscovery,
                    &format!("Discovery {}", i),
                )
                .unwrap();
        }

        let events = engine.process_digestion_cycle().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].catalyst_type, "internal_discovery");
        assert_eq!(events[0].new_edges_created, 5);
    }
}
