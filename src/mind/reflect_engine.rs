use crate::db::crud;
use crate::error::TdgResult;
use crate::models::NewEdge;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReflectConfig {
    min_observations_to_run: usize,
    min_cluster_size: usize,
    min_shared_entities: usize,
    stale_observation_days: u32,
    max_clusters_per_run: usize,
}

impl Default for ReflectConfig {
    fn default() -> Self {
        Self {
            min_observations_to_run: 5,
            min_cluster_size: 3,
            min_shared_entities: 2,
            stale_observation_days: 7,
            max_clusters_per_run: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReflectResult {
    pub skipped: bool,
    pub skip_reason: String,
    pub observations_analyzed: usize,
    pub clusters_found: usize,
    pub clusters_processed: usize,
    pub skills_created: usize,
    pub discoveries_created: usize,
    pub observations_archived: usize,
    pub errors: usize,
}

pub struct ReflectEngine<'a> {
    conn: &'a Connection,
    config: ReflectConfig,
}

impl<'a> ReflectEngine<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self {
            conn,
            config: ReflectConfig::default(),
        }
    }

    pub fn run(&self) -> TdgResult<ReflectResult> {
        let mut result = ReflectResult::default();

        let cutoff =
            chrono::Utc::now() - chrono::Duration::days(self.config.stale_observation_days as i64);
        let cutoff_ts = cutoff.to_rfc3339();

        let observations: Vec<(String, String, String)> = {
            let mut stmt = self.conn.prepare(
                "SELECT id, name, description FROM nodes
                 WHERE node_type = 'observation'
                   AND lifecycle_state = 'active'
                   AND valid_to IS NULL
                 AND created_at >= ?1
                 ORDER BY created_at DESC
                 LIMIT 100",
            )?;
            let rows = stmt.query_map(rusqlite::params![cutoff_ts], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
            rows.filter_map(|r| r.ok()).collect()
        };

        result.observations_analyzed = observations.len();

        if observations.len() < self.config.min_observations_to_run {
            result.skipped = true;
            result.skip_reason = format!(
                "only {} recent observations (need {})",
                observations.len(),
                self.config.min_observations_to_run
            );
            return Ok(result);
        }

        let obs_ids: Vec<String> = observations.iter().map(|(id, _, _)| id.clone()).collect();

        let mut obs_entities: std::collections::HashMap<String, std::collections::HashSet<String>> =
            std::collections::HashMap::new();
        for oid in &obs_ids {
            let entities: Vec<String> = {
                let mut stmt = self.conn.prepare(
                    "SELECT DISTINCT target_id FROM edges
                     WHERE source_id = ?1 AND edge_type = 'MENTIONS' AND valid_to IS NULL",
                )?;
                let rows = stmt.query_map(rusqlite::params![oid], |row| row.get(0))?;
                rows.filter_map(|r| r.ok()).collect()
            };
            if !entities.is_empty() {
                obs_entities.insert(oid.clone(), entities.into_iter().collect());
            }
        }

        let obs_list: Vec<&str> = obs_entities.keys().map(|s| s.as_str()).collect();
        let mut clusters: Vec<Vec<String>> = Vec::new();
        let mut assigned = std::collections::HashSet::new();

        for i in 0..obs_list.len() {
            if assigned.contains(obs_list[i]) {
                continue;
            }
            let mut cluster = vec![obs_list[i].to_string()];
            assigned.insert(obs_list[i].to_string());

            for j in (i + 1)..obs_list.len() {
                if assigned.contains(obs_list[j]) {
                    continue;
                }
                if let (Some(set_a), Some(set_b)) =
                    (obs_entities.get(obs_list[i]), obs_entities.get(obs_list[j]))
                {
                    let shared: usize = set_a.intersection(set_b).count();
                    if shared >= self.config.min_shared_entities {
                        cluster.push(obs_list[j].to_string());
                        assigned.insert(obs_list[j].to_string());
                    }
                }
            }

            if cluster.len() >= self.config.min_cluster_size {
                clusters.push(cluster);
            }
        }

        result.clusters_found = clusters.len();
        let clusters_to_process = clusters.into_iter().take(self.config.max_clusters_per_run);

        for cluster in clusters_to_process {
            match self.process_cluster(&cluster) {
                Ok(detail) => {
                    result.clusters_processed += 1;
                    if detail.skill_created {
                        result.skills_created += 1;
                    }
                    if detail.discovery_created {
                        result.discoveries_created += 1;
                    }
                }
                Err(_) => {
                    result.errors += 1;
                }
            }
        }

        let now = chrono::Utc::now().to_rfc3339();
        let archived = self.conn.execute(
            "UPDATE nodes SET lifecycle_state = 'archived', updated_at = ?1
             WHERE node_type = 'observation'
               AND lifecycle_state = 'active'
               AND valid_to IS NULL
               AND created_at < ?2
               AND id NOT IN (
                   SELECT DISTINCT source_id FROM edges WHERE edge_type = 'MENTIONS' AND valid_to IS NULL
               )",
            rusqlite::params![now, cutoff_ts],
        )?;
        result.observations_archived = archived;

        Ok(result)
    }

    fn process_cluster(&self, cluster: &[String]) -> TdgResult<ClusterDetail> {
        let mut detail = ClusterDetail::default();

        if cluster.len() < self.config.min_cluster_size {
            return Ok(detail);
        }

        let mut entity_set = std::collections::HashSet::new();
        let mut contents = Vec::new();

        for oid in cluster {
            let node: Option<(String, String, String)> = self
                .conn
                .query_row(
                    "SELECT name, description, properties_json FROM nodes WHERE id = ?1 AND valid_to IS NULL",
                    rusqlite::params![oid],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .ok();

            if let Some((name, desc, _props)) = node {
                contents.push(format!("{} {}", name, desc));
            }

            let entities: Vec<String> = {
                let mut stmt = self.conn.prepare(
                    "SELECT DISTINCT target_id FROM edges WHERE source_id = ?1 AND edge_type = 'MENTIONS' AND valid_to IS NULL"
                )?;
                let rows = stmt.query_map(rusqlite::params![oid], |row| row.get(0))?;
                rows.filter_map(|r| r.ok()).collect()
            };
            for e in entities {
                entity_set.insert(e);
            }
        }

        if contents.len() < self.config.min_cluster_size {
            return Ok(detail);
        }

        let mut fingerprint_parts: Vec<String> = entity_set.into_iter().collect();
        fingerprint_parts.sort();

        let fingerprint = {
            let mut hasher = Sha256::new();
            hasher.update(fingerprint_parts.join("|"));
            let result = hasher.finalize();
            format!("{:x}", result)[..16].to_string()
        };
        let skill_id = format!("skill:reflect_{}", fingerprint);

        let exists: bool = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE id = ?1 AND valid_to IS NULL",
                rusqlite::params![skill_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if !exists {
            let tag_label = if fingerprint_parts.len() >= 3 {
                fingerprint_parts[..3].join("_")
            } else {
                format!("cluster_{}", &fingerprint[..8])
            };

            let description = format!(
                "Auto-discovered pattern from {} observations. Created by reflect_engine.",
                cluster.len()
            );

            let properties = serde_json::json!({
                "source_observations": cluster.iter().take(10).cloned().collect::<Vec<_>>(),
                "fingerprint": fingerprint,
                "version": 1,
            });

            let now = chrono::Utc::now().to_rfc3339();
            self.conn.execute(
                "INSERT INTO nodes (id, node_type, name, description, properties_json, lifecycle_state, confidence, created_at, updated_at, source)
                 VALUES (?1, 'skill', ?2, ?3, ?4, 'active', 0.7, ?5, ?5, 'reflect_engine')",
                rusqlite::params![
                    skill_id,
                    format!("Pattern: {}", tag_label),
                    description,
                    properties.to_string(),
                    now,
                ],
            )?;

            detail.skill_created = true;

            for oid in cluster {
                let _ = crud::add_edge(
                    self.conn,
                    &NewEdge {
                        source_id: oid.clone(),
                        target_id: skill_id.clone(),
                        edge_type: "ENABLES".to_string(),
                        weight: Some(0.5),
                        properties: None,
                        agent_id: Some("reflect_engine".to_string()),
                    },
                );
            }
        }

        Ok(detail)
    }
}

#[derive(Debug, Default)]
struct ClusterDetail {
    skill_created: bool,
    discovery_created: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::{init_schema, run_migrations};
    use crate::models::NewNode;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    fn create_obs(conn: &Connection, name: &str) -> String {
        let node = crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: name.to_string(),
                description: Some(format!("Description of {}", name)),
                properties: None,
                quadrants: None,
                drives: None,
                lifecycle_state: Some("active".to_string()),
                teleological_level: None,
                developmental_stage: None,
                confidence: None,
                source: None,
                parent_ids: None,
                agent_id: None,
            },
        )
        .unwrap();
        node.id
    }

    fn create_entity(conn: &Connection, name: &str) -> String {
        let node = crud::add_node(
            conn,
            &NewNode {
                node_type: "people".to_string(),
                name: name.to_string(),
                description: None,
                properties: None,
                quadrants: None,
                drives: None,
                lifecycle_state: Some("active".to_string()),
                teleological_level: None,
                developmental_stage: None,
                confidence: None,
                source: None,
                parent_ids: None,
                agent_id: None,
            },
        )
        .unwrap();
        node.id
    }

    fn mention(conn: &Connection, obs_id: &str, entity_id: &str) {
        crud::add_edge(
            conn,
            &NewEdge {
                source_id: obs_id.to_string(),
                target_id: entity_id.to_string(),
                edge_type: "MENTIONS".to_string(),
                weight: None,
                properties: None,
                agent_id: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn test_reflect_skips_when_few_observations() {
        let conn = setup_db();
        let engine = ReflectEngine::new(&conn);
        let result = engine.run().unwrap();
        assert!(result.skipped);
        assert!(result.skip_reason.contains("only 0"));
    }

    #[test]
    fn test_reflect_clusters_by_entity_overlap() {
        let conn = setup_db();
        let e1 = create_entity(&conn, "Entity1");
        let e2 = create_entity(&conn, "Entity2");

        let mut obs_ids = Vec::new();
        for i in 0..5 {
            let id = create_obs(&conn, &format!("Obs {}", i));
            mention(&conn, &id, &e1);
            mention(&conn, &id, &e2);
            obs_ids.push(id);
        }

        let engine = ReflectEngine::new(&conn);
        let result = engine.run().unwrap();
        assert!(!result.skipped);
        assert_eq!(result.observations_analyzed, 5);
        assert!(result.clusters_found >= 1);
    }

    #[test]
    fn test_reflect_creates_skill_node() {
        let conn = setup_db();
        let e1 = create_entity(&conn, "Entity1");
        let e2 = create_entity(&conn, "Entity2");

        for i in 0..5 {
            let id = create_obs(&conn, &format!("Obs {}", i));
            mention(&conn, &id, &e1);
            mention(&conn, &id, &e2);
        }

        let engine = ReflectEngine::new(&conn);
        let result = engine.run().unwrap();
        assert!(result.skills_created >= 1);

        let skills = crate::db::crud::query_nodes(
            &conn,
            &crate::models::NodeQuery {
                node_type: Some("skill".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(skills.iter().any(|s| s.source == "reflect_engine"));
    }

    #[test]
    fn test_reflect_idempotent() {
        let conn = setup_db();
        let e1 = create_entity(&conn, "Entity1");
        let e2 = create_entity(&conn, "Entity2");

        for i in 0..5 {
            let id = create_obs(&conn, &format!("Obs {}", i));
            mention(&conn, &id, &e1);
            mention(&conn, &id, &e2);
        }

        let engine = ReflectEngine::new(&conn);
        let _r1 = engine.run().unwrap();
        let _r2 = engine.run().unwrap();

        let skills = crate::db::crud::query_nodes(
            &conn,
            &crate::models::NodeQuery {
                node_type: Some("skill".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let reflect_skills: Vec<_> = skills
            .iter()
            .filter(|s| s.source == "reflect_engine")
            .collect();
        assert_eq!(
            reflect_skills.len(),
            1,
            "Should be idempotent - only 1 skill created"
        );
    }
}
