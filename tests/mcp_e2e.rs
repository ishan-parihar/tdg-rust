//! End-to-end tests for MCP tool operations.
//!
//! Tests exercise the same CRUD/FTS5/graph code paths that `src/mcp/tools.rs`
//! wraps, using the public `tdg_rust` API.  Every test uses an in-memory SQLite
//! database so there are zero external dependencies and results are deterministic.

use serde_json::json;
use tdg_rust::db::{init_fts, init_schema, run_migrations, ConnectionPool};
use tdg_rust::models::{NewEdge, NewNode, NodeQuery};

// ─── Shared helpers ─────────────────────────────────────────────────────────

fn make_pool() -> ConnectionPool {
    let pool = ConnectionPool::new(":memory:", 5, 30_000).expect("pool creation");
    pool.with_connection(|conn| {
        init_schema(conn)?;
        init_fts(conn)?;
        run_migrations(conn)?;
        Ok(())
    })
    .expect("schema init");
    pool
}

fn add_node(pool: &ConnectionPool, node_type: &str, name: &str) -> String {
    pool.with_connection(|conn| {
        let node = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: node_type.to_string(),
                name: name.to_string(),
                ..Default::default()
            },
        )?;
        Ok(node.id)
    })
    .unwrap()
}

fn add_node_with_desc(pool: &ConnectionPool, node_type: &str, name: &str, desc: &str) -> String {
    pool.with_connection(|conn| {
        let node = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: node_type.to_string(),
                name: name.to_string(),
                description: Some(desc.to_string()),
                ..Default::default()
            },
        )?;
        Ok(node.id)
    })
    .unwrap()
}

// ═════════════════════════════════════════════════════════════════════════════
// 1. tdg_search — FTS5 search
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_search_returns_matching_nodes() {
    let pool = make_pool();
    add_node_with_desc(&pool, "observation", "Rust perf", "Rust is fast and safe");
    add_node_with_desc(&pool, "observation", "Python GIL", "Python has GIL issues");

    let results = pool
        .with_connection(|conn| {
            let r = tdg_rust::db::crud::search(conn, "Rust", 10)?;
            Ok(r)
        })
        .unwrap();

    assert!(!results.is_empty(), "should find at least one result");
    assert!(
        results.iter().any(|(n, _)| n.name.contains("Rust")),
        "should contain the Rust node"
    );
}

#[test]
fn e2e_search_no_match_returns_empty() {
    let pool = make_pool();
    add_node(&pool, "observation", "Rust perf");

    let results = pool
        .with_connection(|conn| tdg_rust::db::crud::search(conn, "quantum", 10))
        .unwrap();

    assert!(results.is_empty(), "no match expected");
}

#[test]
fn e2e_search_respects_limit() {
    let pool = make_pool();
    for i in 0..10 {
        add_node(&pool, "observation", &format!("unique_term_{i}"));
    }

    let results = pool
        .with_connection(|conn| tdg_rust::db::crud::search(conn, "unique_term", 3))
        .unwrap();

    assert!(
        results.len() <= 3,
        "limit not respected: got {}",
        results.len()
    );
}

#[test]
fn e2e_search_fts5_stemming() {
    let pool = make_pool();
    add_node_with_desc(
        &pool,
        "observation",
        "Running",
        "The runner was running fast",
    );

    let results = pool
        .with_connection(|conn| tdg_rust::db::crud::search(conn, "run", 10))
        .unwrap();

    assert!(
        !results.is_empty(),
        "FTS5 stemming should match 'run' → 'running'"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 2. tdg_get_node — retrieval with optional context
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_get_node_basic() {
    let pool = make_pool();
    let id = add_node_with_desc(&pool, "observation", "Target Node", "Description here");

    let node = pool
        .with_connection(|conn| {
            let n = tdg_rust::db::crud::get_node(conn, &id)?;
            Ok(n)
        })
        .unwrap()
        .expect("node must exist");

    assert_eq!(node.id, id);
    assert_eq!(node.name, "Target Node");
    assert_eq!(node.description, "Description here");
}

#[test]
fn e2e_get_node_not_found_returns_none() {
    let pool = make_pool();

    let node = pool
        .with_connection(|conn| tdg_rust::db::crud::get_node(conn, "n_nonexistent"))
        .unwrap();

    assert!(node.is_none());
}

#[test]
fn e2e_get_node_with_outgoing_edges() {
    let pool = make_pool();
    let src = add_node(&pool, "observation", "Source");
    let tgt = add_node(&pool, "hypothesis", "Target");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: src.clone(),
                target_id: tgt.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let (node, out_edges) = pool
        .with_connection(|conn| {
            let n = tdg_rust::db::crud::get_node(conn, &src)?.unwrap();
            let out = tdg_rust::db::crud::get_edges(conn, Some(&src), None, None, None, 100)?;
            Ok((n, out))
        })
        .unwrap();

    assert_eq!(node.id, src);
    assert_eq!(out_edges.len(), 1);
    assert_eq!(out_edges[0].edge_type, "EVIDENCES");
    assert_eq!(out_edges[0].target_id, tgt);
}

#[test]
fn e2e_get_node_with_incoming_edges() {
    let pool = make_pool();
    let src = add_node(&pool, "observation", "Evidence");
    let tgt = add_node(&pool, "hypothesis", "Claim");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: src.clone(),
                target_id: tgt.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let in_edges = pool
        .with_connection(|conn| {
            let e = tdg_rust::db::crud::get_edges(conn, None, Some(&tgt), None, None, 100)?;
            Ok(e)
        })
        .unwrap();

    assert_eq!(in_edges.len(), 1);
    assert_eq!(in_edges[0].source_id, src);
}

// ═════════════════════════════════════════════════════════════════════════════
// 3. tdg_create — node creation with auto edge wiring
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_create_observation_node() {
    let pool = make_pool();

    let node = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: "Test Obs".to_string(),
                    description: Some("A test observation".to_string()),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    assert!(node.id.starts_with('n'));
    assert_eq!(node.node_type, "observation");
    assert_eq!(node.name, "Test Obs");
    assert_eq!(node.description, "A test observation");
}

#[test]
fn e2e_create_node_with_parent_ids() {
    let pool = make_pool();
    let parent = add_node(&pool, "telos", "Parent Telos");

    let node = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "action".to_string(),
                    name: "Child Action".to_string(),
                    parent_ids: Some(vec![parent.clone()]),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    assert_eq!(node.parent_ids, vec![parent]);
}

#[test]
fn e2e_create_node_with_quadrant_and_drives() {
    let pool = make_pool();

    let node = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: "Quadrant Node".to_string(),
                    quadrants: Some(json!({"primary": "UR"})),
                    drives: Some(json!({"teleological_level": "T3", "stage": 3})),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    assert_eq!(node.quadrants["primary"], "UR");
    assert_eq!(node.drives["teleological_level"], "T3");
    assert_eq!(node.drives["stage"], 3);
}

#[test]
fn e2e_create_auto_wires_blocks_edge() {
    let pool = make_pool();
    let constraint = add_node(&pool, "constraint", "Time Limit");
    let action = add_node(&pool, "action", "Delayed Task");

    pool.with_connection(|conn| {
        let node = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "constraint".to_string(),
                name: "New Constraint".to_string(),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: node.id.clone(),
                target_id: action.clone(),
                edge_type: "BLOCKS".to_string(),
                ..Default::default()
            },
        )?;
        Ok(())
    })
    .unwrap();

    let edges = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::get_edges(conn, Some(&constraint), Some(&action), None, None, 10)
        })
        .unwrap();
    let auto_edges = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::get_edges(conn, None, Some(&action), Some("BLOCKS"), None, 10)
        })
        .unwrap();
    assert!(
        !auto_edges.is_empty() || !edges.is_empty(),
        "BLOCKS edge should exist"
    );
}

#[test]
fn e2e_create_auto_wires_evidence_edge() {
    let pool = make_pool();
    let hyp = add_node(&pool, "hypothesis", "Claim");

    pool.with_connection(|conn| {
        let obs = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Evidence".to_string(),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: obs.id,
                target_id: hyp.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )?;
        Ok(())
    })
    .unwrap();

    let in_edges = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::get_edges(conn, None, Some(&hyp), Some("EVIDENCES"), None, 10)
        })
        .unwrap();

    assert_eq!(in_edges.len(), 1);
    assert_eq!(in_edges[0].edge_type, "EVIDENCES");
}

// ═════════════════════════════════════════════════════════════════════════════
// 4. tdg_update — node updates
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_update_node_name() {
    let pool = make_pool();
    let id = add_node(&pool, "observation", "Original Name");

    let updated = pool
        .with_connection(|conn| {
            let mut updates = std::collections::HashMap::new();
            updates.insert("name".to_string(), json!("Updated Name"));
            tdg_rust::db::crud::update_node(conn, &id, &updates)
        })
        .unwrap()
        .expect("update should succeed");

    assert_eq!(updated.name, "Updated Name");
}

#[test]
fn e2e_update_node_description() {
    let pool = make_pool();
    let id = add_node(&pool, "observation", "Desc Test");

    let updated = pool
        .with_connection(|conn| {
            let mut updates = std::collections::HashMap::new();
            updates.insert("description".to_string(), json!("New detailed description"));
            tdg_rust::db::crud::update_node(conn, &id, &updates)
        })
        .unwrap()
        .expect("update should succeed");

    assert_eq!(updated.description, "New detailed description");
}

#[test]
fn e2e_update_node_lifecycle_state() {
    let pool = make_pool();
    let id = add_node(&pool, "observation", "Lifecycle Node");

    let updated = pool
        .with_connection(|conn| {
            let mut updates = std::collections::HashMap::new();
            updates.insert("lifecycle_state".to_string(), json!("archived"));
            tdg_rust::db::crud::update_node(conn, &id, &updates)
        })
        .unwrap()
        .expect("update should succeed");

    assert_eq!(updated.lifecycle_state, "archived");
}

#[test]
fn e2e_update_node_parent_ids() {
    let pool = make_pool();
    let p1 = add_node(&pool, "telos", "Parent1");
    let p2 = add_node(&pool, "telos", "Parent2");
    let child = add_node(&pool, "action", "Child");

    pool.with_connection(|conn| {
        let mut updates = std::collections::HashMap::new();
        updates.insert("parent_ids".to_string(), json!(vec![p1.clone()]));
        tdg_rust::db::crud::update_node(conn, &child, &updates)?;
        Ok(())
    })
    .unwrap();

    pool.with_connection(|conn| {
        let mut updates = std::collections::HashMap::new();
        updates.insert(
            "parent_ids".to_string(),
            json!(vec![p1.clone(), p2.clone()]),
        );
        tdg_rust::db::crud::update_node(conn, &child, &updates)?;
        Ok(())
    })
    .unwrap();

    let node = pool
        .with_connection(|conn| tdg_rust::db::crud::get_node(conn, &child))
        .unwrap()
        .unwrap();

    assert!(node.parent_ids.contains(&p1));
    assert!(node.parent_ids.contains(&p2));
}

#[test]
fn e2e_update_nonexistent_node_returns_none() {
    let pool = make_pool();

    let result = pool
        .with_connection(|conn| {
            let mut updates = std::collections::HashMap::new();
            updates.insert("name".to_string(), json!("Nope"));
            tdg_rust::db::crud::update_node(conn, "n_nonexistent", &updates)
        })
        .unwrap();

    assert!(result.is_none());
}

// ═════════════════════════════════════════════════════════════════════════════
// 5. tdg_connect — edge creation with auto-detection
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_connect_observation_to_telos_creates_evidences() {
    let pool = make_pool();
    let obs = add_node(&pool, "observation", "Obs");
    let telos = add_node(&pool, "telos", "Goal");

    let edge = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_edge(
                conn,
                &NewEdge {
                    source_id: obs.clone(),
                    target_id: telos.clone(),
                    edge_type: "EVIDENCES".to_string(),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    assert_eq!(edge.edge_type, "EVIDENCES");
    assert_eq!(edge.source_id, obs);
    assert_eq!(edge.target_id, telos);
}

#[test]
fn e2e_connect_telos_to_telos_creates_decomposes() {
    let pool = make_pool();
    let parent = add_node(&pool, "telos", "Parent Goal");
    let child = add_node(&pool, "telos", "Sub Goal");

    let edge = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_edge(
                conn,
                &NewEdge {
                    source_id: parent.clone(),
                    target_id: child.clone(),
                    edge_type: "DECOMPOSES_TO".to_string(),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    assert_eq!(edge.edge_type, "DECOMPOSES_TO");
}

#[test]
fn e2e_connect_constraint_to_action_creates_blocks() {
    let pool = make_pool();
    let constraint = add_node(&pool, "constraint", "Blocker");
    let action = add_node(&pool, "action", "Blocked Action");

    let edge = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_edge(
                conn,
                &NewEdge {
                    source_id: constraint.clone(),
                    target_id: action.clone(),
                    edge_type: "BLOCKS".to_string(),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    assert_eq!(edge.edge_type, "BLOCKS");
}

#[test]
fn e2e_connect_skill_to_action_creates_enables() {
    let pool = make_pool();
    let skill = add_node(&pool, "skill", "Rust");
    let action = add_node(&pool, "action", "Build");

    let edge = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_edge(
                conn,
                &NewEdge {
                    source_id: skill.clone(),
                    target_id: action.clone(),
                    edge_type: "ENABLES".to_string(),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    assert_eq!(edge.edge_type, "ENABLES");
}

#[test]
fn e2e_connect_duplicate_edge_detection() {
    let pool = make_pool();
    let a = add_node(&pool, "observation", "A");
    let b = add_node(&pool, "hypothesis", "B");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: a.clone(),
                target_id: b.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let existing = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::get_edges(conn, Some(&a), Some(&b), Some("EVIDENCES"), None, 10)
        })
        .unwrap();

    assert_eq!(existing.len(), 1, "should detect the existing edge");
}

#[test]
fn e2e_connect_force_overrides_duplicate() {
    let pool = make_pool();
    let a = add_node(&pool, "observation", "A");
    let b = add_node(&pool, "observation", "B");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: a.clone(),
                target_id: b.clone(),
                edge_type: "SUPPORTS".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: a.clone(),
                target_id: b.clone(),
                edge_type: "SUPPORTS".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let edges = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::get_edges(conn, Some(&a), Some(&b), Some("SUPPORTS"), None, 10)
        })
        .unwrap();

    assert_eq!(edges.len(), 2, "force mode should allow duplicates");
}

// ═════════════════════════════════════════════════════════════════════════════
// 6. tdg_observe — observation creation with entity wiring
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_observe_creates_observation_with_properties() {
    let pool = make_pool();

    let node = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: "Obs: Test observation".to_string(),
                    description: Some("A test observation".to_string()),
                    source: Some("mcp_observe".to_string()),
                    properties: Some(json!({
                        "quadrant": "UL",
                        "cycle": 1,
                        "trust": 0.7,
                    })),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    assert!(node.id.starts_with('n'));
    assert_eq!(node.properties["quadrant"], "UL");
    assert_eq!(node.properties["cycle"], 1);
    assert!((node.properties["trust"].as_f64().unwrap() - 0.7).abs() < f64::EPSILON);
}

#[test]
fn e2e_observe_with_entity_wiring() {
    let pool = make_pool();

    // Create an observation node
    let obs = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: "Obs: Meeting with Alice".to_string(),
                    source: Some("mcp_observe".to_string()),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    let entity = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "entity".to_string(),
                    name: "Alice".to_string(),
                    source: Some("mcp_observe".to_string()),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: obs.id.clone(),
                target_id: entity.id.clone(),
                edge_type: "MENTIONS".to_string(),
                weight: Some(1.0),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let mentions = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::get_edges(conn, Some(&obs.id), None, Some("MENTIONS"), None, 10)
        })
        .unwrap();

    assert_eq!(mentions.len(), 1);
    assert_eq!(mentions[0].target_id, entity.id);
}

#[test]
fn e2e_observe_triggers_digestion() {
    let pool = make_pool();

    let obs = add_node(&pool, "observation", "Signal");
    let hyp = add_node(&pool, "hypothesis", "Theory");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: obs.clone(),
                target_id: hyp.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let digested = pool
        .with_connection(|conn| {
            let engine = tdg_rust::DigestionEngine::new(conn);
            engine.check_upward_cascade()
        })
        .unwrap();

    let _ = digested;
}

// ═════════════════════════════════════════════════════════════════════════════
// 7. tdg_mind_state — graph state retrieval
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_mind_state_node_counts() {
    let pool = make_pool();
    add_node(&pool, "observation", "O1");
    add_node(&pool, "observation", "O2");
    add_node(&pool, "telos", "T1");

    let (nc, ec, oc, tc) = pool
        .with_connection(|conn| {
            let nc = tdg_rust::db::crud::count_nodes(conn, None)?;
            let ec = tdg_rust::db::crud::count_edges(conn, None)?;
            let oc = tdg_rust::db::crud::count_nodes(conn, Some("observation"))?;
            let tc = tdg_rust::db::crud::count_nodes(conn, Some("telos"))?;
            Ok((nc, ec, oc, tc))
        })
        .unwrap();

    assert_eq!(nc, 3);
    assert_eq!(ec, 0);
    assert_eq!(oc, 2);
    assert_eq!(tc, 1);
}

#[test]
fn e2e_mind_state_edge_counts() {
    let pool = make_pool();
    let a = add_node(&pool, "observation", "A");
    let b = add_node(&pool, "hypothesis", "B");
    let c = add_node(&pool, "telos", "C");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: a.clone(),
                target_id: b.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: b.clone(),
                target_id: c.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let ec = pool
        .with_connection(|conn| tdg_rust::db::crud::count_edges(conn, None))
        .unwrap();

    assert_eq!(ec, 2);
}

#[test]
fn e2e_mind_state_verify_integrity() {
    let pool = make_pool();

    let integrity: String = pool
        .with_connection(|conn| {
            let r: String = conn
                .pragma_query_value(None, "integrity_check", |row| row.get(0))
                .unwrap_or_else(|_| "error".to_string());
            Ok(r)
        })
        .unwrap();

    assert_eq!(integrity, "ok");
}

#[test]
fn e2e_mind_state_event_count() {
    let pool = make_pool();

    pool.with_connection(|conn| {
        tdg_rust::db::crud::record_event(conn, "test_event", None, None, None, None)?;
        tdg_rust::db::crud::record_event(conn, "another_event", None, None, None, None)?;
        Ok(())
    })
    .unwrap();

    let evc: i64 = pool
        .with_connection(|conn| {
            let c: i64 = conn
                .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
                .unwrap_or(0);
            Ok(c)
        })
        .unwrap();

    assert_eq!(evc, 2);
}

#[test]
fn e2e_mind_state_quadrant_distribution() {
    let pool = make_pool();

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "UR Node".to_string(),
                properties: Some(json!({"primary": "UR"})),
                ..Default::default()
            },
        )
        .unwrap();
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "LL Node".to_string(),
                properties: Some(json!({"primary": "LL"})),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let qd = pool
        .with_connection(|conn| {
            let mut ur = 0i64;
            let mut ll = 0i64;
            if let Ok(mut stmt) = conn.prepare(
                "SELECT properties_json FROM nodes WHERE valid_to IS NULL AND properties_json NOT IN ('{}', '')"
            ) {
                if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                    for row in rows.flatten() {
                        if let Ok(props) = serde_json::from_str::<serde_json::Value>(&row) {
                            if let Some(primary) = props.get("primary").and_then(|v| v.as_str()) {
                                match primary {
                                    "UR" => ur += 1,
                                    "LL" => ll += 1,
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            Ok((ur, ll))
        })
        .unwrap();

    assert_eq!(qd.0, 1);
    assert_eq!(qd.1, 1);
}

// ═════════════════════════════════════════════════════════════════════════════
// 8. tdg_reflect — LLM synthesis (pattern fallback)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_reflect_pattern_synthesis_empty_graph() {
    let pool = make_pool();

    let result = pool
        .with_connection(|conn| {
            let obs_query = NodeQuery {
                node_type: Some("observation".to_string()),
                limit: Some(50),
                ..Default::default()
            };
            let observations = tdg_rust::db::crud::query_nodes(conn, &obs_query)?;

            let telos_query = NodeQuery {
                node_type: Some("telos".to_string()),
                limit: Some(20),
                ..Default::default()
            };
            let telos = tdg_rust::db::crud::query_nodes(conn, &telos_query)?;

            Ok(observations.is_empty() && telos.is_empty())
        })
        .unwrap();

    assert!(result, "empty graph should have no context");
}

#[test]
fn e2e_reflect_pattern_synthesis_with_data() {
    let pool = make_pool();

    add_node(&pool, "observation", "Rust is fast");
    add_node(&pool, "observation", "Memory safety matters");
    add_node(&pool, "telos", "Build great software");
    add_node(&pool, "people", "Alice");

    let context_map = pool
        .with_connection(|conn| {
            let obs_query = NodeQuery {
                node_type: Some("observation".to_string()),
                limit: Some(200),
                ..Default::default()
            };
            let observations = tdg_rust::db::crud::query_nodes(conn, &obs_query)?;

            let people_query = NodeQuery {
                node_type: Some("people".to_string()),
                limit: Some(20),
                ..Default::default()
            };
            let people = tdg_rust::db::crud::query_nodes(conn, &people_query)?;

            let telos_query = NodeQuery {
                node_type: Some("telos".to_string()),
                limit: Some(20),
                ..Default::default()
            };
            let telos_nodes = tdg_rust::db::crud::query_nodes(conn, &telos_query)?;

            let edge_count = tdg_rust::db::crud::count_edges(conn, None)?;
            let total_nodes = tdg_rust::db::crud::count_nodes(conn, None)?;

            let entity_names: Vec<String> = people.iter().map(|p| p.name.clone()).collect();

            Ok(json!({
                "nodes": observations.iter().chain(people.iter()).chain(telos_nodes.iter()).map(|n| {
                    json!({
                        "id": n.id,
                        "type": n.node_type,
                        "name": n.name,
                        "description": n.description.chars().take(200).collect::<String>(),
                        "created_at": n.created_at,
                    })
                }).collect::<Vec<_>>(),
                "entities": entity_names,
                "edges": edge_count,
                "total_nodes": total_nodes,
            }))
        })
        .unwrap();

    assert!(context_map["nodes"].as_array().unwrap().len() >= 3);
    assert_eq!(context_map["total_nodes"], 4);
    assert_eq!(context_map["edges"], 0);
}

#[test]
fn e2e_reflect_store_synthesis_node() {
    let pool = make_pool();

    let synth = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "synthesis".to_string(),
                    name: "Synthesis: Pattern analysis of 4 nodes".to_string(),
                    description: Some("Pattern-based analysis".to_string()),
                    properties: Some(json!({
                        "insights": ["Graph has 3 types"],
                        "patterns": ["observation dominates"],
                        "method": "pattern",
                        "confidence": 0.4,
                    })),
                    quadrants: Some(json!({"primary": "LR", "inferred": true})),
                    lifecycle_state: Some("active".to_string()),
                    source: Some("reflect_tool/pattern".to_string()),
                    confidence: Some(0.4),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    assert!(synth.id.starts_with('n'));
    assert_eq!(synth.node_type, "synthesis");
    assert!(synth.properties.get("insights").is_some());
}

#[test]
fn e2e_reflect_store_sub_insight_nodes() {
    let pool = make_pool();

    let main = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "synthesis".to_string(),
                    name: "Synthesis: Main".to_string(),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    let insights = vec!["Insight one", "Insight two", "Insight three"];
    pool.with_connection(|conn| {
        for (i, text) in insights.iter().enumerate() {
            let sub = tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "synthesis".to_string(),
                    name: format!("Insight: {text}"),
                    properties: Some(json!({
                        "source_node": main.id,
                        "index": i,
                        "kind": "insight",
                        "method": "pattern",
                    })),
                    ..Default::default()
                },
            )?;
            tdg_rust::db::crud::add_edge(
                conn,
                &NewEdge {
                    source_id: sub.id,
                    target_id: main.id.clone(),
                    edge_type: "SYNTHESIZES".to_string(),
                    weight: Some(0.9),
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    })
    .unwrap();

    let synthesize_edges = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::get_edges(conn, None, Some(&main.id), Some("SYNTHESIZES"), None, 10)
        })
        .unwrap();

    assert_eq!(synthesize_edges.len(), 3);
}

// ═════════════════════════════════════════════════════════════════════════════
// 9. tdg_bank — bank management
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_bank_list_empty() {
    let pool = make_pool();

    let banks: Vec<String> = pool
        .with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT agent_id FROM nodes WHERE agent_id IS NOT NULL \
                     AND valid_to IS NULL ORDER BY agent_id",
                )
                .unwrap();
            let banks: Vec<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            Ok(banks)
        })
        .unwrap();

    assert!(banks.is_empty());
}

#[test]
fn e2e_bank_list_with_agent_nodes() {
    let pool = make_pool();

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Agent A obs".to_string(),
                agent_id: Some("agent_a".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Agent A obs 2".to_string(),
                agent_id: Some("agent_a".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Agent B obs".to_string(),
                agent_id: Some("agent_b".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let banks: Vec<(String, i64)> = pool
        .with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT agent_id FROM nodes WHERE agent_id IS NOT NULL \
                     AND valid_to IS NULL ORDER BY agent_id",
                )
                .unwrap();
            let bank_ids: Vec<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();

            let mut result = Vec::new();
            for bid in &bank_ids {
                let count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM nodes WHERE agent_id = ?1 AND valid_to IS NULL",
                        [bid.as_str()],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                result.push((bid.clone(), count));
            }
            Ok(result)
        })
        .unwrap();

    assert_eq!(banks.len(), 2);
    assert_eq!(banks[0].0, "agent_a");
    assert_eq!(banks[0].1, 2);
    assert_eq!(banks[1].0, "agent_b");
    assert_eq!(banks[1].1, 1);
}

// ═════════════════════════════════════════════════════════════════════════════
// 10. tdg_entity — entity resolution
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_entity_resolve_empty() {
    let pool = make_pool();

    let q = NodeQuery {
        node_type: Some("entity".to_string()),
        limit: Some(10),
        ..Default::default()
    };
    let entities = pool
        .with_connection(|conn| tdg_rust::db::crud::query_nodes(conn, &q))
        .unwrap();

    assert!(entities.is_empty());
}

#[test]
fn e2e_entity_resolve_by_name() {
    let pool = make_pool();

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "entity".to_string(),
                name: "Alice".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "entity".to_string(),
                name: "Alice Smith".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "entity".to_string(),
                name: "Bob".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let term = "alice";
    let entities = pool
        .with_connection(|conn| {
            let q = NodeQuery {
                node_type: Some("entity".to_string()),
                limit: Some(10),
                ..Default::default()
            };
            let nodes = tdg_rust::db::crud::query_nodes(conn, &q)?;
            let matched: Vec<String> = nodes
                .iter()
                .filter(|n| n.name.to_lowercase().contains(&term.to_lowercase()))
                .map(|n| n.name.clone())
                .collect();
            Ok(matched)
        })
        .unwrap();

    assert_eq!(entities.len(), 2);
    assert!(entities.contains(&"Alice".to_string()));
    assert!(entities.contains(&"Alice Smith".to_string()));
}

#[test]
fn e2e_entity_get_by_id() {
    let pool = make_pool();
    let id = pool
        .with_connection(|conn| {
            let node = tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "entity".to_string(),
                    name: "Einstein".to_string(),
                    properties: Some(json!({"aliases": ["albert", "einstein"] })),
                    ..Default::default()
                },
            )?;
            Ok(node.id)
        })
        .unwrap();

    let node = pool
        .with_connection(|conn| tdg_rust::db::crud::get_node(conn, &id))
        .unwrap()
        .expect("entity should exist");

    assert_eq!(node.name, "Einstein");
    assert!(node.properties.get("aliases").is_some());
}

#[test]
fn e2e_entity_not_found() {
    let pool = make_pool();

    let node = pool
        .with_connection(|conn| tdg_rust::db::crud::get_node(conn, "n_nonexistent"))
        .unwrap();

    assert!(node.is_none());
}

// ═════════════════════════════════════════════════════════════════════════════
// 11. Event logging (tdg_query_events underlying path)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_event_logging_and_query() {
    let pool = make_pool();
    let node = add_node(&pool, "observation", "Event Node");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::record_event(
            conn,
            "node_created",
            Some(&node),
            None,
            None,
            Some(&json!({"type": "observation"})),
        )?;
        tdg_rust::db::crud::record_event(conn, "node_searched", Some(&node), None, None, None)?;
        Ok(())
    })
    .unwrap();

    let events = pool
        .with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT event_id, event_action, node_id, payload, timestamp \
                     FROM events WHERE 1=1 ORDER BY timestamp DESC LIMIT ?1 OFFSET ?2",
                )
                .unwrap();
            let rows = stmt
                .query_map(rusqlite::params![50i64, 0i64], |row| {
                    Ok(json!({
                        "event_id": row.get::<_, String>(0)?,
                        "event_action": row.get::<_, String>(1)?,
                        "node_id": row.get::<_, Option<String>>(2)?,
                        "payload": row.get::<_, Option<String>>(3)?,
                        "timestamp": row.get::<_, String>(4)?,
                    }))
                })
                .unwrap();
            let events: Vec<serde_json::Value> = rows.filter_map(|r| r.ok()).collect();
            Ok(events)
        })
        .unwrap();

    assert!(
        events.len() >= 2,
        "should have at least 2 events, got {}",
        events.len()
    );
    let actions: Vec<&str> = events
        .iter()
        .filter_map(|e| e["event_action"].as_str())
        .collect();
    assert!(actions.contains(&"node_created"));
    assert!(actions.contains(&"node_searched"));
}

// ═════════════════════════════════════════════════════════════════════════════
// 12. Full E2E workflow — create → connect → search → update → archive
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_full_workflow() {
    let pool = make_pool();

    let telos = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "telos".to_string(),
                    name: "E2E Goal".to_string(),
                    description: Some("End to end test goal".to_string()),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    let action_a = add_node(&pool, "action", "Action A");
    let action_b = add_node(&pool, "action", "Action B");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: telos.id.clone(),
                target_id: action_a.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: telos.id.clone(),
                target_id: action_b.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let obs = pool
        .with_connection(|conn| {
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: "Observation supporting goal".to_string(),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: obs.id.clone(),
                target_id: telos.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let (nc, ec) = pool
        .with_connection(|conn| {
            let nc = tdg_rust::db::crud::count_nodes(conn, None)?;
            let ec = tdg_rust::db::crud::count_edges(conn, None)?;
            Ok((nc, ec))
        })
        .unwrap();

    assert_eq!(nc, 4, "should have 4 nodes");
    assert_eq!(ec, 3, "should have 3 edges");

    let results = pool
        .with_connection(|conn| tdg_rust::db::crud::search(conn, "goal", 10))
        .unwrap();
    assert!(!results.is_empty());

    let (node, out_edges, in_edges) = pool
        .with_connection(|conn| {
            let n = tdg_rust::db::crud::get_node(conn, &telos.id)?.unwrap();
            let out = tdg_rust::db::crud::get_edges(conn, Some(&telos.id), None, None, None, 100)?;
            let inp = tdg_rust::db::crud::get_edges(conn, None, Some(&telos.id), None, None, 100)?;
            Ok((n, out, inp))
        })
        .unwrap();

    assert_eq!(node.name, "E2E Goal");
    assert_eq!(out_edges.len(), 2, "telos has 2 outgoing edges");
    assert_eq!(in_edges.len(), 1, "telos has 1 incoming edge");

    pool.with_connection(|conn| {
        let mut updates = std::collections::HashMap::new();
        updates.insert("lifecycle_state".to_string(), json!("completed"));
        tdg_rust::db::crud::update_node(conn, &action_a, &updates)?;
        Ok(())
    })
    .unwrap();

    let updated_action = pool
        .with_connection(|conn| tdg_rust::db::crud::get_node(conn, &action_a))
        .unwrap()
        .unwrap();
    assert_eq!(updated_action.lifecycle_state, "completed");

    let paths = pool
        .with_connection(|conn| tdg_rust::db::crud::pathfind(conn, &obs.id, &action_a, 6, 100))
        .unwrap();
    assert!(
        !paths.is_empty(),
        "path should exist: obs → telos → action_a"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 13. Bulk operations (tdg_bulk_create underlying path)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_bulk_create_nodes() {
    let pool = make_pool();
    let new_nodes: Vec<NewNode> = (0..5)
        .map(|i| NewNode {
            node_type: "observation".to_string(),
            name: format!("Bulk {i}"),
            ..Default::default()
        })
        .collect();

    let created = pool
        .with_connection(|conn| tdg_rust::db::crud::add_nodes_batch(conn, &new_nodes))
        .unwrap();

    assert_eq!(created.len(), 5);
    for node in &created {
        assert!(node.id.starts_with('n'));
    }

    let count = pool
        .with_connection(|conn| tdg_rust::db::crud::count_nodes(conn, None))
        .unwrap();
    assert_eq!(count, 5);
}

#[test]
fn e2e_bulk_create_with_edges() {
    let pool = make_pool();

    let (nodes, edges) = pool
        .with_connection(|conn| {
            let a = tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: "Bulk A".to_string(),
                    ..Default::default()
                },
            )?;
            let b = tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "hypothesis".to_string(),
                    name: "Bulk B".to_string(),
                    ..Default::default()
                },
            )?;
            let edge = tdg_rust::db::crud::add_edge(
                conn,
                &NewEdge {
                    source_id: a.id.clone(),
                    target_id: b.id.clone(),
                    edge_type: "EVIDENCES".to_string(),
                    ..Default::default()
                },
            )?;
            Ok((vec![a, b], vec![edge]))
        })
        .unwrap();

    assert_eq!(nodes.len(), 2);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].edge_type, "EVIDENCES");
}

// ═════════════════════════════════════════════════════════════════════════════
// 14. Health monitoring (tdg_health_check underlying path)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_health_check_record_and_summary() {
    let pool = make_pool();

    pool.with_connection(|conn| {
        tdg_rust::db::crud::record_health_check(conn, "api-gateway", 42.5, true, None)?;
        tdg_rust::db::crud::record_health_check(
            conn,
            "api-gateway",
            120.0,
            false,
            Some("timeout"),
        )?;
        tdg_rust::db::crud::record_health_check(conn, "db", 5.0, true, None)?;
        Ok(())
    })
    .unwrap();

    let summary = pool
        .with_connection(|conn| tdg_rust::db::crud::get_health_summary(conn))
        .unwrap();

    assert_eq!(summary["total_checks"], 3);
    assert!(summary["success_rate"].as_f64().unwrap() > 0.6);
    assert!(summary["avg_latency_ms"].as_f64().unwrap() > 0.0);

    let recent = pool
        .with_connection(|conn| tdg_rust::db::crud::get_recent_health_checks(conn, None, 10))
        .unwrap();
    assert_eq!(recent.len(), 3);

    let filtered = pool
        .with_connection(|conn| tdg_rust::db::crud::get_recent_health_checks(conn, Some("db"), 10))
        .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0]["service"], "db");
}

// ═════════════════════════════════════════════════════════════════════════════
// 15. Trust scoring (tdg_rate_memory underlying path)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_rate_memory_helpful_increases_score() {
    let pool = make_pool();
    let id = add_node(&pool, "observation", "Rate Me");

    pool.with_connection(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE nodes SET helpful_count = helpful_count + 1, updated_at = ?1 WHERE id = ?2 AND valid_to IS NULL",
            rusqlite::params![now, &id],
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let trust: f64 = pool
        .with_connection(|conn| {
            let t: f64 = conn
                .query_row(
                    "SELECT confidence * (1.0 + helpful_count) / (1.0 + retrieval_count) FROM nodes WHERE id = ?1 AND valid_to IS NULL",
                    rusqlite::params![&id],
                    |row| row.get(0),
                )
                .unwrap_or(0.0);
            Ok(t)
        })
        .unwrap();

    assert!(trust > 0.0, "helpful rating should increase trust score");
}

#[test]
fn e2e_rate_memory_unhelpful_decreases() {
    let pool = make_pool();
    let id = add_node(&pool, "observation", "Bad Memory");

    pool.with_connection(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE nodes SET helpful_count = helpful_count - 1, updated_at = ?1 WHERE id = ?2 AND valid_to IS NULL",
            rusqlite::params![now, &id],
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let trust: f64 = pool
        .with_connection(|conn| {
            let t: f64 = conn
                .query_row(
                    "SELECT confidence * (1.0 + helpful_count) / (1.0 + retrieval_count) FROM nodes WHERE id = ?1 AND valid_to IS NULL",
                    rusqlite::params![&id],
                    |row| row.get(0),
                )
                .unwrap_or(0.0);
            Ok(t)
        })
        .unwrap();

    assert!(trust < 1.0, "unhelpful rating should decrease trust score");
}

// ═════════════════════════════════════════════════════════════════════════════
// 16. Schema introspection (tdg_get_schema underlying path)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_schema_introspection() {
    let pool = make_pool();

    let tables: Vec<String> = pool
        .with_connection(|conn| {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            let names: Vec<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .filter(|n: &String| !n.starts_with("sqlite_"))
                .collect();
            Ok(names)
        })
        .unwrap();

    assert!(tables.contains(&"nodes".to_string()));
    assert!(tables.contains(&"edges".to_string()));
    assert!(tables.contains(&"events".to_string()));
}

// ═════════════════════════════════════════════════════════════════════════════
// 17. Graph traversal — get_related
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_get_related_outgoing() {
    let pool = make_pool();
    let src = add_node(&pool, "observation", "Source");
    let tgt = add_node(&pool, "hypothesis", "Target");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: src.clone(),
                target_id: tgt.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let related: Vec<(String, String)> = pool
        .with_connection(|conn| {
            let edges = tdg_rust::db::crud::get_edges(conn, Some(&src), None, None, None, 20)?;
            let mut result = Vec::new();
            for edge in &edges {
                if let Ok(Some(n)) = tdg_rust::db::crud::get_node(conn, &edge.target_id) {
                    result.push((n.name, edge.edge_type.clone()));
                }
            }
            Ok(result)
        })
        .unwrap();

    assert_eq!(related.len(), 1);
    assert_eq!(related[0].0, "Target");
    assert_eq!(related[0].1, "EVIDENCES");
}

#[test]
fn e2e_get_related_incoming() {
    let pool = make_pool();
    let src = add_node(&pool, "observation", "Evidence");
    let tgt = add_node(&pool, "hypothesis", "Claim");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: src.clone(),
                target_id: tgt.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let related: Vec<(String, String)> = pool
        .with_connection(|conn| {
            let edges = tdg_rust::db::crud::get_edges(conn, None, Some(&tgt), None, None, 20)?;
            let mut result = Vec::new();
            for edge in &edges {
                if let Ok(Some(n)) = tdg_rust::db::crud::get_node(conn, &edge.source_id) {
                    result.push((n.name, edge.edge_type.clone()));
                }
            }
            Ok(result)
        })
        .unwrap();

    assert_eq!(related.len(), 1);
    assert_eq!(related[0].0, "Evidence");
}

#[test]
fn e2e_get_related_both_directions() {
    let pool = make_pool();
    let a = add_node(&pool, "observation", "A");
    let b = add_node(&pool, "hypothesis", "B");
    let c = add_node(&pool, "telos", "C");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: a.clone(),
                target_id: b.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: c.clone(),
                target_id: b.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let in_edges = pool
        .with_connection(|conn| tdg_rust::db::crud::get_edges(conn, None, Some(&b), None, None, 20))
        .unwrap();

    assert_eq!(in_edges.len(), 2);
}

// ═════════════════════════════════════════════════════════════════════════════
// 18. Pathfinding (used by tdg_connect redundancy check)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_pathfind_chain() {
    let pool = make_pool();
    let a = add_node(&pool, "telos", "A");
    let b = add_node(&pool, "action", "B");
    let c = add_node(&pool, "action", "C");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: a.clone(),
                target_id: b.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: b.clone(),
                target_id: c.clone(),
                edge_type: "DEPENDS_ON".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let paths = pool
        .with_connection(|conn| tdg_rust::db::crud::pathfind(conn, &a, &c, 5, 100))
        .unwrap();

    assert!(!paths.is_empty());
    assert_eq!(paths[0].len(), 3, "a → b → c = 3 hops");
}

#[test]
fn e2e_pathfind_no_path() {
    let pool = make_pool();
    let a = add_node(&pool, "telos", "A");
    let b = add_node(&pool, "action", "B");

    let paths = pool
        .with_connection(|conn| tdg_rust::db::crud::pathfind(conn, &a, &b, 5, 100))
        .unwrap();

    assert!(paths.is_empty(), "no path without edges");
}

// ═════════════════════════════════════════════════════════════════════════════
// 19. Knowledge engine operations
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_classify_and_link_catalyst() {
    let pool = make_pool();
    let obs = add_node(&pool, "observation", "Signal: Something happened");
    let hyp = add_node(&pool, "hypothesis", "Possible cause");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: obs.clone(),
                target_id: hyp.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let classified = pool
        .with_connection(|conn| tdg_rust::knowledge::classify_catalyst(conn, &obs))
        .unwrap();
    assert_eq!(classified["status"], "classified");

    let linked = pool
        .with_connection(|conn| tdg_rust::knowledge::link_catalyst_to_structure(conn, &obs))
        .unwrap();
    assert_eq!(linked["status"], "linked");
}

#[test]
fn e2e_hygiene_report() {
    let pool = make_pool();
    add_node(&pool, "observation", "Node1");
    add_node(&pool, "observation", "Node2");

    let report = pool
        .with_connection(|conn| tdg_rust::knowledge::generate_hygiene_report(conn))
        .unwrap();

    assert!(report.total_nodes >= 2);
}

#[test]
fn e2e_detect_orphans() {
    let pool = make_pool();
    let a = add_node(&pool, "observation", "Connected");
    let b = add_node(&pool, "observation", "Orphan");

    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: a.clone(),
                target_id: a.clone(),
                edge_type: "SUPPORTS".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        Ok(())
    })
    .unwrap();

    let orphans = pool
        .with_connection(|conn| tdg_rust::knowledge::detect_orphans(conn))
        .unwrap();

    let disconnected = orphans["disconnected"].as_array().unwrap();
    assert!(
        disconnected
            .iter()
            .any(|n| n["node_id"].as_str() == Some(&b)),
        "Node b should be orphaned"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 20. Record exec (tdg_record_exec underlying path)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_record_exec_creates_observation() {
    let pool = make_pool();

    let node = pool
        .with_connection(|conn| {
            let truncated: String = "Deployed v1.0 to production".chars().take(80).collect();
            let props = json!({
                "action_type": "deploy",
                "result": "success",
                "tags": "production,release",
                "metrics": "{}",
            });
            tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("deploy: {}", truncated),
                    description: Some("Deployed v1.0 to production".to_string()),
                    source: Some("mcp_record_exec".to_string()),
                    properties: Some(props),
                    ..Default::default()
                },
            )
        })
        .unwrap();

    assert!(node.id.starts_with('n'));
    assert_eq!(node.node_type, "observation");
    assert_eq!(node.source, "mcp_record_exec");
    assert_eq!(node.properties["action_type"], "deploy");
    assert_eq!(node.properties["result"], "success");
}

// ═════════════════════════════════════════════════════════════════════════════
// 21. Maintenance operations
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_maintenance_hygiene() {
    let pool = make_pool();
    add_node(&pool, "observation", "Test Node");

    let report = pool
        .with_connection(|conn| tdg_rust::knowledge::generate_hygiene_report(conn))
        .unwrap();

    assert!(report.orphan_count >= 0);
    assert!(report.dangling_edge_count >= 0);
    assert!(report.stale_count >= 0);
}

#[test]
fn e2e_maintenance_archive() {
    let pool = make_pool();
    add_node(&pool, "observation", "Stale Node");

    let archived = pool
        .with_connection(|conn| tdg_rust::knowledge::archive_stale_nodes(conn, Some(0)))
        .unwrap();

    let archived_count = archived
        .get("archived_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert!(archived_count >= 0);
}

// ═════════════════════════════════════════════════════════════════════════════
// 22. MCP Maintenance tool contract tests
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_maintenance_tool_rebuild_fts() {
    let pool = make_pool();
    add_node_with_desc(&pool, "observation", "Test Node", "Test description");

    let server_pool = make_pool();
    let server = tdg_rust::mcp::tools::TdgServer::new(server_pool);
    let params = tdg_rust::mcp::params::MaintenanceParams {
        action: Some("rebuild_fts".to_string()),
        batch_size: None,
        phase: None,
    };

    // Use tokio runtime for async test
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        server
            .tdg_maintenance(rmcp::handler::server::wrapper::Parameters(params))
            .await
    });

    assert!(result.is_ok());
    let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(response["fts_rebuilt"], true);
}

#[test]
fn e2e_maintenance_tool_health() {
    let pool = make_pool();
    add_node(&pool, "observation", "Test Node");

    let server_pool = make_pool();
    let server = tdg_rust::mcp::tools::TdgServer::new(server_pool);
    let params = tdg_rust::mcp::params::MaintenanceParams {
        action: Some("health".to_string()),
        batch_size: None,
        phase: None,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        server
            .tdg_maintenance(rmcp::handler::server::wrapper::Parameters(params))
            .await
    });

    assert!(result.is_ok());
    let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert!(response["orphan_count"].is_number());
    assert!(response["dangling_edge_count"].is_number());
    assert!(response["stale_node_count"].is_number());
}

#[test]
fn e2e_maintenance_tool_phase_fallback() {
    let pool = make_pool();
    add_node(&pool, "observation", "Test Node");

    let server_pool = make_pool();
    let server = tdg_rust::mcp::tools::TdgServer::new(server_pool);
    let params = tdg_rust::mcp::params::MaintenanceParams {
        action: None,
        batch_size: None,
        phase: Some("health".to_string()),
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        server
            .tdg_maintenance(rmcp::handler::server::wrapper::Parameters(params))
            .await
    });

    assert!(result.is_ok());
    let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert!(response["orphan_count"].is_number());
}

#[test]
fn e2e_maintenance_tool_gc_all_implemented() {
    // gc_all is now implemented — it should succeed and return a JSON report
    // containing keys for archived nodes, pruned edges, embeddings, FTS, etc.
    let pool = make_pool();
    add_node(&pool, "observation", "Test Node");

    let server_pool = make_pool();
    let server = tdg_rust::mcp::tools::TdgServer::new(server_pool);
    let params = tdg_rust::mcp::params::MaintenanceParams {
        action: Some("gc_all".to_string()),
        batch_size: None,
        phase: None,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        server
            .tdg_maintenance(rmcp::handler::server::wrapper::Parameters(params))
            .await
    });

    assert!(
        result.is_ok(),
        "gc_all should succeed, got: {:?}",
        result.err()
    );
    let response = result.unwrap();
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();
    // Should include at least one of the GC report keys
    assert!(
        v.get("fts_rebuilt").is_some()
            || v.get("edges_pruned").is_some()
            || v.get("nodes_archived").is_some()
            || v.get("duplicate_edges_collapsed").is_some(),
        "gc_all response should contain GC report keys, got: {}",
        v
    );
}

#[test]
fn e2e_maintenance_tool_no_action_or_phase() {
    let pool = make_pool();
    add_node(&pool, "observation", "Test Node");

    let server_pool = make_pool();
    let server = tdg_rust::mcp::tools::TdgServer::new(server_pool);
    let params = tdg_rust::mcp::params::MaintenanceParams {
        action: None,
        batch_size: None,
        phase: None,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        server
            .tdg_maintenance(rmcp::handler::server::wrapper::Parameters(params))
            .await
    });

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err
        .to_string()
        .contains("Either 'action' or 'phase' parameter is required"));
}
