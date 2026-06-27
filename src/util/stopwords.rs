pub static STOP_WORDS: &[&str] = &[
    // Articles & pronouns
    "the", "a", "an", "i", "you", "he", "she", "it", "we", "they", "me", "him", "her", "us",
    "them", "my", "your", "his", "its", "our", "their", "mine", "yours", "hers", "ours", "theirs",
    // Verbs
    "is", "are", "was", "were", "be", "been", "being", "have", "has", "had", "do", "does", "did",
    "will", "would", "could", "should", "may", "might", "can", "shall", "must", "need", "ought",
    "used", "don", "now",
    // Prepositions
    "to", "of", "in", "for", "on", "with", "at", "by", "from", "as", "into", "through", "during",
    "before", "after", "above", "below", "between", "out", "off", "over", "under",
    // Conjunctions & adverbs
    "again", "further", "then", "once", "here", "there", "when", "where", "why", "how", "all",
    "each", "every", "both", "few", "more", "most", "other", "some", "such", "no", "nor", "not",
    "only", "own", "same", "so", "than", "too", "very", "just", "and", "but", "or", "if", "it",
    "its", "this", "that", "these", "those", "because", "while", "about", "against", "also",
];

pub static EXTENDED_STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
    "do", "does", "did", "will", "would", "could", "should", "may", "might", "can", "shall", "to",
    "of", "in", "for", "on", "with", "at", "by", "from", "as", "into", "through", "during",
    "before", "after", "above", "below", "between", "out", "off", "over", "under", "again",
    "further", "then", "once", "here", "there", "when", "where", "why", "how", "all", "each",
    "every", "both", "few", "more", "most", "other", "some", "such", "no", "nor", "not", "only",
    "own", "same", "so", "than", "too", "very", "just", "don", "now", "and", "but", "or", "if",
    "it", "its", "this", "that", "these", "those", "i", "you", "he", "she", "we", "they", "me",
    "him", "her", "us", "them", "my", "your", "his", "our", "their", "used", "using", "need",
    "make", "like", "want", "know", "think", "see", "get", "give", "take", "come", "go", "run",
    "look", "put", "let", "say", "said", "tell", "told", "set", "also", "well", "back", "even",
    "still", "new", "way", "use", "work", "first", "last", "long", "great", "little", "right",
    "big", "high", "old", "different", "small", "large", "next", "early", "young", "important",
    "public", "bad", "same", "able",
];
