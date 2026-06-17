//! Scripts integration tests for Phase 12 utility functions.
//!
//! Tests cross-module functionality: scripts → CRUD → knowledge → ops.

use tdg_rust::db::{init_fts, init_schema, run_migrations, ConnectionPool};
use tdg_rust::models::{NewEdge, NewNode};
use tdg_rust::scripts;
use tdg_rust::knowledge;
use serde_json::json;

/// Helper: create an in-memory pool for scripts integration tests.
fn make_pool() -> ConnectionPool {
    let pool = ConnectionPool::new(":memory:", 5, 30000).expect("pool creation");
    pool.with_connection(|conn| {
        init_schema(conn)?;
        init_fts(conn)?;
        run_migrations(conn)?;
        Ok(())
    })
    .unwrap();
    pool
}

// ─── Audit Script ────────────────────────────────────────────────────────────

#[test]
fn scripts_audit_empty_graph() {
    let pool = make_pool();
    let result = pool.with_connection(|conn| scripts::audit(conn)).unwrap();
    assert_eq!(result["audit"], "completed");
    assert_eq!(result["health"]["total_nodes"], 0);
}

#[test]
fn scripts_audit_with_data() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(conn, &NewNode {
            node_type: "observation".to_string(),
            name: "Test Obs".to_string(),
            ..Default::default()
        })?;
        tdg_rust::db::crud::add_node(conn, &NewNode {
            node_type: "hypothesis".to_string(),
            name: "Test Hyp".to_string(),
            ..Default::default()
        })?;
        Ok(())
    })
    .unwrap();

    let result = pool.with_connection(|conn| scripts::audit(conn)).unwrap();
    assert_eq!(result["audit"], "completed");
    assert_eq!(result["health"]["total_nodes"], 2);
}

// ─── Check Script ────────────────────────────────────────────────────────────

#[test]
fn scripts_check_empty() {
    let pool = make_pool();
    let result = pool.with_connection(|conn| scripts::check(conn)).unwrap();
    assert_eq!(result["constraints"], 0);
    assert_eq!(result["blocks_edges"], 0);
}

#[test]
fn scripts_check_with_constraints() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::add_node(conn, &NewNode {
            node_type: "constraint".to_string(),
            name: "Constraint 1".to_string(),
            ..Default::default()
        })?;
        tdg_rust::db::crud::add_node(conn, &NewNode {
            node_type: "action".to_string(),
            name: "Action 1".to_string(),
            ..Default::default()
        })?;
        Ok(())
    })
    .unwrap();

    let result = pool.with_connection(|conn| scripts::check(conn)).unwrap();
    assert_eq!(result["constraints"], 1);
    // Warning because no BLOCKS edges exist despite active nodes
    assert!(!result["warnings"].as_array().unwrap().is_empty());
}

// ─── Auto-Capture Script ─────────────────────────────────────────────────────

#[test]
fn scripts_auto_capture_basic() {
    let pool = make_pool();
    let result = pool
        .with_connection(|conn| {
            scripts::auto_capture(conn, "New observation from test", "LR", 0.8, Some("entity1"))
        })
        .unwrap();
    assert!(result["observation_id"].as_str().unwrap().starts_with('n'));
    assert_eq!(result["quadrant"], "LR");
    assert_eq!(result["trust"], 0.8);
}

#[test]
fn scripts_auto_capture_creates_event() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        scripts::auto_capture(conn, "Observed something", "UL", 0.5, None)?;
        Ok(())
    })
    .unwrap();

    let event_count: i64 = pool
        .with_connection(|conn| {
            Ok(conn
                .query_row(
                    "SELECT COUNT(*) FROM events WHERE event_action = 'auto_capture'",
                    [],
                    |r| r.get(0),
                )
                .unwrap())
        })
        .unwrap();
    assert_eq!(event_count, 1);
}

// ─── Create Node Script ──────────────────────────────────────────────────────

#[test]
fn scripts_create_node_basic() {
    let pool = make_pool();
    let result = pool
        .with_connection(|conn| scripts::create_node(conn, "telos", "My Goal", Some("A primary goal")))
        .unwrap();
    assert!(result["id"].as_str().unwrap().starts_with('n'));
    assert_eq!(result["node_type"], "telos");
    assert_eq!(result["name"], "My Goal");
}

#[test]
fn scripts_create_node_no_description() {
    let pool = make_pool();
    let result = pool
        .with_connection(|conn| scripts::create_node(conn, "action", "Do Thing", None))
        .unwrap();
    assert!(result.get("id").is_some());
}

// ─── Maintenance Check Script ────────────────────────────────────────────────

#[test]
fn scripts_maintenance_check_empty() {
    let pool = make_pool();
    let result = pool.with_connection(|conn| scripts::maintenance_check(conn)).unwrap();
    assert_eq!(result["orphan_count"], 0);
}

#[test]
fn scripts_maintenance_check_with_orphans() {
    let pool = make_pool();
    // Create orphan nodes (no edges)
    pool.with_connection(|conn| {
        for i in 0..5 {
            tdg_rust::db::crud::add_node(conn, &NewNode {
                node_type: "observation".to_string(),
                name: format!("Orphan {i}"),
                ..Default::default()
            })?;
        }
        Ok(())
    })
    .unwrap();

    let result = pool.with_connection(|conn| scripts::maintenance_check(conn)).unwrap();
    assert_eq!(result["orphan_count"], 5);
}

// ─── Repair Orphans Script ───────────────────────────────────────────────────

#[test]
fn scripts_repair_orphans_empty() {
    let pool = make_pool();
    let result = pool.with_connection(|conn| scripts::repair_orphans(conn)).unwrap();
    assert_eq!(result["total_orphans"], 0);
    assert_eq!(result["archived"], 0);
}

#[test]
fn scripts_repair_orphans_archives_critical() {
    let pool = make_pool();
    // Create orphan nodes and manually age them
    pool.with_connection(|conn| {
        for i in 0..3 {
            let node = tdg_rust::db::crud::add_node(conn, &NewNode {
                node_type: "observation".to_string(),
                name: format!("Old orphan {i}"),
                ..Default::default()
            })?;
            // Set created_at to 90 days ago to trigger critical severity
            let ninety_days_ago = chrono::Utc::now()
                .naive_utc()
                .checked_sub_signed(chrono::Duration::days(90))
                .unwrap()
                .format("%Y-%m-%dT%H:%M:%S%.f")
                .to_string();
            conn.execute(
                "UPDATE nodes SET created_at = ?1 WHERE id = ?2",
                rusqlite::params![ninety_days_ago, node.id],
            )?;
        }
        Ok(())
    })
    .unwrap();

    let result = pool.with_connection(|conn| scripts::repair_orphans(conn)).unwrap();
    assert!(result["total_orphans"].as_i64().unwrap() >= 1);
    assert!(result["archived"].as_i64().unwrap() >= 1);
}

// ─── Unify Script ────────────────────────────────────────────────────────────

#[test]
fn scripts_unify_empty_graph() {
    let pool = make_pool();
    let result = pool.with_connection(|conn| scripts::unify(conn)).unwrap();
    assert_eq!(result["unified"], true);
    assert_eq!(result["total_events"], 0);
    assert_eq!(result["orphan_events"], 0);
    assert_eq!(result["duplicate_edge_groups"], 0);
}

#[test]
fn scripts_unify_with_events_and_edges() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let a = tdg_rust::db::crud::add_node(conn, &NewNode {
            node_type: "telos".to_string(),
            name: "A".to_string(),
            ..Default::default()
        })?;
        let b = tdg_rust::db::crud::add_node(conn, &NewNode {
            node_type: "action".to_string(),
            name: "B".to_string(),
            ..Default::default()
        })?;
        tdg_rust::db::crud::add_edge(conn, &NewEdge {
            source_id: a.id.clone(),
            target_id: b.id.clone(),
            edge_type: "DECOMPOSES_TO".to_string(),
            ..Default::default()
        })?;
        tdg_rust::db::crud::record_event(
            conn,
            "test",
            Some(&a.id),
            None,
            None,
            Some(&json!({"test": true})),
        )?;
        Ok(())
    })
    .unwrap();

    let result = pool.with_connection(|conn| scripts::unify(conn)).unwrap();
    assert_eq!(result["unified"], true);
    // Events include our test event plus events created during knowledge operations
    assert!(result["total_events"].as_i64().unwrap() >= 1);
}

// ─── Reconcile Constraints Script ────────────────────────────────────────────

#[test]
fn scripts_reconcile_constraints_empty() {
    let pool = make_pool();
    let result = pool.with_connection(|conn| scripts::reconcile_constraints(conn)).unwrap();
    assert_eq!(result["constraints_deduped"], 0);
    assert_eq!(result["dangling_blocks_repaired"], 0);
}

#[test]
fn scripts_reconcile_constraints_dedup() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        // Create 3 constraints with the same name
        for _ in 0..3 {
            tdg_rust::db::crud::add_node(conn, &NewNode {
                node_type: "constraint".to_string(),
                name: "Duplicate Constraint".to_string(),
                ..Default::default()
            })?;
        }
        Ok(())
    })
    .unwrap();

    let result = pool.with_connection(|conn| scripts::reconcile_constraints(conn)).unwrap();
    // Should have deduped 2 constraints (kept 1, archived 2)
    assert_eq!(result["constraints_deduped"], 2);
}

// ─── Sync Skills Script ──────────────────────────────────────────────────────

#[test]
fn scripts_sync_skills_nonexistent_dir() {
    let pool = make_pool();
    let result = pool
        .with_connection(|conn| scripts::sync_skills(conn, "/nonexistent/path"))
        .unwrap();
    assert!(result.get("error").is_some());
    assert_eq!(result["synced"], 0);
}

#[test]
fn scripts_sync_skills_empty_dir() {
    let pool = make_pool();
    let dir = tempfile::tempdir().unwrap();
    let result = pool
        .with_connection(|conn| scripts::sync_skills(conn, dir.path().to_str().unwrap()))
        .unwrap();
    assert_eq!(result["synced"], 0);
    assert_eq!(result["skipped"], 0);
}

#[test]
fn scripts_sync_skills_json_files() {
    let pool = make_pool();
    let dir = tempfile::tempdir().unwrap();
    let skill_file = dir.path().join("test_skill.json");
    std::fs::write(
        &skill_file,
        r#"{"name": "Test Skill", "description": "A test skill"}"#,
    )
    .unwrap();

    let result = pool
        .with_connection(|conn| {
            scripts::sync_skills(conn, dir.path().to_str().unwrap())
        })
        .unwrap();
    assert_eq!(result["synced"], 1);

    // Verify the skill node was created
    let count: i64 = pool
        .with_connection(|conn| {
            Ok(conn
                .query_row(
                    "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL AND node_type = 'skill'",
                    [],
                    |r| r.get(0),
                )
                .unwrap())
        })
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn scripts_sync_skills_skip_duplicates() {
    let pool = make_pool();
    let dir = tempfile::tempdir().unwrap();
    let skill_file = dir.path().join("skill.json");
    std::fs::write(
        &skill_file,
        r#"{"name": "Existing Skill", "description": "Already in graph"}"#,
    )
    .unwrap();

    // First sync
    pool.with_connection(|conn| {
        scripts::sync_skills(conn, dir.path().to_str().unwrap())?;
        Ok(())
    })
    .unwrap();

    // Second sync (should skip)
    let result = pool
        .with_connection(|conn| {
            scripts::sync_skills(conn, dir.path().to_str().unwrap())
        })
        .unwrap();
    assert_eq!(result["synced"], 0);
    assert_eq!(result["skipped"], 1);
}

// ─── Full Pipeline: Scripts → Knowledge → Ops ────────────────────────────────

#[test]
fn scripts_full_pipeline() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        // 1. Create nodes via scripts
        let n1 = scripts::create_node(conn, "telos", "Goal", Some("Primary objective"))?;
        let n2 = scripts::create_node(conn, "action", "Action A", None)?;
        let n3 = scripts::create_node(conn, "observation", "Observation 1", Some("saw something"))?;
        let _n4 = scripts::create_node(conn, "constraint", "Constraint 1", None)?;

        let goal_id = n1["id"].as_str().unwrap();
        let action_id = n2["id"].as_str().unwrap();
        let obs_id = n3["id"].as_str().unwrap();

        // 2. Connect them
        tdg_rust::db::crud::add_edge(conn, &NewEdge {
            source_id: goal_id.to_string(),
            target_id: action_id.to_string(),
            edge_type: "DECOMPOSES_TO".to_string(),
            ..Default::default()
        })?;
        tdg_rust::db::crud::add_edge(conn, &NewEdge {
            source_id: obs_id.to_string(),
            target_id: goal_id.to_string(),
            edge_type: "EVIDENCES".to_string(),
            ..Default::default()
        })?;

        // 3. Auto-capture
        let captured = scripts::auto_capture(conn, "New insight", "UR", 0.9, Some("entity1"))?;
        assert!(captured.get("observation_id").is_some());

        // 4. Audit
        let audit = scripts::audit(conn)?;
        assert_eq!(audit["audit"], "completed");
        assert!(audit["health"]["total_nodes"].as_i64().unwrap() >= 5);

        // 5. Check constraints
        let check = scripts::check(conn)?;
        assert_eq!(check["constraints"], 1);

        // 6. Unify
        let unified = scripts::unify(conn)?;
        assert_eq!(unified["unified"], true);

        // 7. Reconcile constraints
        let reconciled = scripts::reconcile_constraints(conn)?;
        assert!(reconciled.get("constraints_deduped").is_some());

        // 8. Maintenance check
        let maintenance = scripts::maintenance_check(conn)?;
        assert!(maintenance.get("orphan_count").is_some());

        // 9. Repair orphans
        let repaired = scripts::repair_orphans(conn)?;
        assert!(repaired.get("total_orphans").is_some());

        // 10. Knowledge lifecycle on observation
        let classified = knowledge::classify_catalyst(conn, obs_id)?;
        assert_eq!(classified["status"], "classified");

        let linked = knowledge::link_catalyst_to_structure(conn, obs_id)?;
        assert_eq!(linked["status"], "linked");

        let evaluated = knowledge::evaluate_integration_quality(conn, obs_id)?;
        assert!(evaluated["integration_quality"].as_f64().unwrap() > 0.0);

        // 11. Hygiene report
        let report = knowledge::generate_hygiene_report(conn)?;
        assert!(report.total_nodes >= 5);

        Ok(())
    })
    .unwrap();
}
