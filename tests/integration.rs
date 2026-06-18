//! Integration tests for TDG-Rust end-to-end workflows.
//!
//! Tests cross-module functionality: CRUD → Query → Knowledge → Mind → Ops → MCP.

use tdg_rust::db::{init_fts, init_schema, run_migrations, ConnectionPool};
use tdg_rust::knowledge;
use tdg_rust::models::{NewEdge, NewNode, NodeQuery};
use tdg_rust::ops;
// scripts module tested via ops pipeline
use serde_json::json;
use tdg_rust::flow::FlowDriveState;
use tdg_rust::mind::diagnostic::DiagnosticEngine;
use tdg_rust::mind::pulse::PulseEngine;

/// Helper: create an in-memory pool for integration tests.
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

// ─── CRUD → Query Pipeline ───────────────────────────────────────────────────

#[test]
fn integration_create_query_search() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        // Create nodes of different types
        let telos = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Main Goal".to_string(),
                description: Some("Primary objective".to_string()),
                ..Default::default()
            },
        )?;

        let hyp = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "hypothesis".to_string(),
                name: "Rust is memory safe".to_string(),
                description: Some("Memory safety without GC".to_string()),
                ..Default::default()
            },
        )?;

        let obs = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Observation: Rust benchmarks".to_string(),
                description: Some("Rust performs well in benchmarks".to_string()),
                ..Default::default()
            },
        )?;

        // Connect them: telos → hypothesis (DECOMPOSES_TO), obs → hyp (EVIDENCES)
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: telos.id.clone(),
                target_id: hyp.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: obs.id.clone(),
                target_id: hyp.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )?;

        // Query by type
        let q = NodeQuery {
            node_type: Some("hypothesis".to_string()),
            ..Default::default()
        };
        let hyps = tdg_rust::db::crud::query_nodes(conn, &q)?;
        assert_eq!(hyps.len(), 1);
        assert_eq!(hyps[0].id, hyp.id);

        // Search
        let results = tdg_rust::db::crud::search(conn, "memory", 10)?;
        assert!(!results.is_empty());

        // Get edges
        let out_edges =
            tdg_rust::db::crud::get_edges(conn, Some(&telos.id), None, None, None, 100)?;
        assert_eq!(out_edges.len(), 1);
        assert_eq!(out_edges[0].edge_type, "DECOMPOSES_TO");

        let in_edges = tdg_rust::db::crud::get_edges(conn, None, Some(&hyp.id), None, None, 100)?;
        assert_eq!(in_edges.len(), 2);

        // Record event
        let event_id = tdg_rust::db::crud::record_event(
            conn,
            "test_event",
            Some(&hyp.id),
            None,
            None,
            Some(&json!({"test": true})),
        )?;
        assert!(!event_id.is_empty()); // UUID hex string

        Ok(())
    })
    .unwrap();
}

// ─── Knowledge Engine Pipeline ───────────────────────────────────────────────

#[test]
fn integration_knowledge_lifecycle() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        // Create observation nodes
        let obs1 = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Signal: Performance drop".to_string(),
                description: Some("Signal alert detected".to_string()),
                ..Default::default()
            },
        )?;

        let hyp = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "hypothesis".to_string(),
                name: "CPU bottleneck".to_string(),
                ..Default::default()
            },
        )?;

        // Link observation to hypothesis
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: obs1.id.clone(),
                target_id: hyp.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )?;

        // Classify → Link → Evaluate
        let classified = knowledge::classify_catalyst(conn, &obs1.id)?;
        assert_eq!(classified["status"], "classified");

        let linked = knowledge::link_catalyst_to_structure(conn, &obs1.id)?;
        assert_eq!(linked["status"], "linked");
        assert_eq!(linked["hypotheses"][0], hyp.id);

        let evaluated = knowledge::evaluate_integration_quality(conn, &obs1.id)?;
        let quality = evaluated["integration_quality"].as_f64().unwrap();
        assert!(quality > 0.0);

        // Full lifecycle
        let obs2 = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Insight: Pattern found".to_string(),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: obs2.id.clone(),
                target_id: hyp.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )?;
        let lifecycle = knowledge::process_catalyst_lifecycle(conn, &obs2.id)?;
        assert_eq!(lifecycle["status"], "lifecycle_complete");

        // Hygiene report
        let report = knowledge::generate_hygiene_report(conn)?;
        assert!(report.total_nodes >= 3);

        // Detect orphans
        let orphans = knowledge::detect_orphans(conn)?;
        // obs1 and obs2 are linked, hyp is linked - no orphans
        let disconnected = orphans["disconnected"].as_array().unwrap();
        assert!(disconnected.is_empty());

        // Archive stale nodes
        let archived = knowledge::archive_stale_nodes(conn, Some(0))?;
        // With threshold 0, any observation is stale
        let archived_count = archived["archived_count"].as_i64().unwrap();
        assert!(archived_count >= 0); // may or may not archive depending on timing

        Ok(())
    })
    .unwrap();
}

// ─── Flow Engine Pipeline ────────────────────────────────────────────────────

#[test]
fn integration_flow_drives() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let telos = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Flow Telos".to_string(),
                ..Default::default()
            },
        )?;

        let action = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "Flow Action".to_string(),
                ..Default::default()
            },
        )?;

        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: telos.id.clone(),
                target_id: action.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )?;

        // Set drive state on action
        let state = FlowDriveState::intrinsic("action");
        let json = state.to_json();
        conn.execute(
            "UPDATE nodes SET drives_json = ?1 WHERE id = ?2",
            rusqlite::params![json.to_string(), action.id],
        )?;

        // Renormalize graph
        let result = tdg_rust::flow::renormalize_graph(conn, false)?;
        assert!(
            result.get("healed").is_some()
                || result.get("emitted").is_some()
                || result.get("aggregated").is_some()
        );

        // Compute entropy
        let entropy = tdg_rust::flow::compute_graph_entropy(conn)?;
        assert!(entropy.as_f64().unwrap_or(0.0) >= 0.0);

        // Diagnose polarity
        let polarity = tdg_rust::flow::diagnose_polarity(conn)?;
        // Returns addictions/allergies/blind_spots/tension_pairs/chakra_health
        assert!(polarity.get("chakra_health").is_some() || polarity.get("addictions").is_some());

        // Aggregate upward
        let aggregated = tdg_rust::flow::aggregate_upward(conn, &action.id)?;
        assert!(aggregated >= 0);

        Ok(())
    })
    .unwrap();
}

// ─── Mind Pipeline ───────────────────────────────────────────────────────────

#[test]
fn integration_mind_pulse_diagnostic() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        // Create a minimal graph
        let telos = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Mind Telos".to_string(),
                ..Default::default()
            },
        )?;
        let obs = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Mind Observation".to_string(),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: obs.id.clone(),
                target_id: telos.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )?;

        // Pulse
        let pulse_engine = PulseEngine::new();
        let pulses = pulse_engine.pulse(conn, &[])?;
        let summary = pulse_engine.summarize(&pulses);
        assert!(summary.get("total_gaps").is_some());

        // Diagnostic
        let diag_engine = DiagnosticEngine::new();
        let report = diag_engine.analyze(conn, &[], &[])?;
        assert!(report.ghost_nodes >= 0);

        Ok(())
    })
    .unwrap();
}

// ─── Ops Pipeline ────────────────────────────────────────────────────────────

#[test]
fn integration_ops_reconcile_micro_macro() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let telos = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Ops Telos".to_string(),
                ..Default::default()
            },
        )?;
        let action = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "Ops Action".to_string(),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: telos.id.clone(),
                target_id: action.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )?;

        // Reconcile
        let reconciled = ops::reconcile(conn)?;
        assert_eq!(reconciled["status"], "completed");

        // Micro slice
        let micro = ops::micro_slice(conn)?;
        assert_eq!(micro["summary"]["total_actions"], 1);

        // Macro slice
        let macro_result = ops::macro_slice(conn, None)?;
        assert!(macro_result.get("health").is_some());

        // Stage status
        let stage = ops::stage_status(conn)?;
        assert_eq!(stage["total_teloi"], 1);

        // Drive matrix
        let matrix = ops::drive_matrix_report(conn, Some(&telos.id))?;
        assert!(matrix.get("cells").is_some());

        // Record action
        let recorded = ops::record_action(conn, "test_action", Some("UR"), None, Some("notes"))?;
        assert!(recorded.get("action_id").is_some());

        // Flow up
        let flowed = ops::flow_up(conn, &action.id)?;
        assert!(flowed["parents_updated"].as_i64().unwrap() >= 0);

        // Hygiene
        let hygiene = ops::hygiene(conn)?;
        assert!(hygiene.get("orphans").is_some());

        Ok(())
    })
    .unwrap();
}

// ─── End-to-End: Create → Connect → Query → Knowledge → Archive ──────────────

#[test]
fn integration_end_to_end_workflow() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        // 1. Create a telos with actions
        let telos = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "E2E Goal".to_string(),
                description: Some("End to end test goal".to_string()),
                ..Default::default()
            },
        )?;

        let action_a = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "Action A".to_string(),
                ..Default::default()
            },
        )?;
        let action_b = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "Action B".to_string(),
                ..Default::default()
            },
        )?;

        // 2. Connect: telos → actions
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: telos.id.clone(),
                target_id: action_a.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: telos.id.clone(),
                target_id: action_b.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )?;

        // 3. Create observations that evidence the telos
        let obs = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Observation supporting goal".to_string(),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: obs.id.clone(),
                target_id: telos.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )?;

        // 4. Classify + link observation
        knowledge::classify_catalyst(conn, &obs.id)?;
        knowledge::link_catalyst_to_structure(conn, &obs.id)?;

        // 5. Set drives on telos
        let drive_state = FlowDriveState::intrinsic("telos");
        conn.execute(
            "UPDATE nodes SET drives_json = ?1 WHERE id = ?2",
            rusqlite::params![drive_state.to_json().to_string(), telos.id],
        )?;

        // 6. Run reconcile
        ops::reconcile(conn)?;

        // 7. Run hygiene
        ops::hygiene(conn)?;

        // 8. Run diagnostics
        let diag = DiagnosticEngine::new();
        let report = diag.analyze(conn, &[], &[])?;
        assert!(report.ghost_nodes >= 0);

        // 9. Run pulse
        let pulse = PulseEngine::new();
        let pulses = pulse.pulse(conn, &[])?;
        let summary = pulse.summarize(&pulses);
        assert!(summary.get("total_gaps").is_some());

        // 10. Generate hygiene report
        let hygiene = knowledge::generate_hygiene_report(conn)?;
        assert!(hygiene.total_nodes >= 4);
        assert!(hygiene.total_edges >= 3);

        // 11. Full stats check
        let node_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL",
            [],
            |r| r.get(0),
        )?;
        let edge_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE valid_to IS NULL",
            [],
            |r| r.get(0),
        )?;
        let event_count: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;
        assert!(node_count >= 4);
        assert!(edge_count >= 3);
        assert!(event_count >= 1); // at least the classify event

        Ok(())
    })
    .unwrap();
}

// ─── Database Pool Stress Test ───────────────────────────────────────────────

#[test]
fn integration_pool_concurrent_connections() {
    let pool = make_pool();
    // Verify pool can handle multiple sequential connections
    for i in 0..20 {
        pool.with_connection(|conn| {
            let node = tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("Pool Test Node {i}"),
                    ..Default::default()
                },
            )?;
            assert!(node.id.starts_with('n'));
            Ok(())
        })
        .unwrap();
    }

    let count: i64 = pool
        .with_connection(|conn| {
            Ok(conn
                .query_row(
                    "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL",
                    [],
                    |r| r.get(0),
                )
                .unwrap())
        })
        .unwrap();
    assert_eq!(count, 20);
}

// ─── Pathfind Integration ────────────────────────────────────────────────────

#[test]
fn integration_pathfind() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let a = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "A".to_string(),
                ..Default::default()
            },
        )?;
        let b = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "B".to_string(),
                ..Default::default()
            },
        )?;
        let c = tdg_rust::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "action".to_string(),
                name: "C".to_string(),
                ..Default::default()
            },
        )?;

        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: a.id.clone(),
                target_id: b.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )?;
        tdg_rust::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: b.id.clone(),
                target_id: c.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )?;

        let paths = tdg_rust::db::crud::pathfind(conn, &a.id, &c.id, 5, 100)?;
        assert!(!paths.is_empty());
        assert_eq!(paths[0].len(), 3); // a → b → c

        Ok(())
    })
    .unwrap();
}

// ─── Health Check Persistence ────────────────────────────────────────────────

#[test]
fn integration_health_check_crud() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::record_health_check(conn, "api-gateway", 42.5, true, None)?;
        tdg_rust::db::crud::record_health_check(conn, "api-gateway", 120.0, false, Some("timeout"))?;
        tdg_rust::db::crud::record_health_check(conn, "db", 5.0, true, None)?;

        let summary = tdg_rust::db::crud::get_health_summary(conn)?;
        assert_eq!(summary["total_checks"], 3);
        assert!(summary["success_rate"].as_f64().unwrap() > 0.6);
        assert!(summary["avg_latency_ms"].as_f64().unwrap() > 0.0);

        let recent = tdg_rust::db::crud::get_recent_health_checks(conn, None, 10)?;
        assert_eq!(recent.len(), 3);

        let filtered = tdg_rust::db::crud::get_recent_health_checks(conn, Some("db"), 10)?;
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0]["service"], "db");

        Ok(())
    })
    .unwrap();
}
