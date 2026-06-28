use proptest::prelude::*;
use tdg_rust::db::crud;
use tdg_rust::models::{NewNode, NodeQuery};

fn arb_node_name() -> impl Strategy<Value = String> {
    "[A-Za-z0-9 ]{1,50}"
}

fn arb_node_type() -> impl Strategy<Value = String> {
    prop::sample::select(tdg_rust::models::NODE_TYPES.to_vec()).prop_map(|s| s.to_string())
}

fn setup_test_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    tdg_rust::init_schema(&conn).unwrap();
    tdg_rust::run_migrations(&conn).unwrap();
    conn
}

proptest! {
    #[test]
    fn node_creation_never_panics(
        name in arb_node_name(),
        node_type in arb_node_type(),
        description in "[A-Za-z0-9 ]{0,200}",
    ) {
        let conn = setup_test_db();
        let result = crud::add_node(&conn, &NewNode {
            node_type,
            name,
            description: Some(description),
            ..Default::default()
        });
        prop_assert!(result.is_ok(), "add_node panicked or errored: {:?}", result.err());
    }

    #[test]
    fn node_retrieval_by_id(
        name in arb_node_name(),
        node_type in arb_node_type(),
    ) {
        let conn = setup_test_db();
        let node = crud::add_node(&conn, &NewNode {
            node_type,
            name: name.clone(),
            ..Default::default()
        }).unwrap();
        let fetched = crud::get_node(&conn, &node.id).unwrap();
        prop_assert!(fetched.is_some(), "Node not found after creation");
        prop_assert_eq!(fetched.unwrap().name, name);
    }

    #[test]
    fn node_soft_delete(
        name in arb_node_name(),
        node_type in arb_node_type(),
    ) {
        let conn = setup_test_db();
        let node = crud::add_node(&conn, &NewNode {
            node_type,
            name,
            ..Default::default()
        }).unwrap();
        let deleted = crud::delete_node(&conn, &node.id).unwrap();
        prop_assert!(deleted);
        let fetched = crud::get_node(&conn, &node.id).unwrap();
        prop_assert!(fetched.is_none(), "Soft-deleted node still visible");
    }

    #[test]
    fn search_deterministic(
        query in "[A-Za-z]{1,20}",
    ) {
        let conn = setup_test_db();
        let _ = crud::add_node(&conn, &NewNode {
            node_type: "observation".to_string(),
            name: format!("Test {}", query),
            description: Some(format!("A test about {}", query)),
            ..Default::default()
        });
        tdg_rust::init_fts(&conn).unwrap();

        let r1 = crud::search(&conn, &query, 10);
        let r2 = crud::search(&conn, &query, 10);
        prop_assert!(r1.is_ok());
        prop_assert!(r2.is_ok());
        let v1 = r1.unwrap();
        let v2 = r2.unwrap();
        prop_assert_eq!(v1.len(), v2.len());
        for (a, b) in v1.iter().zip(v2.iter()) {
            prop_assert_eq!(&a.0.id, &b.0.id);
        }
    }

    #[test]
    fn edge_creation_never_panics(
        node_name in arb_node_name(),
        node_type in arb_node_type(),
    ) {
        let conn = setup_test_db();
        let n1 = crud::add_node(&conn, &NewNode {
            node_type: node_type.clone(),
            name: node_name.clone(),
            ..Default::default()
        }).unwrap();
        let n2 = crud::add_node(&conn, &NewNode {
            node_type,
            name: format!("{}-target", node_name),
            ..Default::default()
        }).unwrap();
        let edge = crud::add_edge(&conn, &tdg_rust::models::NewEdge {
            source_id: n1.id.clone(),
            target_id: n2.id.clone(),
            edge_type: "DEPENDS_ON".to_string(),
            ..Default::default()
        });
        prop_assert!(edge.is_ok(), "add_edge errored: {:?}", edge.err());
    }

    #[test]
    fn query_nodes_filter_by_type(
        names in prop::collection::vec(arb_node_name(), 1..10),
    ) {
        let conn = setup_test_db();
        let mut skill_ids = Vec::new();
        for name in &names {
            let node = crud::add_node(&conn, &NewNode {
                node_type: "skill".to_string(),
                name: name.clone(),
                ..Default::default()
            }).unwrap();
            skill_ids.push(node.id);
        }
        let _ = crud::add_node(&conn, &NewNode {
            node_type: "observation".to_string(),
            name: "not-a-skill".to_string(),
            ..Default::default()
        }).unwrap();

        let query = NodeQuery {
            node_type: Some("skill".to_string()),
            limit: Some(100),
            ..Default::default()
        };
        let results = crud::query_nodes(&conn, &query).unwrap();
        let unique_names: std::collections::HashSet<&str> = names.iter().map(|s| s.as_str()).collect();
        prop_assert_eq!(results.len(), unique_names.len());
        for r in &results {
            prop_assert_eq!(&r.node_type, "skill");
        }
    }

    #[test]
    fn node_update_never_panics(
        name in arb_node_name(),
        new_name in arb_node_name(),
        node_type in arb_node_type(),
    ) {
        let conn = setup_test_db();
        let node = crud::add_node(&conn, &NewNode {
            node_type,
            name,
            ..Default::default()
        }).unwrap();
        let mut updates = std::collections::HashMap::new();
        updates.insert("name".to_string(), serde_json::json!(new_name.clone()));
        let updated = crud::update_node(&conn, &node.id, &updates).unwrap();
        prop_assert!(updated.is_some());
        prop_assert_eq!(updated.unwrap().name, new_name);
    }

    #[test]
    fn batch_extract_never_panics(
        messages in prop::collection::vec("[A-Za-z0-9 ]{1,100}", 1..20),
    ) {
        let ext = tdg_rust::plugins::EntityExtractor::new();
        let refs: Vec<&str> = messages.iter().map(|s| s.as_str()).collect();
        let _ = ext.extract_from_messages(&refs, None);
    }

    #[test]
    fn preference_extract_never_panics(
        messages in prop::collection::vec("[A-Za-z ]{1,100}", 1..20),
    ) {
        let ext = tdg_rust::plugins::preference_extractor::PreferenceExtractor::new();
        let owned: Vec<String> = messages.into_iter().collect();
        let _ = ext.extract_from_messages(&owned);
    }
}
