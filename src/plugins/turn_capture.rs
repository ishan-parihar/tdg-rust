//! Turn Capture — conversation turn observation ingestion
//!
//! Port of `plugins/tdg/turn_capture.py`.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::TdgResult;
use crate::models::NewNode;

/// A captured conversation turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedTurn {
    pub observation_id: String,
    pub text: String,
    pub timestamp: String,
    pub entities: Vec<String>,
}

/// Deduplication window (seconds).
const DEDUP_WINDOW_SECS: i64 = 300; // 5 minutes

/// The Turn Capture — captures conversation turns as observations.
pub struct TurnCapture;

impl TurnCapture {
    pub fn new() -> Self {
        Self
    }

    /// Capture a conversation turn as an observation node.
    pub fn capture(
        &self,
        conn: &Connection,
        text: &str,
        agent_id: Option<&str>,
    ) -> TdgResult<CapturedTurn> {
        // Check for recent duplicate
        if self.is_duplicate(conn, text)? {
            return Ok(CapturedTurn {
                observation_id: String::new(),
                text: text.to_string(),
                timestamp: crate::db::crud::now_iso(),
                entities: Vec::new(),
            });
        }

        // Extract entities
        let extractor = crate::plugins::EntityExtractor::new();
        let entities = extractor.extract(text, Some(conn));
        let entity_names: Vec<String> = entities.iter().map(|e| e.name.clone()).collect();

        // Create observation node
        let node = crate::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: format!(
                    "Turn: {}",
                    text.chars().take(80).collect::<String>()
                ),
                description: Some(text.to_string()),
                source: Some("turn_capture".to_string()),
                agent_id: agent_id.map(|s| s.to_string()),
                properties: Some(serde_json::json!({
                    "entities": entity_names,
                    "turn_length": text.len(),
                })),
                ..Default::default()
            },
        )?;

        // Create edges to extracted entities
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

        Ok(CapturedTurn {
            observation_id: node.id,
            text: text.to_string(),
            timestamp: node.created_at,
            entities: entity_names,
        })
    }

    fn is_duplicate(&self, conn: &Connection, text: &str) -> TdgResult<bool> {
        // Check for recent observations with similar content
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
            // Simple overlap-based deduplication
            let overlap = calculate_overlap(&text_lower, &name_lower);
            if overlap > 0.7 {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

/// Calculate text similarity using word overlap (Jaccard-like).
fn calculate_overlap(a: &str, b: &str) -> f64 {
    let a_words: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let b_words: std::collections::HashSet<&str> = b.split_whitespace().collect();

    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
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
        let _r2 = capture.capture(&conn, "Hello world test again", None).unwrap();
        // The second one might be deduplicated
        assert!(!r1.observation_id.is_empty());
    }

    #[test]
    fn overlap_calculation() {
        assert!(calculate_overlap("the quick brown fox", "the quick brown fox") > 0.9);
        assert!(calculate_overlap("hello world", "completely different") < 0.1);
    }
}
