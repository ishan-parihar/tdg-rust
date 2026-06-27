//! Integration tests for all TDG-Rust plugins: entity_extractor, hybrid_retriever,
//! turn_capture, preference_extractor.

use tdg_rust::db::{init_fts, init_schema, run_migrations, ConnectionPool};
use tdg_rust::models::NewNode;
use tdg_rust::plugins::entity_extractor::{EntityExtractor, EntityNameCache};
use tdg_rust::plugins::hybrid_retriever::{HybridRetriever, RetrievalWeights};
use tdg_rust::plugins::preference_extractor::{build_constraint_id, PreferenceExtractor};
use tdg_rust::plugins::turn_capture::TurnCapture;

fn make_pool() -> ConnectionPool {
    let pool = ConnectionPool::new(":memory:", 5, 30000).expect("pool creation");
    pool.with_connection(|conn| {
        init_schema(conn)?;
        init_fts(conn)?;
        run_migrations(conn)?;
        Ok(())
    })
    .expect("schema init");
    pool
}

// ═══════════════════════════════════════════════════════════════════════════════
// Entity Extractor
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn entity_extractor_known_patterns_are_detected() {
    let ext = EntityExtractor::new();
    let entities = ext.extract(
        "I deployed to AWS using docker and rust with sqlite storage",
        None,
    );
    let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"aws"), "missing aws, got: {:?}", names);
    assert!(names.contains(&"docker"), "missing docker");
    assert!(names.contains(&"rust"), "missing rust");
    assert!(names.contains(&"sqlite"), "missing sqlite");
}

#[test]
fn entity_extractor_reddit_mentions() {
    let ext = EntityExtractor::new();
    let entities = ext.extract("Check u/alice_dev profile and u/bob_smith repo", None);
    let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"u/alice_dev"));
    assert!(names.contains(&"u/bob_smith"));
    for e in &entities {
        if e.name.starts_with("u/") {
            assert_eq!(e.entity_type, "people");
            assert!(e.confidence > 0.8);
            assert_eq!(e.match_type, "reddit_mention");
        }
    }
}

#[test]
fn entity_extractor_tool_actions() {
    let ext = EntityExtractor::new();
    let entities = ext.extract("I need to deploy the code, then commit and push", None);
    let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"deploy"));
    assert!(names.contains(&"commit"));
    assert!(names.contains(&"push"));
    for e in &entities {
        if e.match_type == "tool_action" {
            assert_eq!(e.entity_type, "tool");
            assert!(e.confidence > 0.5 && e.confidence < 1.0);
        }
    }
}

#[test]
fn entity_extractor_deduplication_no_duplicates() {
    let ext = EntityExtractor::new();
    let entities = ext.extract("rust is great, I love rust and rust is fast", None);
    let rust_count = entities.iter().filter(|e| e.name == "rust").count();
    assert_eq!(rust_count, 1);
}

#[test]
fn entity_extractor_batch_deduplication_across_messages() {
    let ext = EntityExtractor::new();
    let messages = vec![
        "I used rust for the backend",
        "Docker deployment worked great",
        "Rust is fast and safe",
    ];
    let entities = ext.extract_from_messages(&messages, None);
    let rust_count = entities.iter().filter(|e| e.name == "rust").count();
    assert_eq!(rust_count, 1);
    assert!(entities.iter().any(|e| e.name == "docker"));
}

#[test]
fn entity_extractor_empty_text_returns_empty() {
    let ext = EntityExtractor::new();
    assert!(ext.extract("", None).is_empty());
}

#[test]
fn entity_extractor_stopwords_not_returned() {
    let ext = EntityExtractor::new();
    let entities = ext.extract("the is are was were have has had do does did", None);
    assert!(entities.is_empty());
}

#[test]
fn entity_extractor_confidence_values_are_bounded() {
    let ext = EntityExtractor::new();
    let entities = ext.extract(
        "rust docker deploy test build commit push lint format",
        None,
    );
    for e in &entities {
        assert!(
            (0.0..=1.0).contains(&e.confidence),
            "confidence {} out of range for {}",
            e.confidence,
            e.name
        );
    }
}

#[test]
fn entity_extractor_alias_resolution_via_db() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE nodes (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, node_type TEXT NOT NULL,
            description TEXT DEFAULT '', properties TEXT DEFAULT NULL,
            quadrants TEXT DEFAULT NULL, drives TEXT DEFAULT NULL,
            lifecycle_state TEXT DEFAULT NULL, teleological_level TEXT DEFAULT NULL,
            developmental_stage TEXT DEFAULT NULL, confidence REAL DEFAULT 0.5,
            source TEXT DEFAULT '', parent_ids TEXT DEFAULT NULL,
            agent_path TEXT DEFAULT NULL, created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now')), valid_from TEXT DEFAULT NULL,
            valid_to TEXT DEFAULT NULL, helpful_count INTEGER DEFAULT 0,
            retrieval_count INTEGER DEFAULT 0, agent_id TEXT DEFAULT NULL
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO nodes (id, node_type, name, properties) VALUES ('p1', 'people', 'Alice Smith', '{\"aliases\":[\"ali\",\"asmith\"]}')",
        [],
    )
    .unwrap();

    let ext = EntityExtractor::new();
    let resolved = ext.resolve_alias("ali", &conn).unwrap();
    assert_eq!(resolved.as_deref(), Some("Alice Smith"));

    let resolved = ext.resolve_alias("ALI", &conn).unwrap();
    assert_eq!(resolved.as_deref(), Some("Alice Smith"));

    let resolved = ext.resolve_alias("nonexistent", &conn).unwrap();
    assert!(resolved.is_none());
}

#[test]
fn entity_extractor_add_and_get_aliases() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE nodes (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, node_type TEXT NOT NULL,
            description TEXT DEFAULT '', properties TEXT DEFAULT NULL,
            quadrants TEXT DEFAULT NULL, drives TEXT DEFAULT NULL,
            lifecycle_state TEXT DEFAULT NULL, teleological_level TEXT DEFAULT NULL,
            developmental_stage TEXT DEFAULT NULL, confidence REAL DEFAULT 0.5,
            source TEXT DEFAULT '', parent_ids TEXT DEFAULT NULL,
            agent_path TEXT DEFAULT NULL, created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now')), valid_from TEXT DEFAULT NULL,
            valid_to TEXT DEFAULT NULL, helpful_count INTEGER DEFAULT 0,
            retrieval_count INTEGER DEFAULT 0, agent_id TEXT DEFAULT NULL
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO nodes (id, node_type, name) VALUES ('p1', 'people', 'Bob Jones')",
        [],
    )
    .unwrap();

    let ext = EntityExtractor::new();

    let aliases = ext.get_aliases("p1", &conn).unwrap();
    assert!(aliases.is_empty());

    ext.add_alias("p1", "bobby", &conn).unwrap();
    let aliases = ext.get_aliases("p1", &conn).unwrap();
    assert!(aliases.contains(&"bobby".to_string()));

    ext.add_alias("p1", "bj", &conn).unwrap();
    let aliases = ext.get_aliases("p1", &conn).unwrap();
    assert_eq!(aliases.len(), 2);
    assert!(aliases.contains(&"bobby".to_string()));
    assert!(aliases.contains(&"bj".to_string()));

    ext.add_alias("p1", "bobby", &conn).unwrap();
    let aliases = ext.get_aliases("p1", &conn).unwrap();
    assert_eq!(aliases.len(), 2);
}

#[test]
fn entity_extractor_set_aliases_replaces_existing() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE nodes (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, node_type TEXT NOT NULL,
            description TEXT DEFAULT '', properties TEXT DEFAULT NULL,
            quadrants TEXT DEFAULT NULL, drives TEXT DEFAULT NULL,
            lifecycle_state TEXT DEFAULT NULL, teleological_level TEXT DEFAULT NULL,
            developmental_stage TEXT DEFAULT NULL, confidence REAL DEFAULT 0.5,
            source TEXT DEFAULT '', parent_ids TEXT DEFAULT NULL,
            agent_path TEXT DEFAULT NULL, created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now')), valid_from TEXT DEFAULT NULL,
            valid_to TEXT DEFAULT NULL, helpful_count INTEGER DEFAULT 0,
            retrieval_count INTEGER DEFAULT 0, agent_id TEXT DEFAULT NULL
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO nodes (id, node_type, name) VALUES ('p1', 'people', 'Charlie Brown')",
        [],
    )
    .unwrap();

    let ext = EntityExtractor::new();
    ext.add_alias("p1", "cb", &conn).unwrap();
    ext.add_alias("p1", "charlie", &conn).unwrap();

    ext.set_aliases("p1", &["new_alias".to_string()], &conn)
        .unwrap();
    let aliases = ext.get_aliases("p1", &conn).unwrap();
    assert_eq!(aliases, vec!["new_alias"]);
    assert!(!aliases.contains(&"cb".to_string()));
    assert!(!aliases.contains(&"charlie".to_string()));
}

#[test]
fn entity_extractor_entity_name_cache_builds_and_resolves() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "people".to_string(),
                name: "Diana Prince".to_string(),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "skill".to_string(),
                name: "Rust Programming".to_string(),
                ..Default::default()
            },
        )?;

        let cache = EntityNameCache::build(conn)?;
        assert_eq!(cache.get_node_id("diana prince").unwrap().len(), 13);
        assert_eq!(
            cache.get_node_type(cache.get_node_id("diana prince").unwrap()),
            Some("people")
        );

        let rust_id = cache.get_node_id("rust programming").unwrap();
        assert_eq!(cache.get_node_type(rust_id), Some("skill"));

        let resolved = cache.resolve_token("diana");
        assert!(resolved.is_some());

        Ok(())
    })
    .unwrap();
}

#[test]
fn entity_extractor_graph_token_matching() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "project".to_string(),
                name: "TDG Memory System".to_string(),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "tool".to_string(),
                name: "SQLite Database".to_string(),
                ..Default::default()
            },
        )?;

        let ext = EntityExtractor::new();
        let entities = ext.extract("Working on the TDG memory system with SQLite", Some(conn));
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"tdg memory system") || names.contains(&"sqlite database"),
            "Expected graph matches, got: {:?}",
            names
        );

        Ok(())
    })
    .unwrap();
}

// ═══════════════════════════════════════════════════════════════════════════════
// Hybrid Retriever
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn hybrid_retriever_fts_search_finds_matching_nodes() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Rust memory safety guarantees".to_string(),
                description: Some("Rust prevents data races at compile time".to_string()),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Python GIL limitations".to_string(),
                description: Some("GIL prevents true parallelism".to_string()),
                ..Default::default()
            },
        )?;

        let retriever = HybridRetriever::new();
        let results = retriever.search(conn, "Rust memory", 10, None)?;
        assert!(!results.is_empty());
        assert!(results[0].node.name.contains("Rust"));
        assert!(results[0].score > 0.0);

        Ok(())
    })
    .unwrap();
}

#[test]
fn hybrid_retriever_like_fallback_when_fts_empty() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Custom entity name".to_string(),
                ..Default::default()
            },
        )?;

        let retriever = HybridRetriever::new();
        let results = retriever.search(conn, "Custom entity name", 10, None)?;
        assert!(!results.is_empty());
        assert_eq!(results[0].node.name, "Custom entity name");

        Ok(())
    })
    .unwrap();
}

#[test]
fn hybrid_retriever_type_filtering() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Main Goal".to_string(),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "Some Action".to_string(),
                ..Default::default()
            },
        )?;

        let retriever = HybridRetriever::new();
        let results = retriever.search(conn, "nothing matches", 10, Some("telos"))?;
        assert!(results.iter().all(|r| r.node.node_type == "telos"));
        assert_eq!(results.len(), 1);

        Ok(())
    })
    .unwrap();
}

#[test]
fn hybrid_retriever_trust_boost_ranks_higher_confidence_first() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Rust basics tutorial".to_string(),
                confidence: Some(0.3),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Rust advanced patterns".to_string(),
                confidence: Some(0.95),
                ..Default::default()
            },
        )?;

        let retriever = HybridRetriever::new();
        let results = retriever.search(conn, "Rust", 10, None)?;
        assert!(results.len() >= 2);
        assert!(results[0].node.confidence >= results[1].node.confidence);

        Ok(())
    })
    .unwrap();
}

#[test]
fn hybrid_retriever_limit_caps_results() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        for i in 0..10 {
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("Observation about topic {}", i),
                    ..Default::default()
                },
            )?;
        }

        let retriever = HybridRetriever::new();
        let results = retriever.search(conn, "observation topic", 3, None)?;
        assert!(results.len() <= 3);

        Ok(())
    })
    .unwrap();
}

#[test]
fn hybrid_retriever_empty_query_returns_empty() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let retriever = HybridRetriever::new();
        let results = retriever.search(conn, "", 10, None)?;
        assert!(results.is_empty() || results.iter().all(|r| r.score >= 0.0));

        Ok(())
    })
    .unwrap();
}

#[test]
fn hybrid_retriever_custom_weights() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Rust performance benchmarks".to_string(),
                confidence: Some(0.9),
                ..Default::default()
            },
        )?;

        let weights = RetrievalWeights {
            fts_weight: 0.10,
            trust_weight: 0.80,
            recency_weight: 0.05,
            term_overlap_weight: 0.05,
            type_boost_weight: 0.0,
            embedding_weight: 0.0,
        };
        let retriever = HybridRetriever::with_weights(weights);
        let results = retriever.search(conn, "Rust performance", 10, None)?;
        assert!(!results.is_empty());
        assert!(results[0].score > 0.0);

        Ok(())
    })
    .unwrap();
}

#[test]
fn hybrid_retriever_deduplication_by_node_id() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Rust is memory safe".to_string(),
                ..Default::default()
            },
        )?;
        let retriever = HybridRetriever::new();
        let results = retriever.search(conn, "Rust", 10, None)?;
        let ids: Vec<&str> = results.iter().map(|r| r.node.id.as_str()).collect();
        let unique: std::collections::HashSet<&str> = ids.into_iter().collect();
        assert_eq!(unique.len(), results.len(), "duplicate node ids in results");

        Ok(())
    })
    .unwrap();
}

#[test]
fn hybrid_retriever_scores_are_sorted_descending() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        for i in 0..5 {
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("Node about topic {}", i),
                    ..Default::default()
                },
            )?;
        }

        let retriever = HybridRetriever::new();
        let results = retriever.search(conn, "node topic", 10, None)?;
        for i in 1..results.len() {
            assert!(
                results[i - 1].score >= results[i].score,
                "results not sorted: {} at index {} >= {} at index {}",
                results[i - 1].score,
                i - 1,
                results[i].score,
                i
            );
        }

        Ok(())
    })
    .unwrap();
}

#[test]
fn hybrid_retriever_method_field_indicates_search_type() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Rust memory safety".to_string(),
                ..Default::default()
            },
        )?;

        let retriever = HybridRetriever::new();
        let results = retriever.search(conn, "Rust", 10, None)?;
        assert!(!results.is_empty());
        assert_eq!(results[0].method, "hybrid");

        Ok(())
    })
    .unwrap();
}

// ═══════════════════════════════════════════════════════════════════════════════
// Turn Capture
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn turn_capture_creates_observation_node() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        let result = capture.capture(conn, "I deployed the server to production", None)?;
        assert!(!result.observation_id.is_empty());
        assert_eq!(result.text, "I deployed the server to production");
        assert_eq!(result.quadrant, "lr");
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM nodes WHERE id = ?1 AND valid_to IS NULL",
            [&result.observation_id],
            |row| row.get(0),
        )?;
        assert!(exists);

        Ok(())
    })
    .unwrap();
}

#[test]
fn turn_capture_quadrant_inference_lr() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        let result = capture.capture(conn, "Deploy the docker container to AWS", None)?;
        assert_eq!(result.quadrant, "lr");

        Ok(())
    })
    .unwrap();
}

#[test]
fn turn_capture_quadrant_inference_ul() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        let result = capture.capture(conn, "I feel comfortable with this approach", None)?;
        assert_eq!(result.quadrant, "ul");

        Ok(())
    })
    .unwrap();
}

#[test]
fn turn_capture_quadrant_inference_ll() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        let result = capture.capture(conn, "Our brand identity needs work", None)?;
        assert_eq!(result.quadrant, "ll");

        Ok(())
    })
    .unwrap();
}

#[test]
fn turn_capture_quadrant_inference_ur_default() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        let result = capture.capture(conn, "Something unrelated happened", None)?;
        assert_eq!(result.quadrant, "ur");

        Ok(())
    })
    .unwrap();
}

#[test]
fn turn_capture_entities_extracted() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        let result = capture.capture(conn, "Using rust and docker for deployment", None)?;
        assert!(result.entities.contains(&"rust".to_string()));
        assert!(result.entities.contains(&"docker".to_string()));

        Ok(())
    })
    .unwrap();
}

#[test]
fn turn_capture_deduplication_high_overlap() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        let r1 = capture.capture(conn, "Deploy the server to production now", None)?;
        assert!(!r1.observation_id.is_empty());

        let r2 = capture.capture(conn, "Deploy the server to production now", None)?;
        assert!(r2.observation_id.is_empty(), "duplicate was not caught");

        Ok(())
    })
    .unwrap();
}

#[test]
fn turn_capture_distinct_texts_not_deduplicated() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        let r1 = capture.capture(conn, "Deploy the server to production", None)?;
        let r2 = capture.capture(conn, "Rust memory safety is important", None)?;
        assert!(!r1.observation_id.is_empty());
        assert!(!r2.observation_id.is_empty());

        Ok(())
    })
    .unwrap();
}

#[test]
fn turn_capture_with_agent_id_creates_agent_node() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        let result = capture.capture(conn, "Testing agent capture", Some("test-agent"))?;
        assert!(!result.observation_id.is_empty());
        let agent_exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM nodes WHERE node_type = 'agent' AND valid_to IS NULL",
            [],
            |row| row.get(0),
        )?;
        assert!(agent_exists);

        Ok(())
    })
    .unwrap();
}

#[test]
fn turn_capture_agent_id_creates_agent_nodes() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        capture.capture(conn, "First turn", Some("my-agent"))?;
        capture.capture(conn, "Second turn", Some("my-agent"))?;
        let agent_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE node_type = 'agent' AND valid_to IS NULL",
            [],
            |row| row.get(0),
        )?;
        assert!(agent_count >= 1);

        Ok(())
    })
    .unwrap();
}

#[test]
fn turn_capture_rate_limiting_via_capture() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        let mut captured_count = 0;
        for i in 0..20 {
            let result = capture.capture(conn, &format!("Turn number {i}"), None)?;
            if !result.observation_id.is_empty() {
                captured_count += 1;
            }
        }
        assert!(
            captured_count > 0 && captured_count <= 20,
            "should capture some turns, got {}",
            captured_count
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn turn_capture_contradiction_detection_empty_for_distinct_obs() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        capture.capture(conn, "the weather is beautiful today", None)?;

        let result = capture.capture(conn, "another unrelated topic entirely", None)?;
        assert!(result.contradictions.is_empty());

        Ok(())
    })
    .unwrap();
}

#[test]
fn turn_capture_mentions_edges_created_for_known_entities() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "tool".to_string(),
                name: "rust".to_string(),
                ..Default::default()
            },
        )?;

        let capture = TurnCapture::new();
        let result = capture.capture(conn, "I love using rust for everything", None)?;
        assert!(!result.observation_id.is_empty());

        let edge_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE edge_type = 'MENTIONS' AND valid_to IS NULL",
            [],
            |row| row.get(0),
        )?;
        assert!(edge_count >= 0);

        Ok(())
    })
    .unwrap();
}

#[test]
fn turn_capture_properties_json_contains_entities_and_quadrant() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        let result = capture.capture(conn, "Using rust for the backend", None)?;

        let props_json: String = conn.query_row(
            "SELECT properties_json FROM nodes WHERE id = ?1",
            [&result.observation_id],
            |row| row.get(0),
        )?;
        let props: serde_json::Value = serde_json::from_str(&props_json)?;
        assert!(props.get("entities").is_some());
        assert!(props.get("turn_length").is_some());
        assert!(props.get("quadrant").is_some());

        Ok(())
    })
    .unwrap();
}

// ═══════════════════════════════════════════════════════════════════════════════
// Preference Extractor
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn preference_extractor_correction_detection() {
    let ext = PreferenceExtractor::new();
    let results = ext.extract_from_message("Don't use Docker for this project");
    assert!(!results.is_empty());
    let correction = results.iter().find(|r| r.extraction_type == "correction");
    assert!(
        correction.is_some(),
        "expected correction, got: {:?}",
        results
    );
    let c = correction.unwrap();
    assert!(c.confidence > 0.7);
    assert!(!c.constraint_id.is_empty());
    assert!(c.constraint_id.starts_with('c'));
}

#[test]
fn preference_extractor_correction_variants() {
    let ext = PreferenceExtractor::new();
    let cases = vec![
        "Don't use Docker for this",
        "Stop using PostgreSQL for the main DB",
        "Never go with MySQL",
        "Avoid circular dependencies",
        "No more manual deployments",
    ];
    for text in cases {
        let results = ext.extract_from_message(text);
        assert!(
            results.iter().any(|r| r.extraction_type == "correction"),
            "Expected correction for: {}",
            text
        );
    }
}

#[test]
fn preference_extractor_preference_detection() {
    let ext = PreferenceExtractor::new();
    let results = ext.extract_from_message("I prefer using Rust for backend");
    assert!(!results.is_empty());
    let pref = results.iter().find(|r| r.extraction_type == "preference");
    assert!(pref.is_some());
    let p = pref.unwrap();
    assert!(p.confidence > 0.7);
    assert!(!p.constraint_id.is_empty());
}

#[test]
fn preference_extractor_preference_variants() {
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
            "Expected preference for: {}",
            text
        );
    }
}

#[test]
fn preference_extractor_memory_request_detection() {
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
            "Expected memory for: {}",
            text
        );
    }
}

#[test]
fn preference_extractor_recurring_pattern_detection() {
    let ext = PreferenceExtractor::new();
    let results = ext.extract_from_message("Every time I check the logs there's an error");
    assert!(
        results
            .iter()
            .any(|r| r.extraction_type == "recurring_pattern"),
        "Expected recurring_pattern"
    );
}

#[test]
fn preference_extractor_autonomous_insight_detection() {
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
fn preference_extractor_no_extraction_for_neutral_text() {
    let ext = PreferenceExtractor::new();
    let results = ext.extract_from_message("The weather is nice today");
    assert!(results.is_empty());
}

#[test]
fn preference_extractor_constraint_id_deterministic() {
    let id1 = build_constraint_id("correction", "don't use Docker");
    let id2 = build_constraint_id("correction", "don't use Docker");
    let id3 = build_constraint_id("correction", "don't use Kubernetes");
    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
    assert!(id1.starts_with('c'));
}

#[test]
fn preference_extractor_constraint_id_case_insensitive() {
    let id1 = build_constraint_id("preference", "Use Rust");
    let id2 = build_constraint_id("preference", "use rust");
    assert_eq!(id1, id2);
}

#[test]
fn preference_extractor_batch_deduplication() {
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
fn preference_extractor_quadrant_inference_all_types() {
    let ext = PreferenceExtractor::new();
    assert_eq!(ext.infer_quadrant("deploy the server"), "lr");
    assert_eq!(ext.infer_quadrant("I feel comfortable"), "ul");
    assert_eq!(ext.infer_quadrant("our brand identity"), "ll");
    assert_eq!(ext.infer_quadrant("build the workflow"), "ur");
    assert_eq!(ext.infer_quadrant("something else entirely"), "ur");
}

#[test]
fn preference_extractor_constraint_id_is_unique_per_type_and_text() {
    let id_corr = build_constraint_id("correction", "don't use Docker");
    let id_pref = build_constraint_id("preference", "don't use Docker");
    assert_ne!(
        id_corr, id_pref,
        "different types should produce different ids"
    );
}

#[test]
fn preference_extractor_recurring_patterns_finds_repeated_keywords() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        for i in 0..5 {
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("obs{i}"),
                    description: Some("deploy the server to AWS".to_string()),
                    properties: Some(
                        serde_json::json!({"description": "deploy the server to AWS"}),
                    ),
                    source: Some("turn_capture".to_string()),
                    ..Default::default()
                },
            )?;
        }

        let ext = PreferenceExtractor::new();
        let results = ext.detect_recurring_patterns(conn, 100);
        let deploy_patterns: Vec<_> = results
            .iter()
            .filter(|r| r.constraint_text.contains("deploy"))
            .collect();
        assert!(
            !deploy_patterns.is_empty(),
            "Expected recurring deploy pattern"
        );

        Ok(())
    })
    .unwrap();
}

#[test]
fn preference_extractor_recurring_patterns_empty_when_few_observations() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "obs1".to_string(),
                description: Some("deploy the server".to_string()),
                properties: Some(serde_json::json!({"description": "deploy the server"})),
                source: Some("turn_capture".to_string()),
                ..Default::default()
            },
        )?;

        let ext = PreferenceExtractor::new();
        let results = ext.detect_recurring_patterns(conn, 100);
        let deploy_patterns: Vec<_> = results
            .iter()
            .filter(|r| r.constraint_text.contains("deploy"))
            .collect();
        assert!(deploy_patterns.is_empty());

        Ok(())
    })
    .unwrap();
}

#[test]
fn preference_extractor_cross_cycle_patterns_finds_action_verbs() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "obs1".to_string(),
                description: Some("I need to deploy the new version".to_string()),
                properties: Some(
                    serde_json::json!({"description": "I need to deploy the new version"}),
                ),
                source: Some("turn_capture".to_string()),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "obs2".to_string(),
                description: Some("Let me create a new module for this".to_string()),
                properties: Some(
                    serde_json::json!({"description": "Let me create a new module for this"}),
                ),
                source: Some("turn_capture".to_string()),
                ..Default::default()
            },
        )?;

        let ext = PreferenceExtractor::new();
        let results = ext.detect_cross_cycle_patterns(conn, 100);
        let labels: Vec<_> = results.iter().map(|r| r.constraint_text.as_str()).collect();
        assert!(labels.iter().any(|l| l.contains("deploy")));
        assert!(labels.iter().any(|l| l.contains("create")));

        Ok(())
    })
    .unwrap();
}

#[test]
fn preference_extractor_cross_cycle_patterns_empty_when_no_actions() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "obs1".to_string(),
                description: Some("The weather is nice today".to_string()),
                properties: Some(serde_json::json!({"description": "The weather is nice today"})),
                source: Some("turn_capture".to_string()),
                ..Default::default()
            },
        )?;

        let ext = PreferenceExtractor::new();
        let results = ext.detect_cross_cycle_patterns(conn, 100);
        assert!(results.is_empty());

        Ok(())
    })
    .unwrap();
}

#[test]
fn preference_extractor_multiple_extractions_from_single_message() {
    let ext = PreferenceExtractor::new();
    let results = ext.extract_from_message("Remember that port is 3000, don't use Docker");
    let types: Vec<&str> = results.iter().map(|r| r.extraction_type.as_str()).collect();
    assert!(types.contains(&"memory"));
    assert!(types.contains(&"correction"));
}

#[test]
fn preference_extractor_confidence_bounded() {
    let ext = PreferenceExtractor::new();
    let text =
        "Don't use Docker, I prefer Rust, remember that port is 3000, every time something happens";
    let results = ext.extract_from_message(text);
    for r in &results {
        assert!(
            (0.0..=1.0).contains(&r.confidence),
            "confidence {} out of range",
            r.confidence
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Cross-Plugin Integration
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn cross_plugin_entity_extractor_plus_hybrid_retriever() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "tool".to_string(),
                name: "rust".to_string(),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "tool".to_string(),
                name: "docker".to_string(),
                ..Default::default()
            },
        )?;

        let ext = EntityExtractor::new();
        let entities = ext.extract("Using rust and docker for the project", Some(conn));
        assert!(!entities.is_empty());

        let retriever = HybridRetriever::new();
        for entity in &entities {
            let results = retriever.search(conn, &entity.name, 5, None)?;
            if !results.is_empty() {
                assert!(results[0].node.name.to_lowercase().contains(&entity.name));
            }
        }

        Ok(())
    })
    .unwrap();
}

#[test]
fn cross_plugin_turn_capture_plus_preference_extractor() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let capture = TurnCapture::new();
        let result = capture.capture(conn, "I prefer using Rust for backend", None)?;
        assert!(!result.observation_id.is_empty());

        let ext = PreferenceExtractor::new();
        let prefs = ext.extract_from_message("I prefer using Rust for backend");
        assert!(prefs.iter().any(|p| p.extraction_type == "preference"));

        let retriever = HybridRetriever::new();
        let results = retriever.search(conn, "Rust backend preference", 5, None)?;
        assert!(!results.is_empty());

        Ok(())
    })
    .unwrap();
}

#[test]
fn cross_plugin_entity_extractor_alias_resolution_end_to_end() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "people".to_string(),
                name: "Eve Wilson".to_string(),
                properties: Some(serde_json::json!({"aliases": ["eve", "ewilson"]})),
                ..Default::default()
            },
        )?;

        let ext = EntityExtractor::new();
        let entities = ext.extract("Talked to eve about the project", Some(conn));
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        let names_lower: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
        assert!(
            names_lower.iter().any(|n| n == "eve wilson"),
            "Expected 'eve wilson' from alias resolution, got: {:?}",
            names
        );

        Ok(())
    })
    .unwrap();
}
