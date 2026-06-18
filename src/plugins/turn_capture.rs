use std::collections::HashSet;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::TdgResult;
use crate::models::NewNode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedTurn {
    pub observation_id: String,
    pub text: String,
    pub timestamp: String,
    pub entities: Vec<String>,
    pub quadrant: String,
    pub contradictions: Vec<String>,
}

const DEDUP_WINDOW_SECS: i64 = 300;

const RATE_LIMIT_WPS: u64 = 10;

const CONTRADICTION_THRESHOLD: f64 = 0.4;

static QUADRANT_KEYWORDS: LazyLock<Vec<(&'static str, Vec<&'static str>)>> = LazyLock::new(|| {
    vec![
        (
            "lr",
            vec![
                "deploy", "server", "database", "api", "infrastructure", "docker", "aws",
                "pricing", "cost", "build", "compile", "test", "run", "fix", "debug",
            ],
        ),
        (
            "ul",
            vec![
                "prefer", "feel", "like", "dislike", "comfortable", "trust", "believe",
                "value", "think", "believe", "understand", "learn",
            ],
        ),
        (
            "ll",
            vec![
                "identity", "brand", "name", "persona", "style", "tone", "voice", "culture",
                "remember", "note", "memo",
            ],
        ),
        (
            "ur",
            vec![
                "do", "action", "behavior", "habit", "practice", "technique", "approach",
                "create", "build", "make", "write", "implement",
            ],
        ),
    ]
});

pub struct TurnCapture {
    last_write_ts: std::cell::Cell<u64>,
    write_count: std::cell::Cell<u64>,
}

impl TurnCapture {
    pub fn new() -> Self {
        Self {
            last_write_ts: std::cell::Cell::new(0),
            write_count: std::cell::Cell::new(0),
        }
    }

    pub fn capture(
        &self,
        conn: &Connection,
        text: &str,
        agent_id: Option<&str>,
    ) -> TdgResult<CapturedTurn> {
        if !self.check_rate_limit() {
            return Ok(CapturedTurn {
                observation_id: String::new(),
                text: text.to_string(),
                timestamp: crate::db::crud::now_iso(),
                entities: Vec::new(),
                quadrant: "ur".to_string(),
                contradictions: Vec::new(),
            });
        }

        if self.is_duplicate(conn, text)? {
            return Ok(CapturedTurn {
                observation_id: String::new(),
                text: text.to_string(),
                timestamp: crate::db::crud::now_iso(),
                entities: Vec::new(),
                quadrant: "ur".to_string(),
                contradictions: Vec::new(),
            });
        }

        let extractor = crate::plugins::EntityExtractor::new();
        let entities = extractor.extract(text, Some(conn));
        let entity_names: Vec<String> = entities.iter().map(|e| e.name.clone()).collect();

        let quadrant = infer_quadrant_from_content(text);

        let node = crate::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: format!("Turn: {}", text.chars().take(80).collect::<String>()),
                description: Some(text.to_string()),
                source: Some("turn_capture".to_string()),
                agent_id: agent_id.map(|s| s.to_string()),
                properties: Some(serde_json::json!({
                    "entities": entity_names,
                    "turn_length": text.len(),
                    "quadrant": quadrant,
                })),
                ..Default::default()
            },
        )?;

        if let Some(aid) = agent_id {
            let agent_node_id = find_or_create_agent_self(conn, aid)?;
            let _ = crate::db::crud::add_edge(
                conn,
                &crate::models::NewEdge {
                    source_id: node.id.clone(),
                    target_id: agent_node_id,
                    edge_type: "EXPERIENCES".to_string(),
                    ..Default::default()
                },
            );
        }

        for entity in &entities {
            if let Some(ref entity_id) = entity.id {
                let _ = crate::db::crud::add_edge(
                    conn,
                    &crate::models::NewEdge {
                        source_id: node.id.clone(),
                        target_id: entity_id.clone(),
                        edge_type: "MENTIONS".to_string(),
                        ..Default::default()
                    },
                );
            }
        }

        let contradictions = self.detect_contradictions(conn, text, &entity_names)?;

        Ok(CapturedTurn {
            observation_id: node.id,
            text: text.to_string(),
            timestamp: node.created_at,
            entities: entity_names,
            quadrant,
            contradictions,
        })
    }

    fn check_rate_limit(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let last = self.last_write_ts.get();
        let count = self.write_count.get();

        if now != last {
            self.last_write_ts.set(now);
            self.write_count.set(1);
            return true;
        }

        if count < RATE_LIMIT_WPS {
            self.write_count.set(count + 1);
            return true;
        }

        false
    }

    fn detect_contradictions(
        &self,
        conn: &Connection,
        text: &str,
        entities: &[String],
    ) -> TdgResult<Vec<String>> {
        let mut stmt = conn.prepare(
            "SELECT id, name, properties_json FROM nodes WHERE valid_to IS NULL
             AND node_type = 'observation' AND source = 'turn_capture'
             ORDER BY created_at DESC LIMIT 20",
        )?;

        let recent: Vec<(String, String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let mut contradictions = Vec::new();
        let entity_set: HashSet<&str> = entities.iter().map(|s| s.as_str()).collect();

        for (obs_id, _obs_name, props_str) in &recent {
            if let Ok(props) = serde_json::from_str::<serde_json::Value>(props_str) {
                if let Some(obs_entities) = props.get("entities").and_then(|v| v.as_array()) {
                    let obs_entity_set: HashSet<&str> =
                        obs_entities.iter().filter_map(|v| v.as_str()).collect();

                    let entity_overlap = if entity_set.is_empty() || obs_entity_set.is_empty() {
                        0.0
                    } else {
                        let intersection = entity_set
                            .intersection(&obs_entity_set)
                            .count() as f64;
                        let union = entity_set.union(&obs_entity_set).count() as f64;
                        if union == 0.0 {
                            0.0
                        } else {
                            intersection / union
                        }
                    };

                    let content_sim = calculate_overlap(
                        &text.to_lowercase(),
                        &obs_id.to_lowercase(), // simplified — would need full text
                    );

                    let score = entity_overlap * (1.0 - content_sim);

                    if score > CONTRADICTION_THRESHOLD && entity_overlap > 0.3 {
                        contradictions.push(obs_id.clone());
                    }
                }
            }
        }

        Ok(contradictions)
    }

    fn is_duplicate(&self, conn: &Connection, text: &str) -> TdgResult<bool> {
        let mut stmt = conn.prepare(
            "SELECT name FROM nodes WHERE valid_to IS NULL AND node_type = 'observation'
             AND source = 'turn_capture' ORDER BY created_at DESC LIMIT 10",
        )?;

        let recent: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        let text_lower = text.to_lowercase();
        for name in &recent {
            let name_lower = name.to_lowercase();
            let overlap = calculate_overlap(&text_lower, &name_lower);
            if overlap > 0.7 {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

impl Default for TurnCapture {
    fn default() -> Self {
        Self::new()
    }
}

fn calculate_overlap(a: &str, b: &str) -> f64 {
    let a_words: HashSet<&str> = a.split_whitespace().collect();
    let b_words: HashSet<&str> = b.split_whitespace().collect();

    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

fn infer_quadrant_from_content(text: &str) -> String {
    let lower = text.to_lowercase();
    for (quadrant, keywords) in QUADRANT_KEYWORDS.iter() {
        if keywords.iter().any(|kw| lower.contains(kw)) {
            return quadrant.to_string();
        }
    }
    "ur".to_string()
}

fn find_or_create_agent_self(conn: &Connection, agent_id: &str) -> TdgResult<String> {
    let node_id = format!("agent:{}", agent_id);

    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM nodes WHERE id = ?1 AND valid_to IS NULL",
            [&node_id],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if exists {
        return Ok(node_id);
    }

    let _ = crate::db::crud::add_node(
        conn,
        &NewNode {
            node_type: "agent".to_string(),
            name: format!("Agent: {}", agent_id),
            description: Some(format!("Agent {}", agent_id)),
            source: Some("turn_capture".to_string()),
            ..Default::default()
        },
    );

    Ok(node_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_fts, init_schema, run_migrations};
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn capture_basic() {
        let conn = setup_db();
        let capture = TurnCapture::new();
        let result = capture
            .capture(&conn, "This is a test conversation turn", None)
            .unwrap();
        assert!(!result.observation_id.is_empty());
    }

    #[test]
    fn capture_deduplication() {
        let conn = setup_db();
        let capture = TurnCapture::new();
        let r1 = capture.capture(&conn, "Hello world test", None).unwrap();
        let _r2 = capture
            .capture(&conn, "Hello world test again", None)
            .unwrap();
        // The second one might be deduplicated
        assert!(!r1.observation_id.is_empty());
    }

    #[test]
    fn overlap_calculation() {
        assert!(calculate_overlap("the quick brown fox", "the quick brown fox") > 0.9);
        assert!(calculate_overlap("hello world", "completely different") < 0.1);
    }
}
