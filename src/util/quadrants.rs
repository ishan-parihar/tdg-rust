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
    for (quadrant, keywords) in QUADRANT_KEYWORDS.iter() {
        if keywords.iter().any(|kw| lower.contains(kw)) {
            return quadrant.to_string();
        }
    }
    "ur".to_string()
}
