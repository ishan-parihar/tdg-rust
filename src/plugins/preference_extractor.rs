use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::crud;
use crate::models::NodeQuery;
use crate::util::quadrants::infer_quadrant as infer_quadrant_from_text;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferenceExtraction {
    pub extraction_type: String,
    pub constraint_text: String,
    pub confidence: f64,
    pub quadrant: String,
    pub constraint_id: String,
}

static CORRECTION_PATTERNS: LazyLock<Vec<(&'static str, f64)>> = LazyLock::new(|| {
    vec![
        (r"(?i)don't\s+(do|use|try|go)\s+(.+)", 0.85),
        (r"(?i)stop\s+(doing|using|saying)\s+(.+)", 0.85),
        (r"(?i)never\s+(do|use|say|go)\s+(.+)", 0.9),
        (r"(?i)avoid\s+(.+)", 0.8),
        (r"(?i)no\s+more\s+(.+)", 0.8),
    ]
});

static PREFERENCE_PATTERNS: LazyLock<Vec<(&'static str, f64)>> = LazyLock::new(|| {
    vec![
        (r"(?i)I\s+prefer\s+(.+)", 0.85),
        (r"(?i)always\s+(do|use|say|go)\s+(.+)", 0.8),
        (r"(?i)please\s+(do|use|say)\s+(.+)", 0.75),
        (r"(?i)I\s+like\s+when\s+(.+)", 0.8),
        (r"(?i)keep\s+(doing|using|saying)\s+(.+)", 0.8),
    ]
});

static MEMORY_PATTERNS: LazyLock<Vec<(&'static str, f64)>> = LazyLock::new(|| {
    vec![
        (r"(?i)(remember|note|memo|recall)\s+(?:that\s+)?(.+)", 0.9),
        (r"(?i)don't\s+forget\s+(?:that\s+)?(.+)", 0.9),
        (r"(?i)make\s+a\s+note\s+(?:that\s+)?(.+)", 0.85),
    ]
});

static RECURRING_PATTERNS: LazyLock<Vec<(&'static str, f64)>> = LazyLock::new(|| {
    vec![
        (r"(?i)(?:every|each)\s+time\s+(.+)", 0.8),
        (r"(?i)(?:always|consistently)\s+(.+)", 0.75),
        (r"(?i)(?:I\s+notice|I've\s+noticed)\s+(?:that\s+)?(.+)", 0.7),
        (r"(?i)(?:pattern|trend|habit)[:\s]+(.+)", 0.8),
    ]
});

static AUTONOMOUS_PATTERNS: LazyLock<Vec<(&'static str, f64)>> = LazyLock::new(|| {
    vec![
        (
            r"(?i)(?:based\s+on|from)\s+(?:my\s+)?(?:observations?|history|data)\s*,?\s*(.+)",
            0.7,
        ),
        (
            r"(?i)(?:the\s+)?(?:data|evidence)\s+(?:suggests?|shows?|indicates?)\s+(.+)",
            0.75,
        ),
        (
            r"(?i)(?:I've\s+inferred|deduced|concluded)\s+(?:that\s+)?(.+)",
            0.8,
        ),
        (
            r"(?i)(?:the\s+)?(?:pattern|signal)\s+(?:is|shows?|suggests?)\s+(.+)",
            0.7,
        ),
    ]
});

const RECURRING_MIN_OCCURRENCES: usize = 3;

const CROSS_CYCLE_VERBS: &[(&str, &str)] = &[
    (r"(?i)\b(?:set|configure|establish)\b", "establish"),
    (r"(?i)\b(?:create|build|generate)\b", "create"),
    (r"(?i)\b(?:update|modify|change|revise)\b", "modify"),
    (r"(?i)\b(?:test|verify|validate)\b", "validate"),
    (r"(?i)\b(?:deploy|ship|release|launch)\b", "deploy"),
    (r"(?i)\b(?:fix|repair|resolve)\b", "resolve"),
    (r"(?i)\b(?:optimize|improve|enhance)\b", "optimize"),
];

pub struct PreferenceExtractor;

impl PreferenceExtractor {
    pub fn new() -> Self {
        Self
    }

    pub fn extract_from_message(&self, text: &str) -> Vec<PreferenceExtraction> {
        let mut results = Vec::new();

        for (pattern, confidence) in MEMORY_PATTERNS.iter() {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(caps) = re.captures(text) {
                    if let Some(content) = caps.get(2).or_else(|| caps.get(1)) {
                        let constraint_text = content.as_str().trim().to_string();
                        let quadrant = infer_quadrant_from_text(&constraint_text);
                        let constraint_id = build_constraint_id("memory", &constraint_text);
                        results.push(PreferenceExtraction {
                            extraction_type: "memory".to_string(),
                            constraint_text,
                            confidence: *confidence,
                            quadrant,
                            constraint_id,
                        });
                    }
                }
            }
        }

        for (pattern, confidence) in CORRECTION_PATTERNS.iter() {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(caps) = re.captures(text) {
                    if let Some(content) = caps.get(2).or_else(|| caps.get(1)) {
                        let constraint_text = content.as_str().trim().to_string();
                        let quadrant = infer_quadrant_from_text(&constraint_text);
                        let constraint_id = build_constraint_id("correction", &constraint_text);
                        results.push(PreferenceExtraction {
                            extraction_type: "correction".to_string(),
                            constraint_text,
                            confidence: *confidence,
                            quadrant,
                            constraint_id,
                        });
                    }
                }
            }
        }

        for (pattern, confidence) in PREFERENCE_PATTERNS.iter() {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(caps) = re.captures(text) {
                    if let Some(content) = caps.get(2).or_else(|| caps.get(1)) {
                        let constraint_text = content.as_str().trim().to_string();
                        let quadrant = infer_quadrant_from_text(&constraint_text);
                        let constraint_id = build_constraint_id("preference", &constraint_text);
                        results.push(PreferenceExtraction {
                            extraction_type: "preference".to_string(),
                            constraint_text,
                            confidence: *confidence,
                            quadrant,
                            constraint_id,
                        });
                    }
                }
            }
        }

        for (pattern, confidence) in RECURRING_PATTERNS.iter() {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(caps) = re.captures(text) {
                    if let Some(content) = caps.get(1) {
                        let constraint_text = content.as_str().trim().to_string();
                        let quadrant = infer_quadrant_from_text(&constraint_text);
                        let constraint_id =
                            build_constraint_id("recurring_pattern", &constraint_text);
                        results.push(PreferenceExtraction {
                            extraction_type: "recurring_pattern".to_string(),
                            constraint_text,
                            confidence: *confidence,
                            quadrant,
                            constraint_id,
                        });
                    }
                }
            }
        }

        for (pattern, confidence) in AUTONOMOUS_PATTERNS.iter() {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(caps) = re.captures(text) {
                    if let Some(content) = caps.get(1) {
                        let constraint_text = content.as_str().trim().to_string();
                        let quadrant = infer_quadrant_from_text(&constraint_text);
                        let constraint_id =
                            build_constraint_id("autonomous_insight", &constraint_text);
                        results.push(PreferenceExtraction {
                            extraction_type: "autonomous_insight".to_string(),
                            constraint_text,
                            confidence: *confidence,
                            quadrant,
                            constraint_id,
                        });
                    }
                }
            }
        }

        let mut seen = std::collections::HashSet::new();
        results.retain(|r| seen.insert(r.constraint_id.clone()));

        results
    }

    pub fn extract_from_messages(&self, messages: &[String]) -> Vec<PreferenceExtraction> {
        let mut all_results = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for message in messages {
            let extractions = self.extract_from_message(message);
            for ext in extractions {
                if seen_ids.insert(ext.constraint_id.clone()) {
                    all_results.push(ext);
                }
            }
        }

        all_results
    }

    pub fn detect_recurring_patterns(
        &self,
        conn: &Connection,
        lookback_limit: usize,
    ) -> Vec<PreferenceExtraction> {
        let query = NodeQuery {
            node_type: Some("observation".to_string()),
            limit: Some(lookback_limit as i64),
            ..Default::default()
        };

        let observations = match crud::query_nodes(conn, &query) {
            Ok(nodes) => nodes,
            Err(_) => return vec![],
        };

        let mut keyword_counts: HashMap<String, Vec<String>> = HashMap::new();

        for obs in &observations {
            let text = obs
                .properties
                .get("description")
                .or_else(|| obs.properties.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let lower = text.to_lowercase();

            for (_quadrant, keywords) in crate::util::quadrants::QUADRANT_KEYWORDS.iter() {
                for kw in *keywords {
                    if lower.contains(kw) {
                        keyword_counts
                            .entry(kw.to_string())
                            .or_default()
                            .push(obs.id.clone());
                    }
                }
            }
        }

        let mut results = Vec::new();

        for (keyword, obs_ids) in &keyword_counts {
            if obs_ids.len() >= RECURRING_MIN_OCCURRENCES {
                let unique_obs: Vec<&String> = obs_ids.iter().collect();
                if unique_obs.len() >= RECURRING_MIN_OCCURRENCES {
                    let text = format!(
                        "Recurring pattern detected: '{}' appears {} times in observations",
                        keyword,
                        unique_obs.len()
                    );
                    let quadrant = infer_quadrant_from_text(keyword);
                    let constraint_id = build_constraint_id("recurring_pattern", &text);
                    results.push(PreferenceExtraction {
                        extraction_type: "recurring_pattern".to_string(),
                        constraint_text: text,
                        confidence: 0.8,
                        quadrant,
                        constraint_id,
                    });
                }
            }
        }

        results
    }

    pub fn detect_cross_cycle_patterns(
        &self,
        conn: &Connection,
        lookback_limit: usize,
    ) -> Vec<PreferenceExtraction> {
        let query = NodeQuery {
            node_type: Some("observation".to_string()),
            limit: Some(lookback_limit as i64),
            ..Default::default()
        };

        let observations = match crud::query_nodes(conn, &query) {
            Ok(nodes) => nodes,
            Err(_) => return vec![],
        };

        let mut results = Vec::new();

        for obs in &observations {
            let text = obs
                .properties
                .get("description")
                .or_else(|| obs.properties.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            for (verb_pattern, verb_label) in CROSS_CYCLE_VERBS {
                if let Ok(re) = regex::Regex::new(verb_pattern) {
                    if re.is_match(text) {
                        let constraint_text =
                            format!("Cross-cycle action detected: {} in: {}", verb_label, text);
                        let quadrant = infer_quadrant_from_text(text);
                        let constraint_id =
                            build_constraint_id("autonomous_insight", &constraint_text);
                        results.push(PreferenceExtraction {
                            extraction_type: "autonomous_insight".to_string(),
                            constraint_text,
                            confidence: 0.7,
                            quadrant,
                            constraint_id,
                        });
                        break;
                    }
                }
            }
        }

        let mut seen = std::collections::HashSet::new();
        results.retain(|r| seen.insert(r.constraint_id.clone()));

        results
    }
}

impl Default for PreferenceExtractor {
    fn default() -> Self {
        Self::new()
    }
}

pub fn build_constraint_id(extraction_type: &str, constraint_text: &str) -> String {
    let mut hasher = DefaultHasher::new();
    extraction_type.hash(&mut hasher);
    constraint_text.to_lowercase().trim().hash(&mut hasher);
    format!("c{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_correction() {
        let ext = PreferenceExtractor::new();
        let results = ext.extract_from_message("Don't use Docker for this project");
        assert!(!results.is_empty());
        assert_eq!(results[0].extraction_type, "correction");
        assert!(!results[0].constraint_id.is_empty());
    }

    #[test]
    fn extract_preference() {
        let ext = PreferenceExtractor::new();
        let results = ext.extract_from_message("I prefer using Rust for backend");
        assert!(!results.is_empty());
        assert_eq!(results[0].extraction_type, "preference");
        assert!(!results[0].constraint_id.is_empty());
    }

    #[test]
    fn extract_memory_request() {
        let ext = PreferenceExtractor::new();
        let results = ext.extract_from_message("Remember that the server is on port 3000");
        assert!(!results.is_empty());
        assert_eq!(results[0].extraction_type, "memory");
        assert!(!results[0].constraint_id.is_empty());
    }

    #[test]
    fn no_extraction() {
        let ext = PreferenceExtractor::new();
        let results = ext.extract_from_message("The weather is nice today");
        assert!(results.is_empty());
    }

    #[test]
    fn quadrant_inference() {
        assert_eq!(infer_quadrant_from_text("deploy the server"), "lr");
        assert_eq!(infer_quadrant_from_text("I feel comfortable"), "ul");
    }

    #[test]
    fn extract_recurring_pattern() {
        let ext = PreferenceExtractor::new();
        let results = ext.extract_from_message("Every time I notice a pattern in the logs");
        assert!(!results.is_empty());
        assert_eq!(results[0].extraction_type, "recurring_pattern");
    }

    #[test]
    fn extract_autonomous_insight() {
        let ext = PreferenceExtractor::new();
        let results =
            ext.extract_from_message("Based on my observations, the system tends to fail at night");
        assert!(!results.is_empty());
        assert_eq!(results[0].extraction_type, "autonomous_insight");
    }

    #[test]
    fn extract_from_messages_batch() {
        let ext = PreferenceExtractor::new();
        let messages = vec![
            "Don't use Docker for this project".to_string(),
            "I prefer using Rust for backend".to_string(),
            "The weather is nice today".to_string(),
            "Don't use Docker for this project".to_string(),
        ];
        let results = ext.extract_from_messages(&messages);
        let corrections: Vec<_> = results
            .iter()
            .filter(|r| r.extraction_type == "correction")
            .collect();
        assert_eq!(corrections.len(), 1);
    }

    #[test]
    fn deterministic_constraint_id() {
        let id1 = build_constraint_id("correction", "don't use Docker");
        let id2 = build_constraint_id("correction", "don't use Docker");
        let id3 = build_constraint_id("correction", "don't use Kubernetes");
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert!(id1.starts_with('c'));
    }

    #[test]
    fn constraint_id_case_insensitive() {
        let id1 = build_constraint_id("preference", "Use Rust");
        let id2 = build_constraint_id("preference", "use rust");
        assert_eq!(id1, id2);
    }

    fn setup_obs_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        crate::db::init_fts(&conn).unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    fn insert_observation(conn: &rusqlite::Connection, name: &str, description: &str) {
        let props = serde_json::json!({ "description": description });
        crate::db::crud::add_node(
            conn,
            &crate::models::NewNode {
                node_type: "observation".to_string(),
                name: name.to_string(),
                description: Some(description.to_string()),
                properties: Some(props),
                source: Some("turn_capture".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
    }

    #[test]
    fn detect_recurring_patterns_finds_repeated_keywords() {
        let conn = setup_obs_db();
        for i in 0..5 {
            insert_observation(&conn, &format!("obs{i}"), "deploy the server to AWS");
        }

        let ext = PreferenceExtractor::new();
        let results = ext.detect_recurring_patterns(&conn, 100);
        let deploy_patterns: Vec<_> = results
            .iter()
            .filter(|r| r.constraint_text.contains("deploy"))
            .collect();
        assert!(!deploy_patterns.is_empty());
    }

    #[test]
    fn detect_recurring_patterns_empty_when_few_observations() {
        let conn = setup_obs_db();
        insert_observation(&conn, "obs1", "deploy the server");

        let ext = PreferenceExtractor::new();
        let results = ext.detect_recurring_patterns(&conn, 100);
        let deploy_patterns: Vec<_> = results
            .iter()
            .filter(|r| r.constraint_text.contains("deploy"))
            .collect();
        assert!(deploy_patterns.is_empty());
    }

    #[test]
    fn detect_cross_cycle_patterns_finds_action_verbs() {
        let conn = setup_obs_db();
        insert_observation(&conn, "obs1", "I need to deploy the new version");
        insert_observation(&conn, "obs2", "Let me create a new module for this");

        let ext = PreferenceExtractor::new();
        let results = ext.detect_cross_cycle_patterns(&conn, 100);
        let labels: Vec<_> = results.iter().map(|r| r.constraint_text.as_str()).collect();
        assert!(labels.iter().any(|l| l.contains("deploy")));
        assert!(labels.iter().any(|l| l.contains("create")));
    }

    #[test]
    fn detect_cross_cycle_patterns_empty_when_no_actions() {
        let conn = setup_obs_db();
        insert_observation(&conn, "obs1", "The weather is nice today");

        let ext = PreferenceExtractor::new();
        let results = ext.detect_cross_cycle_patterns(&conn, 100);
        assert!(results.is_empty());
    }

    #[test]
    fn extract_memory_request_variants() {
        let ext = PreferenceExtractor::new();
        let cases = vec![
            "Remember that the port is 3000",
            "Don't forget that we use sqlite",
            "Make a note that the API key is stored in .env",
        ];
        for text in cases {
            let results = ext.extract_from_message(text);
            assert!(
                results.iter().any(|r| r.extraction_type == "memory"),
                "Expected memory extraction for: {}",
                text
            );
        }
    }

    #[test]
    fn extract_correction_variants() {
        let ext = PreferenceExtractor::new();
        let cases = vec![
            "Don't use Docker for this",
            "Stop using PostgreSQL",
            "Never go with MySQL",
        ];
        for text in cases {
            let results = ext.extract_from_message(text);
            assert!(
                results.iter().any(|r| r.extraction_type == "correction"),
                "Expected correction extraction for: {}",
                text
            );
        }
    }

    #[test]
    fn extract_preference_variants() {
        let ext = PreferenceExtractor::new();
        let cases = vec![
            "I prefer Rust for backend",
            "Always use cargo fmt before commit",
            "Please use snake_case",
            "I like when tests run in parallel",
            "Keep using WAL mode",
        ];
        for text in cases {
            let results = ext.extract_from_message(text);
            assert!(
                results.iter().any(|r| r.extraction_type == "preference"),
                "Expected preference extraction for: {}",
                text
            );
        }
    }

    #[test]
    fn extract_recurring_pattern_variants() {
        let ext = PreferenceExtractor::new();
        let cases = vec![
            "Every time I check the logs there's an error",
            "I notice the server crashes at night",
        ];
        for text in cases {
            let results = ext.extract_from_message(text);
            assert!(
                results
                    .iter()
                    .any(|r| r.extraction_type == "recurring_pattern"),
                "Expected recurring_pattern for: {}",
                text
            );
        }
    }

    #[test]
    fn extract_autonomous_insight_variants() {
        let ext = PreferenceExtractor::new();
        let cases = vec![
            "Based on my observations, the system slows down at night",
            "The data suggests we need more memory",
            "I've inferred that the pattern is weekly",
        ];
        for text in cases {
            let results = ext.extract_from_message(text);
            assert!(
                results
                    .iter()
                    .any(|r| r.extraction_type == "autonomous_insight"),
                "Expected autonomous_insight for: {}",
                text
            );
        }
    }

    #[test]
    fn infer_quadrant_all_types() {
        assert_eq!(infer_quadrant_from_text("deploy the server"), "lr");
        assert_eq!(infer_quadrant_from_text("I feel comfortable"), "ul");
        assert_eq!(infer_quadrant_from_text("our brand identity"), "ll");
        assert_eq!(infer_quadrant_from_text("build the workflow"), "ur");
        assert_eq!(infer_quadrant_from_text("something else entirely"), "ur");
    }

    #[test]
    fn extract_deduplication_across_messages() {
        let ext = PreferenceExtractor::new();
        let messages = vec![
            "I prefer using Rust".to_string(),
            "I prefer using Rust".to_string(),
        ];
        let results = ext.extract_from_messages(&messages);
        let pref_count = results
            .iter()
            .filter(|r| r.extraction_type == "preference")
            .count();
        assert_eq!(pref_count, 1);
    }
}
