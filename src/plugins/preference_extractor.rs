//! Preference Extractor — pattern-based preference/constraint extraction
//!
//! Port of `plugins/tdg/preference_extractor.py`.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

/// Extraction type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferenceExtraction {
    pub extraction_type: String, // "preference", "correction", "memory"
    pub constraint_text: String,
    pub confidence: f64,
    pub quadrant: String,
}

/// Correction patterns (negative constraints).
static CORRECTION_PATTERNS: LazyLock<Vec<(&'static str, f64)>> = LazyLock::new(|| {
    vec![
        (r"(?i)don't\s+(do|use|try|go)\s+(.+)", 0.85),
        (r"(?i)stop\s+(doing|using|saying)\s+(.+)", 0.85),
        (r"(?i)never\s+(do|use|say|go)\s+(.+)", 0.9),
        (r"(?i)avoid\s+(.+)", 0.8),
        (r"(?i)no\s+more\s+(.+)", 0.8),
    ]
});

/// Preference patterns (positive constraints).
static PREFERENCE_PATTERNS: LazyLock<Vec<(&'static str, f64)>> = LazyLock::new(|| {
    vec![
        (r"(?i)I\s+prefer\s+(.+)", 0.85),
        (r"(?i)always\s+(do|use|say|go)\s+(.+)", 0.8),
        (r"(?i)please\s+(do|use|say)\s+(.+)", 0.75),
        (r"(?i)I\s+like\s+when\s+(.+)", 0.8),
        (r"(?i)keep\s+(doing|using|saying)\s+(.+)", 0.8),
    ]
});

/// Memory patterns (explicit remember requests).
static MEMORY_PATTERNS: LazyLock<Vec<(&'static str, f64)>> = LazyLock::new(|| {
    vec![
        (r"(?i)(remember|note|memo|recall)\s+(?:that\s+)?(.+)", 0.9),
        (r"(?i)don't\s+forget\s+(?:that\s+)?(.+)", 0.9),
        (r"(?i)make\s+a\s+note\s+(?:that\s+)?(.+)", 0.85),
    ]
});

/// Topic keyword classification.
static TOPIC_KEYWORDS: LazyLock<Vec<(&'static str, Vec<&'static str>)>> = LazyLock::new(|| {
    vec![
        ("lr", vec!["deploy", "server", "database", "api", "infrastructure", "docker", "aws", "pricing", "cost"]),
        ("ul", vec!["prefer", "feel", "like", "dislike", "comfortable", "trust", "believe", "value"]),
        ("ll", vec!["identity", "brand", "name", "persona", "style", "tone", "voice", "culture"]),
        ("ur", vec!["do", "action", "behavior", "habit", "practice", "technique", "approach"]),
    ]
});

/// The Preference Extractor — pattern-based preference/constraint extraction.
pub struct PreferenceExtractor;

impl PreferenceExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract preferences from a message.
    pub fn extract_from_message(&self, text: &str) -> Vec<PreferenceExtraction> {
        let mut results = Vec::new();

        // Check memory patterns first (highest priority)
        for (pattern, confidence) in MEMORY_PATTERNS.iter() {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(caps) = re.captures(text) {
                    if let Some(content) = caps.get(2).or_else(|| caps.get(1)) {
                        let quadrant = self.infer_quadrant(content.as_str());
                        results.push(PreferenceExtraction {
                            extraction_type: "memory".to_string(),
                            constraint_text: content.as_str().trim().to_string(),
                            confidence: *confidence,
                            quadrant,
                        });
                    }
                }
            }
        }

        // Check correction patterns
        for (pattern, confidence) in CORRECTION_PATTERNS.iter() {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(caps) = re.captures(text) {
                    if let Some(content) = caps.get(2) {
                        let quadrant = self.infer_quadrant(content.as_str());
                        results.push(PreferenceExtraction {
                            extraction_type: "correction".to_string(),
                            constraint_text: content.as_str().trim().to_string(),
                            confidence: *confidence,
                            quadrant,
                        });
                    }
                }
            }
        }

        // Check preference patterns
        for (pattern, confidence) in PREFERENCE_PATTERNS.iter() {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(caps) = re.captures(text) {
                    if let Some(content) = caps.get(2).or_else(|| caps.get(1)) {
                        let quadrant = self.infer_quadrant(content.as_str());
                        results.push(PreferenceExtraction {
                            extraction_type: "preference".to_string(),
                            constraint_text: content.as_str().trim().to_string(),
                            confidence: *confidence,
                            quadrant,
                        });
                    }
                }
            }
        }

        // Deduplicate
        let mut seen = std::collections::HashSet::new();
        results.retain(|r| seen.insert(r.constraint_text.clone()));

        results
    }

    fn infer_quadrant(&self, text: &str) -> String {
        let lower = text.to_lowercase();

        for (quadrant, keywords) in TOPIC_KEYWORDS.iter() {
            if keywords.iter().any(|kw| lower.contains(kw)) {
                return quadrant.to_string();
            }
        }

        "ur".to_string() // default
    }
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
    }

    #[test]
    fn extract_preference() {
        let ext = PreferenceExtractor::new();
        let results = ext.extract_from_message("I prefer using Rust for backend");
        assert!(!results.is_empty());
        assert_eq!(results[0].extraction_type, "preference");
    }

    #[test]
    fn extract_memory_request() {
        let ext = PreferenceExtractor::new();
        let results = ext.extract_from_message("Remember that the server is on port 3000");
        assert!(!results.is_empty());
        assert_eq!(results[0].extraction_type, "memory");
    }

    #[test]
    fn no_extraction() {
        let ext = PreferenceExtractor::new();
        let results = ext.extract_from_message("The weather is nice today");
        assert!(results.is_empty());
    }

    #[test]
    fn quadrant_inference() {
        let ext = PreferenceExtractor::new();
        assert_eq!(ext.infer_quadrant("deploy the server"), "lr");
        assert_eq!(ext.infer_quadrant("I feel comfortable"), "ul");
    }
}
