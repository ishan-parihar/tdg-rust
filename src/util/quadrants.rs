pub static QUADRANT_KEYWORDS: &[(&str, &[&str])] = &[
    (
        "lr",
        &[
            "deploy", "server", "database", "api", "infrastructure", "docker", "aws", "pricing",
            "cost", "compile", "test", "run", "fix", "debug", "hosting", "domain", "ssl",
            "nginx", "kubernetes",
        ],
    ),
    (
        "ul",
        &[
            "prefer", "feel", "like", "dislike", "comfortable", "trust", "believe", "value",
            "think", "understand", "learn", "satisfied", "frustrated", "happy", "unhappy", "opinion",
        ],
    ),
    (
        "ll",
        &[
            "identity", "brand", "name", "persona", "style", "tone", "voice", "culture", "remember",
            "note", "memo", "image", "reputation", "messaging", "positioning",
        ],
    ),
    (
        "ur",
        &[
            "do", "action", "behavior", "habit", "practice", "technique", "approach", "create",
            "build", "make", "write", "implement", "workflow", "process", "method", "routine",
            "execute",
        ],
    ),
];

pub fn infer_quadrant(text: &str) -> String {
    let lower = text.to_lowercase();
    // Split on non-alphanumeric boundaries to get whole words, then check
    // for keyword matches. Previously used substring matching (lower.contains(kw))
    // which caused false positives:
    //   "do" matched "document", "domain", "todo", "undo"
    //   "fix" matched "prefix", "suffix", "fixture"
    //   "make" matched "marketplace", "makeup"
    //   "test" matched "latest", "protest", "contest"
    //   "api" matched "rapid", "shaping"
    //   "note" matched "denote", "quote"
    //   "name" matched "filename", "namespace"
    // Almost any technical text contains "document" or "domain", so nearly
    // everything was classified as UR quadrant.
    let words: std::collections::HashSet<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    for (quadrant, keywords) in QUADRANT_KEYWORDS.iter() {
        if keywords.iter().any(|kw| words.contains(*kw)) {
            return quadrant.to_string();
        }
    }
    "ur".to_string()
}
