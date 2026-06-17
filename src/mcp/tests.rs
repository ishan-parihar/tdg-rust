//! MCP Tool Tests — comprehensive testing for all 17 tools
//!
//! Uses temp-file-backed SQLite for isolation. Tests call tool methods directly
//! on TdgServer via tokio_test::block_on.

#[cfg(test)]
mod tests {
    use crate::db::{init_fts, init_schema, run_migrations};
    use crate::models::{NewEdge, NewNode};
    use crate::mcp::tools::{TdgServer, CreateParams, GetNodeParams, SearchParams, ConnectParams,
        QueryEventsParams, UpdateParams, RateMemoryParams, MindStateParams, ObserveParams,
        GetRelatedParams, MaintenanceParams, BankParams, EntityParams, BulkCreateParams,
        RecordExecParams, ReflectParams};
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
            }).unwrap();
            Self { _tmpfile: tmpfile, pool }
        }

        fn server(&self) -> TdgServer {
            TdgServer::new(crate::db::ConnectionPool::new(
                self._tmpfile.path().to_str().unwrap(), 5, 30000
            ).unwrap())
        }

        fn add_node(&self, node_type: &str, name: &str) -> String {
            self.pool.with_connection(|conn| {
                let node = crate::db::crud::add_node(conn, &NewNode {
                    node_type: node_type.to_string(),
                    name: name.to_string(),
                    ..Default::default()
                }).unwrap();
                Ok(node.id)
            }).unwrap()
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
            node_type: "observation".into(), name: "Test Node".into(),
            description: Some("A test".into()), quadrant: Some("LR".into()),
            parent_ids: None, t_level: Some("L1".into()), stage: None,
            lifecycle_state: None, source: None, blocks_targets: None, evidence_targets: None,
        };
        let result = rt().block_on(server.tdg_create(Parameters(params))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["id"].as_str().unwrap().starts_with('n'));
        assert_eq!(v["name"], "Test Node");
    }

    #[test]
    fn create_empty_name_fails() {
        let env = TestEnv::new();
        let server = env.server();
        let params = CreateParams {
            node_type: "observation".into(), name: "".into(),
            description: None, quadrant: None, parent_ids: None, t_level: None,
            stage: None, lifecycle_state: None, source: None,
            blocks_targets: None, evidence_targets: None,
        };
        assert!(rt().block_on(server.tdg_create(Parameters(params))).is_err());
    }

    // ── tdg_get_node ──────────────────────────────────────────────────────

    #[test]
    fn get_node_basic() {
        let env = TestEnv::new();
        let id = env.add_node("observation", "Find Me");
        let server = env.server();
        let params = GetNodeParams { node_id: id.clone(), include_context: Some(false) };
        let result = rt().block_on(server.tdg_get_node(Parameters(params))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["id"], id);
        assert_eq!(v["name"], "Find Me");
    }

    #[test]
    fn get_node_with_context() {
        let env = TestEnv::new();
        let id = env.add_node("observation", "Context Node");
        let server = env.server();
        let params = GetNodeParams { node_id: id, include_context: Some(true) };
        let result = rt().block_on(server.tdg_get_node(Parameters(params))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("neighbors").is_some());
    }

    #[test]
    fn get_node_not_found() {
        let env = TestEnv::new();
        let server = env.server();
        let params = GetNodeParams { node_id: "n_nonexistent".into(), include_context: None };
        assert!(rt().block_on(server.tdg_get_node(Parameters(params))).is_err());
    }

    // ── tdg_search ────────────────────────────────────────────────────────

    #[test]
    fn search_basic() {
        let env = TestEnv::new();
        env.add_node("observation", "Rust memory safety");
        env.add_node("observation", "Python GIL");
        let server = env.server();
        let params = SearchParams { query: "Rust".into(), node_type: None, limit: None };
        let result = rt().block_on(server.tdg_search(Parameters(params))).unwrap();
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
            source_id: src, target_id: tgt,
            as_edge: Some("DECOMPOSES_TO".into()), force: None,
        };
        let result = rt().block_on(server.tdg_connect(Parameters(params))).unwrap();
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
        let params = ConnectParams { source_id: src, target_id: tgt, as_edge: None, force: None };
        let result = rt().block_on(server.tdg_connect(Parameters(params))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["edge_type"], "EVOLVES_INTO");
    }

    #[test]
    fn connect_duplicate_detected() {
        let env = TestEnv::new();
        let src = env.add_node("action", "A");
        let tgt = env.add_node("action", "B");
        let server = env.server();
        let p = ConnectParams { source_id: src.clone(), target_id: tgt.clone(), as_edge: Some("USES".into()), force: None };
        rt().block_on(server.tdg_connect(Parameters(p))).unwrap();
        // Second connect should detect duplicate
        let p2 = ConnectParams { source_id: src, target_id: tgt, as_edge: Some("USES".into()), force: None };
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
            action: None, node_id: None, after: None, before: None, limit: None, offset: None,
        };
        let result = rt().block_on(server.tdg_query_events(Parameters(params))).unwrap();
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
            node_id: id, name: Some("Updated".into()),
            description: None, lifecycle_state: None, new_type: None,
            t_level: None, stage: None, add_parent_ids: None, remove_parent_ids: None,
        };
        let result = rt().block_on(server.tdg_update(Parameters(params))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["name"], "Updated");
    }

    // ── tdg_rate_memory ───────────────────────────────────────────────────

    #[test]
    fn rate_memory_helpful() {
        let env = TestEnv::new();
        let id = env.add_node("observation", "Rate Me");
        let server = env.server();
        let params = RateMemoryParams { node_id: id, helpful: true };
        let result = rt().block_on(server.tdg_rate_memory(Parameters(params))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["trust_score"].as_f64().unwrap() > 0.0);
    }

    // ── tdg_mind_state ────────────────────────────────────────────────────

    #[test]
    fn mind_state_counts() {
        let env = TestEnv::new();
        let server = env.server();
        let params = MindStateParams { detail: None, health: None, verify: None };
        let result = rt().block_on(server.tdg_mind_state(Parameters(params))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("total_nodes").is_some());
        assert!(v.get("by_type").is_some());
    }

    // ── tdg_observe ───────────────────────────────────────────────────────

    #[test]
    fn observe_basic() {
        let env = TestEnv::new();
        let server = env.server();
        let params = ObserveParams {
            description: "Test obs".into(), quadrant: Some("UL".into()),
            cycle: None, trust: None, entities: None,
        };
        let result = rt().block_on(server.tdg_observe(Parameters(params))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["observation_id"].as_str().unwrap().starts_with('n'));
    }

    #[test]
    fn observe_empty_fails() {
        let env = TestEnv::new();
        let server = env.server();
        let params = ObserveParams {
            description: "".into(), quadrant: None, cycle: None, trust: None, entities: None,
        };
        assert!(rt().block_on(server.tdg_observe(Parameters(params))).is_err());
    }

    // ── tdg_get_related ───────────────────────────────────────────────────

    #[test]
    fn get_related_outgoing() {
        let env = TestEnv::new();
        let src = env.add_node("observation", "Source");
        let tgt = env.add_node("hypothesis", "Target");
        env.pool.with_connection(|conn| {
            crate::db::crud::add_edge(conn, &NewEdge {
                source_id: src.clone(), target_id: tgt, edge_type: "EVIDENCES".into(), ..Default::default()
            }).unwrap();
            Ok(())
        }).unwrap();
        let server = env.server();
        let params = GetRelatedParams {
            node_id: src, edge_type: None, direction: Some("out".into()), limit: None,
        };
        let result = rt().block_on(server.tdg_get_related(Parameters(params))).unwrap();
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
        let params = MaintenanceParams { phase: Some("hygiene".into()), full: None };
        let result = rt().block_on(server.tdg_maintenance(Parameters(params))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("orphan_count").is_some());
    }

    // ── tdg_bank ──────────────────────────────────────────────────────────

    #[test]
    fn bank_list_empty() {
        let env = TestEnv::new();
        let server = env.server();
        let params = BankParams {
            action: Some("list".into()), profile: None, bank_id: None, node_type: None, limit: None,
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
            name: Some("nonexistent".into()), text: None, node_id: None,
            action: Some("resolve".into()),
        };
        let result = rt().block_on(server.tdg_entity(Parameters(params))).unwrap();
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
        ]).to_string();
        let params = BulkCreateParams { nodes_json, edges_json: None };
        let result = rt().block_on(server.tdg_bulk_create(Parameters(params))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["created_nodes"], 2);
    }

    // ── tdg_record_exec ───────────────────────────────────────────────────

    #[test]
    fn record_exec_basic() {
        let env = TestEnv::new();
        let server = env.server();
        let params = RecordExecParams {
            action_type: "deploy".into(), description: "Deployed v1.0".into(),
            result: "success".into(), tags: None, metrics_json: None,
        };
        let result = rt().block_on(server.tdg_record_exec(Parameters(params))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["observation_id"].as_str().unwrap().starts_with('n'));
    }

    // ── tdg_reflect ───────────────────────────────────────────────────────

    #[test]
    fn reflect_checks_ollama() {
        let env = TestEnv::new();
        let server = env.server();
        let params = ReflectParams { turns: None, focus_topics: None, status_only: Some(true) };
        let result = rt().block_on(server.tdg_reflect(Parameters(params))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("status").is_some());
    }
}
