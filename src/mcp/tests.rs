//! MCP Tool Tests — comprehensive testing for all 17 tools
//!
//! Uses temp-file-backed SQLite for isolation. Tests call tool methods directly
//! on TdgServer via tokio_test::block_on.

#[cfg(test)]
mod tool_tests {
    use crate::db::{init_fts, init_schema, run_migrations};
    use crate::mcp::params::{
        BankParams, BulkCreateParams, ConnectParams, CreateParams, EntityParams, GetNodeParams,
        GetRelatedParams, MaintenanceParams, MindStateParams, ObserveParams, QueryEventsParams,
        RateMemoryParams, RecordExecParams, ReflectParams, SearchParams, UpdateParams,
    };
    use crate::mcp::tools::TdgServer;
    use crate::models::{NewEdge, NewNode};
    use rmcp::handler::server::wrapper::Parameters;
    use tempfile::NamedTempFile;

    struct TestEnv {
        _tmpfile: NamedTempFile,
        pool: crate::db::ConnectionPool,
    }

    impl TestEnv {
        fn new() -> Self {
            let tmpfile = NamedTempFile::new().unwrap();
            let path = tmpfile.path().to_str().unwrap();
            let pool = crate::db::ConnectionPool::new(path, 5, 30000).unwrap();
            pool.with_connection(|conn| {
                init_schema(conn).unwrap();
                init_fts(conn).unwrap();
                run_migrations(conn).unwrap();
                Ok(())
            })
            .unwrap();
            Self {
                _tmpfile: tmpfile,
                pool,
            }
        }

        fn server(&self) -> TdgServer {
            TdgServer::new(
                crate::db::ConnectionPool::new(self._tmpfile.path().to_str().unwrap(), 5, 30000)
                    .unwrap(),
            )
        }

        fn add_node(&self, node_type: &str, name: &str) -> String {
            self.pool
                .with_connection(|conn| {
                    let node = crate::db::crud::add_node(
                        conn,
                        &NewNode {
                            node_type: node_type.to_string(),
                            name: name.to_string(),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    Ok(node.id)
                })
                .unwrap()
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Runtime::new().unwrap()
    }

    // ── tdg_create ────────────────────────────────────────────────────────

    #[test]
    fn create_basic() {
        let env = TestEnv::new();
        let server = env.server();
        let params = CreateParams {
            node_id: None,
            node_type: "observation".into(),
            text: "Test node text".into(),
            embedding: None,
            aliases: None,
            meta: None,
            trust: None,
            name: "Test Node".into(),
            parent_ids: None,
            quadrant: Some("LR".into()),
            t_level: Some("L1".into()),
            stage: None,
            description: Some("A test".into()),
            source: None,
            lifecycle_state: None,
            blocks_targets: None,
            evidence_targets: None,
        };
        let result = rt()
            .block_on(server.tdg_create(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["id"].as_str().unwrap().starts_with('n'));
        assert_eq!(v["name"], "Test Node");
    }

    #[test]
    fn create_empty_name_fails() {
        let env = TestEnv::new();
        let server = env.server();
        let params = CreateParams {
            node_id: None,
            node_type: "observation".into(),
            text: "".into(),
            embedding: None,
            aliases: None,
            meta: None,
            trust: None,
            name: "".into(),
            parent_ids: None,
            quadrant: None,
            t_level: None,
            stage: None,
            description: None,
            source: None,
            lifecycle_state: None,
            blocks_targets: None,
            evidence_targets: None,
        };
        assert!(rt()
            .block_on(server.tdg_create(Parameters(params)))
            .is_err());
    }

    // ── tdg_get_node ──────────────────────────────────────────────────────

    #[test]
    fn get_node_basic() {
        let env = TestEnv::new();
        let id = env.add_node("observation", "Find Me");
        let server = env.server();
        let params = GetNodeParams {
            node_id: id.clone(),
            include_context: Some(false),
        };
        let result = rt()
            .block_on(server.tdg_get_node(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["id"], id);
        assert_eq!(v["name"], "Find Me");
    }

    #[test]
    fn get_node_with_context() {
        let env = TestEnv::new();
        let id = env.add_node("observation", "Context Node");
        let server = env.server();
        let params = GetNodeParams {
            node_id: id,
            include_context: Some(true),
        };
        let result = rt()
            .block_on(server.tdg_get_node(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("neighbors").is_some());
    }

    #[test]
    fn get_node_not_found() {
        let env = TestEnv::new();
        let server = env.server();
        let params = GetNodeParams {
            node_id: "n_nonexistent".into(),
            include_context: None,
        };
        assert!(rt()
            .block_on(server.tdg_get_node(Parameters(params)))
            .is_err());
    }

    // ── tdg_search ────────────────────────────────────────────────────────

    #[test]
    fn search_basic() {
        let env = TestEnv::new();
        env.add_node("observation", "Rust memory safety");
        env.add_node("observation", "Python GIL");
        let server = env.server();
        let params = SearchParams {
            query: "Rust".into(),
            node_type: None,
            limit: None,
        };
        let result = rt()
            .block_on(server.tdg_search(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["total"].as_u64().unwrap() > 0);
    }

    // ── tdg_connect ───────────────────────────────────────────────────────

    #[test]
    fn connect_basic() {
        let env = TestEnv::new();
        let src = env.add_node("action", "Source");
        let tgt = env.add_node("telos", "Target");
        let server = env.server();
        let params = ConnectParams {
            source_id: src,
            target_id: tgt,
            edge_type: "DECOMPOSES_TO".into(),
            weight: None,
            meta: None,
            as_edge: None,
            force: None,
        };
        let result = rt()
            .block_on(server.tdg_connect(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["edge_id"].as_str().unwrap().starts_with('e'));
        assert_eq!(v["edge_type"], "DECOMPOSES_TO");
    }

    #[test]
    fn connect_auto_type_to_telos() {
        let env = TestEnv::new();
        let src = env.add_node("observation", "Obs");
        let tgt = env.add_node("telos", "Goal");
        let server = env.server();
        let params = ConnectParams {
            source_id: src,
            target_id: tgt,
            edge_type: "EVIDENCES".into(),
            weight: None,
            meta: None,
            as_edge: None,
            force: None,
        };
        let result = rt()
            .block_on(server.tdg_connect(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["edge_type"], "EVIDENCES");
    }

    #[test]
    fn connect_duplicate_detected() {
        let env = TestEnv::new();
        let src = env.add_node("observation", "A");
        let tgt = env.add_node("observation", "B");
        let server = env.server();
        let p = ConnectParams {
            source_id: src.clone(),
            target_id: tgt.clone(),
            edge_type: "SUPPORTS".into(),
            weight: None,
            meta: None,
            as_edge: None,
            force: None,
        };
        rt().block_on(server.tdg_connect(Parameters(p))).unwrap();
        // Second connect should detect duplicate
        let p2 = ConnectParams {
            source_id: src,
            target_id: tgt,
            edge_type: "SUPPORTS".into(),
            weight: None,
            meta: None,
            as_edge: None,
            force: None,
        };
        let result = rt().block_on(server.tdg_connect(Parameters(p2))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["status"], "already_exists");
    }

    // ── tdg_query_events ──────────────────────────────────────────────────

    #[test]
    fn query_events_empty() {
        let env = TestEnv::new();
        let server = env.server();
        let params = QueryEventsParams {
            action: None,
            node_id: None,
            after: None,
            before: None,
            limit: None,
            offset: None,
        };
        let result = rt()
            .block_on(server.tdg_query_events(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["total"], 0);
    }

    // ── tdg_update ────────────────────────────────────────────────────────

    #[test]
    fn update_basic() {
        let env = TestEnv::new();
        let id = env.add_node("observation", "Original");
        let server = env.server();
        let params = UpdateParams {
            node_id: id,
            text: None,
            node_type: None,
            aliases: None,
            meta: None,
            name: Some("Updated".into()),
            description: None,
            lifecycle_state: None,
            t_level: None,
            stage: None,
            add_parent_ids: None,
            remove_parent_ids: None,
        };
        let result = rt()
            .block_on(server.tdg_update(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["name"], "Updated");
    }

    // ── tdg_rate_memory ───────────────────────────────────────────────────

    #[test]
    fn rate_memory_helpful() {
        let env = TestEnv::new();
        let id = env.add_node("observation", "Rate Me");
        let server = env.server();
        let params = RateMemoryParams {
            node_id: id,
            rating: "helpful".into(),
            reason: None,
            helpful: true,
        };
        let result = rt()
            .block_on(server.tdg_rate_memory(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["trust_score"].as_f64().unwrap() > 0.0);
    }

    // ── tdg_mind_state ────────────────────────────────────────────────────

    #[test]
    fn mind_state_counts() {
        let env = TestEnv::new();
        let server = env.server();
        let params = MindStateParams {
            terrain_for: None,
            injection_status: None,
            summary: None,
            full: None,
            detail: None,
            health: None,
            verify: None,
        };
        let result = rt()
            .block_on(server.tdg_mind_state(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("graph").is_some());
        assert!(v["graph"].get("nodes").is_some());
        assert!(v["graph"].get("edges").is_some());
        assert!(v.get("observations").is_some());
        assert!(v.get("quadrants").is_some());
    }

    // ── tdg_observe ───────────────────────────────────────────────────────

    #[test]
    fn observe_basic() {
        let env = TestEnv::new();
        let server = env.server();
        let params = ObserveParams {
            text: "Test observation text".into(),
            speaker: None,
            turn: None,
            topic: None,
            cycle: None,
            description: "Test obs".into(),
            entities: None,
            quadrant: Some("UL".into()),
            trigger_digestion: None,
            trust: None,
        };
        let result = rt()
            .block_on(server.tdg_observe(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["observation_id"].as_str().unwrap().starts_with('n'));
    }

    #[test]
    fn observe_empty_fails() {
        let env = TestEnv::new();
        let server = env.server();
        let params = ObserveParams {
            text: "".into(),
            speaker: None,
            turn: None,
            topic: None,
            cycle: None,
            description: "".into(),
            entities: None,
            quadrant: None,
            trigger_digestion: None,
            trust: None,
        };
        assert!(rt()
            .block_on(server.tdg_observe(Parameters(params)))
            .is_err());
    }

    #[test]
    fn observe_wires_extracted_entities() {
        let env = TestEnv::new();
        let server = env.server();
        let params = ObserveParams {
            text: "Used rust and docker for deployment".into(),
            speaker: None,
            turn: None,
            topic: None,
            cycle: None,
            description: "Used rust and docker for deployment".into(),
            entities: None,
            quadrant: Some("LR".into()),
            trigger_digestion: None,
            trust: None,
        };
        let result = rt()
            .block_on(server.tdg_observe(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let obs_id = v["observation_id"].as_str().unwrap();

        // Verify observation was created
        assert!(obs_id.starts_with('n'));

        // Verify extracted entities were returned
        let extracted = v["extracted_entities"].as_array().unwrap();
        assert!(!extracted.is_empty());

        // Verify entities were wired into the graph.
        // The EntityExtractor classifies known tools (rust, docker, etc.) with
        // entity_type="tool", platforms with entity_type="platform", etc.
        // So the wired node_type is "tool", NOT "entity".
        env.pool
            .with_connection(|conn| {
                // Check for rust entity (extracted as node_type='tool')
                let rust_exists: bool = conn
                    .query_row(
                        "SELECT COUNT(*) > 0 FROM nodes WHERE node_type = 'tool' AND name = 'rust' AND valid_to IS NULL",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(false);
                assert!(rust_exists, "rust entity should exist (node_type='tool')");

                // Check for docker entity (extracted as node_type='tool')
                let docker_exists: bool = conn
                    .query_row(
                        "SELECT COUNT(*) > 0 FROM nodes WHERE node_type = 'tool' AND name = 'docker' AND valid_to IS NULL",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(false);
                assert!(docker_exists, "docker entity should exist (node_type='tool')");

                // Check for USES edges from observation to entities
                let mentions_count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM edges WHERE source_id = ?1 AND edge_type = 'USES' AND valid_to IS NULL",
                        [obs_id],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);
                assert!(mentions_count >= 2, "Should have at least 2 USES edges, got {}", mentions_count);

                Ok(())
            })
            .unwrap();
    }

    // ── tdg_get_related ───────────────────────────────────────────────────

    #[test]
    fn get_related_outgoing() {
        let env = TestEnv::new();
        let src = env.add_node("observation", "Source");
        let tgt = env.add_node("hypothesis", "Target");
        env.pool
            .with_connection(|conn| {
                crate::db::crud::add_edge(
                    conn,
                    &NewEdge {
                        source_id: src.clone(),
                        target_id: tgt,
                        edge_type: "EVIDENCES".into(),
                        ..Default::default()
                    },
                )
                .unwrap();
                Ok(())
            })
            .unwrap();
        let server = env.server();
        let params = GetRelatedParams {
            node_id: src,
            edge_type: None,
            direction: Some("out".into()),
            limit: None,
        };
        let result = rt()
            .block_on(server.tdg_get_related(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["total"], 1);
    }

    // ── tdg_get_schema ────────────────────────────────────────────────────

    #[test]
    fn schema_has_tables() {
        let env = TestEnv::new();
        let server = env.server();
        let result = rt().block_on(server.tdg_get_schema()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let tables = v["tables"].as_object().unwrap();
        assert!(tables.contains_key("nodes"));
        assert!(tables.contains_key("edges"));
        assert!(tables.contains_key("events"));
    }

    // ── tdg_maintenance ───────────────────────────────────────────────────

    #[test]
    fn maintenance_hygiene() {
        let env = TestEnv::new();
        let server = env.server();
        let params = MaintenanceParams {
            action: Some("hygiene".into()),
            batch_size: None,
            phase: None,
            dry_run: None,
        };
        let result = rt()
            .block_on(server.tdg_maintenance(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("orphan_count").is_some());
    }

    // ── tdg_bank ──────────────────────────────────────────────────────────

    #[test]
    fn bank_list_empty() {
        let env = TestEnv::new();
        let server = env.server();
        let params = BankParams {
            action: Some("list".into()),
            profile: None,
            bank_id: None,
            node_type: None,
            limit: None,
        };
        let result = rt().block_on(server.tdg_bank(Parameters(params))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["banks"].as_array().unwrap().is_empty());
    }

    // ── tdg_entity ────────────────────────────────────────────────────────

    #[test]
    fn entity_resolve_empty() {
        let env = TestEnv::new();
        let server = env.server();
        let params = EntityParams {
            name: Some("nonexistent".into()),
            text: None,
            node_id: None,
            action: Some("resolve".into()),
        };
        let result = rt()
            .block_on(server.tdg_entity(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["entities"].as_array().unwrap().is_empty());
    }

    // ── tdg_bulk_create ───────────────────────────────────────────────────

    #[test]
    fn bulk_create_basic() {
        let env = TestEnv::new();
        let server = env.server();
        let nodes_json = serde_json::json!([
            {"node_type": "observation", "name": "Bulk 1"},
            {"node_type": "observation", "name": "Bulk 2"},
        ])
        .to_string();
        let params = BulkCreateParams {
            nodes: vec![],
            nodes_json,
            edges_json: None,
        };
        let result = rt()
            .block_on(server.tdg_bulk_create(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["created_nodes"], 2);
    }

    // ── tdg_record_exec ───────────────────────────────────────────────────

    #[test]
    fn record_exec_basic() {
        let env = TestEnv::new();
        let server = env.server();
        let params = RecordExecParams {
            node_id: "test_node".into(),
            helpful: true,
            reason: None,
            action_type: "deploy".into(),
            description: "Deployed v1.0".into(),
            metrics_json: None,
            result: "success".into(),
            tags: None,
        };
        let result = rt()
            .block_on(server.tdg_record_exec(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["observation_id"].as_str().unwrap().starts_with('n'));
    }

    // ── tdg_reflect ───────────────────────────────────────────────────────

    #[test]
    fn reflect_checks_ollama() {
        let env = TestEnv::new();
        let server = env.server();
        let params = ReflectParams {
            turns: None,
            focus_topics: None,
            status_only: Some(true),
        };
        let result = rt()
            .block_on(server.tdg_reflect(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("status").is_some());
    }

    #[test]
    fn reflect_pattern_fallback_with_focus_topics() {
        let env = TestEnv::new();
        let server = env.server();

        env.pool
            .with_connection(|conn| {
                for (name, desc) in vec![
                    (
                        "Rust async runtime",
                        "Tokio runtime with epoll for async I/O",
                    ),
                    (
                        "Garden planning",
                        "Planning the vegetable garden for summer",
                    ),
                    (
                        "Rust borrow checker",
                        "Understanding lifetime annotations in Rust",
                    ),
                    ("Grocery shopping", "Weekly grocery list and meal prep"),
                    (
                        "Rust trait objects",
                        "dyn Trait vs generics for dynamic dispatch",
                    ),
                ]
                .into_iter()
                {
                    let _ = crate::db::crud::add_node(
                        conn,
                        &crate::models::NewNode {
                            node_type: "observation".into(),
                            name: name.into(),
                            description: Some(desc.into()),
                            ..Default::default()
                        },
                    );
                }
                let _ = crate::db::crud::add_node(
                    conn,
                    &crate::models::NewNode {
                        node_type: "people".into(),
                        name: "Alice".into(),
                        ..Default::default()
                    },
                );
                Ok(())
            })
            .unwrap();

        let params = ReflectParams {
            turns: Some(50),
            focus_topics: Some("rust".into()),
            status_only: None,
        };
        let result = rt()
            .block_on(server.tdg_reflect(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(v["status"], "ok");
        assert_eq!(v["method"], "pattern");

        let insights = v["insights"].as_array().unwrap();
        assert!(!insights.is_empty());

        let synthesis_nodes = v["synthesis_nodes"].as_array().unwrap();
        assert!(
            !synthesis_nodes.is_empty(),
            "pattern fallback should create synthesis nodes"
        );

        let insights_text: String = insights
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            insights_text.contains("rust") || insights_text.contains("Rust"),
            "focus topic 'rust' should appear in insights: {}",
            insights_text
        );
    }

    #[test]
    fn reflect_empty_graph_returns_error() {
        let env = TestEnv::new();
        let server = env.server();
        let params = ReflectParams {
            turns: Some(10),
            focus_topics: Some("anything".into()),
            status_only: None,
        };
        let result = rt()
            .block_on(server.tdg_reflect(Parameters(params)))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["status"], "error");
        assert!(v["error"].as_str().unwrap().contains("No graph context"));
    }
}
