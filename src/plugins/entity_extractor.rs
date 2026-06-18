//! Entity Extractor — pattern-based NER from text
//!
//! Port of `plugins/tdg/entity_extractor.py`.

use std::collections::HashMap;
use std::sync::LazyLock;

use rusqlite::{params, Connection};
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
        "deploy", "build", "test", "compile", "lint", "format", "commit", "push", "pull", "merge",
        "rebase", "branch", "install", "upgrade", "migrate", "backup", "restore", "docker",
        "kubernetes", "terraform", "ansible", "pytest", "cargo", "npm", "pnpm", "yarn", "pip",
    ]
});

/// Stop words to exclude from token matching.
static STOP_WORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
        "do", "does", "did", "will", "would", "could", "should", "may", "might", "can", "shall",
        "to", "of", "in", "for", "on", "with", "at", "by", "from", "as", "into", "through",
        "during", "before", "after", "above", "below", "between", "out", "off", "over", "under",
        "again", "further", "then", "once", "here", "there", "when", "where", "why", "how", "all",
        "each", "every", "both", "few", "more", "most", "other", "some", "such", "no", "nor",
        "not", "only", "own", "same", "so", "than", "too", "very", "just", "don", "now", "and",
        "but", "or", "if", "it", "its", "this", "that", "these", "those", "i", "you", "he", "she",
        "we", "they", "me", "him", "her", "us", "them", "my", "your", "his", "our", "their",
    ]
});

/// Token regex: alphanumeric sequences.
static TOKEN_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"[a-zA-Z0-9]+").unwrap());

/// Reddit username regex.
static REDDIT_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"u/([A-Za-z0-9_]{3,20})").unwrap());

/// Name token split regex: splits on colons, whitespace, underscores, hyphens, slashes.
static NAME_TOKEN_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"[:\s_/-]+").unwrap());

/// Token-level stopwords for graph matching (broader than top-level STOP_WORDS).
static TOKEN_STOPWORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has",
        "had", "do", "does", "did", "will", "would", "could", "should", "may", "might", "can",
        "shall", "to", "of", "in", "for", "on", "with", "at", "by", "from", "as", "into",
        "through", "during", "before", "after", "above", "below", "between", "out", "off", "over",
        "under", "again", "further", "then", "once", "here", "there", "when", "where", "why",
        "how", "all", "each", "every", "both", "few", "more", "most", "other", "some", "such",
        "no", "nor", "not", "only", "own", "same", "so", "than", "too", "very", "just", "don",
        "now", "and", "but", "or", "if", "it", "its", "this", "that", "these", "those", "i",
        "you", "he", "she", "we", "they", "me", "him", "her", "us", "them", "my", "your", "his",
        "our", "their", "used", "using", "need", "make", "like", "want", "know", "think", "see",
        "get", "give", "take", "come", "go", "run", "look", "put", "let", "say", "said", "tell",
        "told", "set", "also", "well", "back", "even", "still", "new", "way", "use", "work",
        "first", "last", "long", "great", "little", "right", "big", "high", "old", "different",
        "small", "large", "next", "early", "young", "important", "public", "bad", "same", "able",
    ]
});

/// Inverted index cache for graph node names.
pub struct EntityNameCache {
    /// name → node_id
    node_names: HashMap<String, String>,
    /// token → Vec<(node_id, node_name)>
    token_to_nodes: HashMap<String, Vec<(String, String)>>,
    /// node_id → node_type
    node_types: HashMap<String, String>,
}

impl EntityNameCache {
    pub fn build(conn: &Connection) -> TdgResult<Self> {
        let mut stmt = conn.prepare(
            "SELECT id, name, node_type FROM nodes WHERE valid_to IS NULL",
        )?;

        let mut node_names = HashMap::new();
        let mut token_to_nodes: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let mut node_types = HashMap::new();

        let rows: Vec<(String, String, String)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        for (id, name, node_type) in rows {
            node_names.insert(name.to_lowercase(), id.clone());
            node_types.insert(id.clone(), node_type);

            // Split name into tokens for inverted index
            let tokens: Vec<&str> = NAME_TOKEN_RE.split(&name)
                .filter(|t| t.len() > 2 && !TOKEN_STOPWORDS.contains(t))
                .collect();

            for token in tokens {
                let token_lower = token.to_lowercase();
                token_to_nodes
                    .entry(token_lower)
                    .or_default()
                    .push((id.clone(), name.clone()));
            }
        }

        Ok(Self {
            node_names,
            token_to_nodes,
            node_types,
        })
    }

    /// Resolve a text token to a node name via the inverted index.
    pub fn resolve_token(&self, token: &str) -> Option<&(String, String)> {
        self.token_to_nodes.get(token)?.first()
    }

    /// Look up a node ID from a name.
    pub fn get_node_id(&self, name: &str) -> Option<&str> {
        self.node_names.get(&name.to_lowercase()).map(|s| s.as_str())
    }

    /// Get the node type for a given node ID.
    pub fn get_node_type(&self, node_id: &str) -> Option<&str> {
        self.node_types.get(node_id).map(|s| s.as_str())
    }
}

/// The Entity Extractor — pattern-based NER from text.
pub struct EntityExtractor;

impl EntityExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract entities from text.
    pub fn extract(&self, text: &str, conn: Option<&Connection>) -> Vec<ExtractedEntity> {
        let mut entities = Vec::new();
        let mut seen: HashMap<String, bool> = HashMap::new();

        // Strategy 1: Known patterns
        for (pattern, etype) in KNOWN_PATTERNS.iter() {
            if text.to_lowercase().contains(pattern) {
                let name = pattern.to_string();
                if seen.insert(name.clone(), true).is_none() {
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

        // Strategy 4: Graph-based token matching (with inverted index cache)
        if let Some(db) = conn {
            if let Ok(graph_entities) = self.graph_token_match(text, db) {
                for e in graph_entities {
                    let key = e.name.to_lowercase();
                    if seen.insert(key, true).is_none() {
                        entities.push(e);
                    }
                }
            }
        }

        // Strategy 5: Alias-based entity resolution
        if let Some(db) = conn {
            if let Ok(alias_entities) = self.alias_resolve(text, db) {
                for e in alias_entities {
                    let key = e.name.to_lowercase();
                    if seen.insert(key, true).is_none() {
                        entities.push(e);
                    }
                }
            }
        }

        entities
    }

    /// Extract entities from a batch of messages with deduplication.
    pub fn extract_from_messages(
        &self,
        messages: &[&str],
        conn: Option<&Connection>,
    ) -> Vec<ExtractedEntity> {
        let mut all_entities = Vec::new();
        let mut seen_hashes: HashMap<String, bool> = HashMap::new();

        for msg in messages {
            let entities = self.extract(msg, conn);
            for e in entities {
                // Dedup by name + match_type
                let key = format!("{}:{}", e.name.to_lowercase(), e.match_type);
                if seen_hashes.insert(key, true).is_none() {
                    all_entities.push(e);
                }
            }
        }

        all_entities
    }

    /// Strategy 4: Enhanced graph-based token matching with inverted index cache.
    fn graph_token_match(&self, text: &str, conn: &Connection) -> TdgResult<Vec<ExtractedEntity>> {
        let cache = EntityNameCache::build(conn)?;
        let text_lower = text.to_lowercase();

        // Extract meaningful input tokens
        let input_tokens: Vec<String> = TOKEN_RE
            .find_iter(text)
            .map(|m| m.as_str().to_lowercase())
            .filter(|t| !TOKEN_STOPWORDS.contains(&t.as_str()) && t.len() > 2)
            .collect();

        if input_tokens.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        let mut seen_ids: HashMap<String, bool> = HashMap::new();

        // Pass 1: Exact full-name containment (score += 999)
        for (name_lower, node_id) in &cache.node_names {
            if seen_ids.contains_key(node_id) {
                continue;
            }

            // Exact match
            if text_lower == name_lower.as_str() {
                let node_type = cache.get_node_type(node_id).unwrap_or("unknown").to_string();
                seen_ids.insert(node_id.clone(), true);
                results.push(ExtractedEntity {
                    name: name_lower.clone(),
                    entity_type: node_type,
                    id: Some(node_id.clone()),
                    confidence: 0.99,
                    match_type: "exact_name".to_string(),
                });
                continue;
            }

            // Containment match
            if text_lower.contains(name_lower.as_str()) {
                let node_type = cache.get_node_type(node_id).unwrap_or("unknown").to_string();
                seen_ids.insert(node_id.clone(), true);
                results.push(ExtractedEntity {
                    name: name_lower.clone(),
                    entity_type: node_type,
                    id: Some(node_id.clone()),
                    confidence: 0.95,
                    match_type: "name_containment".to_string(),
                });
            }
        }

        // Pass 2: Token overlap scoring via inverted index
        let mut node_scores: HashMap<String, f64> = HashMap::new();
        let mut node_names: HashMap<String, String> = HashMap::new();

        for input_token in &input_tokens {
            if let Some(entries) = cache.token_to_nodes.get(input_token) {
                for (node_id, node_name) in entries {
                    if seen_ids.contains_key(node_id) {
                        continue;
                    }

                    let name_lower = node_name.to_lowercase();
                    let name_tokens: Vec<&str> = NAME_TOKEN_RE.split(&name_lower)
                        .filter(|t| t.len() > 2 && !TOKEN_STOPWORDS.contains(t))
                        .collect();

                    let total_tokens = name_tokens.len().max(1) as f64;

                    // Calculate overlap score
                    let overlap_count = name_tokens
                        .iter()
                        .filter(|nt| input_tokens.contains(&nt.to_string()))
                        .count() as f64;

                    let mut score = overlap_count / total_tokens;

                    // Bonus: full name tokens all present
                    if overlap_count == name_tokens.len() as f64 && name_tokens.len() > 1 {
                        score += 1.0;
                    }

                    // Plural stripping bonus
                    if input_token.ends_with('s') {
                        let singular = &input_token[..input_token.len() - 1];
                        if name_lower.contains(singular) {
                            score += 0.1;
                        }
                    }

                    let entry = node_scores.entry(node_id.clone()).or_insert(0.0);
                    if score > *entry {
                        *entry = score;
                        node_names.insert(node_id.clone(), node_name.clone());
                    }
                }
            }
        }

        // Collect scored results
        for (node_id, score) in &node_scores {
            if *score > 0.4 && !seen_ids.contains_key(node_id) {
                let node_type = cache.get_node_type(node_id).unwrap_or("unknown").to_string();
                let name = node_names.get(node_id).unwrap_or(node_id).clone();
                let confidence = (*score * 0.85).min(0.9);
                seen_ids.insert(node_id.clone(), true);
                results.push(ExtractedEntity {
                    name,
                    entity_type: node_type,
                    id: Some(node_id.clone()),
                    confidence,
                    match_type: "token_overlap".to_string(),
                });
            }
        }

        Ok(results)
    }

    /// Strategy 5: Alias-based entity resolution.
    fn alias_resolve(&self, text: &str, conn: &Connection) -> TdgResult<Vec<ExtractedEntity>> {
        let alias_map = self.expand_aliases(conn)?;
        if alias_map.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        let text_lower = text.to_lowercase();

        for (alias, canonical_name) in &alias_map {
            if text_lower.contains(&alias.to_lowercase()) {
                // Look up the canonical node
                let mut stmt = conn.prepare(
                    "SELECT id, node_type FROM nodes WHERE name = ?1 AND valid_to IS NULL LIMIT 1",
                )?;

                let node_info = stmt
                    .query_map(params![canonical_name], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?
                    .filter_map(|r| r.ok())
                    .next();

                if let Some((node_id, node_type)) = node_info {
                    results.push(ExtractedEntity {
                        name: canonical_name.clone(),
                        entity_type: node_type,
                        id: Some(node_id),
                        confidence: 0.85,
                        match_type: "alias_resolve".to_string(),
                    });
                }
            }
        }

        Ok(results)
    }

    /// Build alias → canonical name map from people nodes.
    /// Alias is derived from lowercase name tokens.
    pub fn expand_aliases(&self, conn: &Connection) -> TdgResult<HashMap<String, String>> {
        let mut stmt = conn.prepare(
            "SELECT id, name, properties FROM nodes WHERE valid_to IS NULL AND node_type = 'people'",
        )?;

        let mut alias_map: HashMap<String, String> = HashMap::new();

        let rows: Vec<(String, String, Option<String>)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        for (_id, name, properties) in rows {
            // The canonical name itself
            alias_map.insert(name.to_lowercase(), name.clone());

            // Extract aliases from properties JSON if present
            if let Some(props_json) = properties {
                if let Ok(props) = serde_json::from_str::<serde_json::Value>(&props_json) {
                    if let Some(aliases) = props.get("aliases").and_then(|a| a.as_array()) {
                        for alias_val in aliases {
                            if let Some(alias_str) = alias_val.as_str() {
                                alias_map.insert(alias_str.to_lowercase(), name.clone());
                            }
                        }
                    }
                }
            }

            // Name token aliases (e.g., "Ishan Patel" → "ishan", "ishanp")
            let tokens: Vec<&str> = name.split_whitespace().collect();
            for token in &tokens {
                let token_lower = token.to_lowercase();
                if token_lower.len() > 2 && !alias_map.contains_key(&token_lower) {
                    alias_map.insert(token_lower, name.clone());
                }
            }
        }

        Ok(alias_map)
    }

    /// Resolve a single alias to a canonical node name.
    pub fn resolve_alias(&self, alias: &str, conn: &Connection) -> TdgResult<Option<String>> {
        let alias_map = self.expand_aliases(conn)?;
        Ok(alias_map.get(&alias.to_lowercase()).cloned())
    }

    /// Add an alias to a person node's properties.
    pub fn add_alias(
        &self,
        node_id: &str,
        alias: &str,
        conn: &Connection,
    ) -> TdgResult<bool> {
        let mut stmt = conn.prepare(
            "SELECT properties FROM nodes WHERE id = ?1 AND valid_to IS NULL",
        )?;

        let current_props: Option<String> = stmt
            .query_map(params![node_id], |row| row.get::<_, Option<String>>(0))?
            .filter_map(|r| r.ok())
            .next()
            .flatten();

        let mut props: serde_json::Value = match current_props {
            Some(json_str) => serde_json::from_str(&json_str).unwrap_or(serde_json::json!({})),
            None => serde_json::json!({}),
        };

        let alias_str = alias.to_string();
        if let Some(obj) = props.as_object_mut() {
            let aliases = obj
                .entry("aliases")
                .or_insert_with(|| serde_json::json!([]));

            if let Some(arr) = aliases.as_array_mut() {
                if !arr.iter().any(|v: &serde_json::Value| v.as_str() == Some(&alias_str)) {
                    arr.push(serde_json::json!(alias));
                }
            }
        }

        let new_props = serde_json::to_string(&props)?;
        conn.execute(
            "UPDATE nodes SET properties = ?1, updated_at = datetime('now') WHERE id = ?2 AND valid_to IS NULL",
            params![new_props, node_id],
        )?;

        Ok(true)
    }

    /// Set all aliases for a person node (replaces existing).
    pub fn set_aliases(
        &self,
        node_id: &str,
        aliases: &[String],
        conn: &Connection,
    ) -> TdgResult<bool> {
        let mut stmt = conn.prepare(
            "SELECT properties FROM nodes WHERE id = ?1 AND valid_to IS NULL",
        )?;

        let current_props: Option<String> = stmt
            .query_map(params![node_id], |row| row.get::<_, Option<String>>(0))?
            .filter_map(|r| r.ok())
            .next()
            .flatten();

        let mut props: serde_json::Value = match current_props {
            Some(json_str) => serde_json::from_str(&json_str).unwrap_or(serde_json::json!({})),
            None => serde_json::json!({}),
        };

        let alias_values: Vec<serde_json::Value> = aliases
            .iter()
            .map(|a| serde_json::json!(a))
            .collect();

        props["aliases"] = serde_json::json!(alias_values);

        let new_props = serde_json::to_string(&props)?;
        conn.execute(
            "UPDATE nodes SET properties = ?1, updated_at = datetime('now') WHERE id = ?2 AND valid_to IS NULL",
            params![new_props, node_id],
        )?;

        Ok(true)
    }

    /// Get all aliases for a person node.
    pub fn get_aliases(&self, node_id: &str, conn: &Connection) -> TdgResult<Vec<String>> {
        let mut stmt = conn.prepare(
            "SELECT properties FROM nodes WHERE id = ?1 AND valid_to IS NULL",
        )?;

        let current_props: Option<String> = stmt
            .query_map(params![node_id], |row| row.get::<_, Option<String>>(0))?
            .filter_map(|r| r.ok())
            .next()
            .flatten();

        match current_props {
            Some(json_str) => {
                if let Ok(props) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    if let Some(aliases) = props.get("aliases").and_then(|a| a.as_array()) {
                        return Ok(aliases
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect());
                    }
                }
                Ok(Vec::new())
            }
            None => Ok(Vec::new()),
        }
    }
}

impl Default for EntityExtractor {
    fn default() -> Self {
        Self::new()
    }
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

    #[test]
    fn extract_from_messages_batch() {
        let ext = EntityExtractor::new();
        let messages = vec![
            "I used rust for the backend",
            "Docker deployment worked great",
            "Rust is fast",
        ];
        let entities = ext.extract_from_messages(&messages, None);
        // Should not have duplicate rust entries
        let rust_count = entities.iter().filter(|e| e.name == "rust").count();
        assert_eq!(rust_count, 1);
    }

    #[test]
    fn entity_name_cache_build() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE nodes (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                node_type TEXT NOT NULL,
                description TEXT DEFAULT '',
                properties TEXT DEFAULT NULL,
                quadrants TEXT DEFAULT NULL,
                drives TEXT DEFAULT NULL,
                lifecycle_state TEXT DEFAULT NULL,
                teleological_level TEXT DEFAULT NULL,
                developmental_stage TEXT DEFAULT NULL,
                confidence REAL DEFAULT 0.5,
                source TEXT DEFAULT '',
                parent_ids TEXT DEFAULT NULL,
                agent_path TEXT DEFAULT NULL,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now')),
                valid_from TEXT DEFAULT NULL,
                valid_to TEXT DEFAULT NULL,
                helpful_count INTEGER DEFAULT 0,
                retrieval_count INTEGER DEFAULT 0,
                agent_id TEXT DEFAULT NULL
            );",
        )
        .unwrap();

        conn.execute(
            "INSERT INTO nodes (id, name, node_type) VALUES ('n1', 'Ishan Patel', 'people')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes (id, name, node_type) VALUES ('n2', 'Rust Backend', 'skill')",
            [],
        )
        .unwrap();

        let cache = EntityNameCache::build(&conn).unwrap();
        assert_eq!(cache.get_node_id("ishan patel"), Some("n1"));
        assert_eq!(cache.get_node_id("rust backend"), Some("n2"));
        assert_eq!(cache.get_node_type("n1"), Some("people"));
    }
}
