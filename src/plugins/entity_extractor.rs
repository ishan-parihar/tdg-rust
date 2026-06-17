//! Entity Extractor — pattern-based NER from text
//!
//! Port of `plugins/tdg/entity_extractor.py`.

use std::collections::HashMap;
use std::sync::LazyLock;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::TdgResult;

/// Extracted entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: String,
    pub id: Option<String>,
    pub confidence: f64,
    pub match_type: String,
}

/// Known entity patterns.
static KNOWN_PATTERNS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    vec![
        ("telegram", "platform"),
        ("discord", "platform"),
        ("github", "platform"),
        ("rust", "tool"),
        ("python", "tool"),
        ("docker", "tool"),
        ("git", "tool"),
        ("sql", "tool"),
        ("sqlite", "tool"),
        ("ollama", "tool"),
        ("openai", "tool"),
        ("anthropic", "tool"),
        ("claude", "tool"),
        ("chatgpt", "tool"),
        ("vscode", "tool"),
        ("cursor", "tool"),
        ("linux", "tool"),
        ("macos", "tool"),
        ("windows", "tool"),
        ("aws", "platform"),
        ("gcp", "platform"),
        ("azure", "platform"),
        ("supabase", "platform"),
        ("vercel", "platform"),
        ("railway", "platform"),
        ("fly", "platform"),
    ]
});

/// Tool/action-bound words.
static TOOL_WORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "deploy", "build", "test", "compile", "lint", "format",
        "commit", "push", "pull", "merge", "rebase", "branch",
        "install", "upgrade", "migrate", "backup", "restore",
        "docker", "kubernetes", "terraform", "ansible",
        "pytest", "cargo", "npm", "pnpm", "yarn", "pip",
    ]
});

/// Stop words to exclude from token matching.
static STOP_WORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
        "have", "has", "had", "do", "does", "did", "will", "would", "could",
        "should", "may", "might", "can", "shall", "to", "of", "in", "for",
        "on", "with", "at", "by", "from", "as", "into", "through", "during",
        "before", "after", "above", "below", "between", "out", "off", "over",
        "under", "again", "further", "then", "once", "here", "there", "when",
        "where", "why", "how", "all", "each", "every", "both", "few", "more",
        "most", "other", "some", "such", "no", "nor", "not", "only", "own",
        "same", "so", "than", "too", "very", "just", "don", "now",
        "and", "but", "or", "if", "it", "its", "this", "that", "these",
        "those", "i", "you", "he", "she", "we", "they", "me", "him",
        "her", "us", "them", "my", "your", "his", "our", "their",
    ]
});

/// Token regex: alphanumeric sequences.
static TOKEN_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"[a-zA-Z0-9]+").unwrap());

/// Reddit username regex.
static REDDIT_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"u/([A-Za-z0-9_]{3,20})").unwrap());

/// The Entity Extractor — pattern-based NER from text.
pub struct EntityExtractor;

impl EntityExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract entities from text.
    pub fn extract(&self, text: &str, conn: Option<&Connection>) -> Vec<ExtractedEntity> {
        let mut entities = Vec::new();
        let mut seen = HashMap::new();

        // Strategy 1: Known patterns
        for (pattern, etype) in KNOWN_PATTERNS.iter() {
            if text.to_lowercase().contains(pattern) {
                let name = pattern.to_string();
                if !seen.contains_key(&name) {
                    seen.insert(name.clone(), true);
                    entities.push(ExtractedEntity {
                        name: name.clone(),
                        entity_type: etype.to_string(),
                        id: None,
                        confidence: 0.8,
                        match_type: "known_pattern".to_string(),
                    });
                }
            }
        }

        // Strategy 2: Reddit mentions
        for cap in REDDIT_RE.captures_iter(text) {
            if let Some(username) = cap.get(1) {
                let name = format!("u/{}", username.as_str());
                if !seen.contains_key(&name) {
                    seen.insert(name.clone(), true);
                    entities.push(ExtractedEntity {
                        name: name.clone(),
                        entity_type: "people".to_string(),
                        id: None,
                        confidence: 0.9,
                        match_type: "reddit_mention".to_string(),
                    });
                }
            }
        }

        // Strategy 3: Tool words in action context
        let lower = text.to_lowercase();
        for tool in TOOL_WORDS.iter() {
            if lower.contains(tool) && !seen.contains_key(*tool) {
                seen.insert(tool.to_string(), true);
                entities.push(ExtractedEntity {
                    name: tool.to_string(),
                    entity_type: "tool".to_string(),
                    id: None,
                    confidence: 0.6,
                    match_type: "tool_action".to_string(),
                });
            }
        }

        // Strategy 4: Graph-based token matching
        if let Some(db) = conn {
            if let Ok(graph_entities) = self.graph_token_match(text, db) {
                for e in graph_entities {
                    let key = e.name.to_lowercase();
                    if !seen.contains_key(&key) {
                        seen.insert(key, true);
                        entities.push(e);
                    }
                }
            }
        }

        entities
    }

    fn graph_token_match(
        &self,
        text: &str,
        conn: &Connection,
    ) -> TdgResult<Vec<ExtractedEntity>> {
        // Load entity nodes from graph
        let mut stmt = conn.prepare(
            "SELECT id, name, node_type FROM nodes WHERE valid_to IS NULL AND node_type IN ('people', 'skill', 'artifact', 'being')",
        )?;

        let graph_nodes: Vec<(String, String, String)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Tokenize input
        let input_tokens: Vec<String> = TOKEN_RE
            .find_iter(text)
            .map(|m| m.as_str().to_lowercase())
            .filter(|t| !STOP_WORDS.contains(&t.as_str()) && t.len() > 2)
            .collect();

        if input_tokens.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();

        for (id, name, node_type) in &graph_nodes {
            let name_lower = name.to_lowercase();
            let name_tokens: Vec<&str> = name_lower.split_whitespace().collect();

            // Exact full name match
            if lower_text_contains(text, &name_lower) {
                results.push(ExtractedEntity {
                    name: name.clone(),
                    entity_type: node_type.clone(),
                    id: Some(id.clone()),
                    confidence: 0.95,
                    match_type: "exact_name".to_string(),
                });
                continue;
            }

            // Token overlap scoring
            let overlap_count = name_tokens
                .iter()
                .filter(|nt| input_tokens.contains(&nt.to_string()))
                .count();

            if overlap_count > 0 {
                let score = overlap_count as f64 / name_tokens.len().max(1) as f64;
                if score > 0.5 {
                    results.push(ExtractedEntity {
                        name: name.clone(),
                        entity_type: node_type.clone(),
                        id: Some(id.clone()),
                        confidence: score * 0.8,
                        match_type: "token_overlap".to_string(),
                    });
                }
            }
        }

        Ok(results)
    }
}

fn lower_text_contains(text: &str, needle: &str) -> bool {
    text.to_lowercase().contains(needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_known_patterns() {
        let ext = EntityExtractor::new();
        let entities = ext.extract("I used rust and docker to build the project", None);
        assert!(entities.iter().any(|e| e.name == "rust"));
        assert!(entities.iter().any(|e| e.name == "docker"));
    }

    #[test]
    fn extract_reddit_mentions() {
        let ext = EntityExtractor::new();
        let entities = ext.extract("Check u/ishanp profile", None);
        assert!(entities.iter().any(|e| e.name == "u/ishanp"));
    }

    #[test]
    fn extract_tool_actions() {
        let ext = EntityExtractor::new();
        let entities = ext.extract("I need to deploy and build this", None);
        assert!(entities.iter().any(|e| e.name == "deploy"));
        assert!(entities.iter().any(|e| e.name == "build"));
    }

    #[test]
    fn no_duplicates() {
        let ext = EntityExtractor::new();
        let entities = ext.extract("rust is better than rust for this", None);
        let rust_count = entities.iter().filter(|e| e.name == "rust").count();
        assert_eq!(rust_count, 1);
    }
}
