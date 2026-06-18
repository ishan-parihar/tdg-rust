use rusqlite::Connection;
use tempfile::TempDir;

use crate::db::crud;
use crate::error::TdgResult;
use crate::models::{Edge, NewEdge, NewNode, Node};

pub struct TestDb {
    pub _dir: TempDir,
    pub conn: Connection,
}

impl TestDb {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;")
            .unwrap();
        crate::db::init_schema(&conn).unwrap();
        crate::db::init_fts(&conn).unwrap();
        crate::db::run_migrations(&conn).unwrap();
        Self { _dir: dir, conn }
    }

    pub fn new_in_memory() -> Self {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        crate::db::init_fts(&conn).unwrap();
        crate::db::run_migrations(&conn).unwrap();
        // ponytail: need _dir to keep uniform type; tempdir is cheap
        let dir = tempfile::tempdir().unwrap();
        Self { _dir: dir, conn }
    }

    pub fn create_node(&self, node_type: &str, name: &str) -> TdgResult<Node> {
        crud::add_node(
            &self.conn,
            &NewNode {
                node_type: node_type.to_string(),
                name: name.to_string(),
                ..Default::default()
            },
        )
    }

    pub fn create_person(&self, name: &str, aliases: &[&str]) -> TdgResult<Node> {
        let props = if aliases.is_empty() {
            None
        } else {
            let alias_vals: Vec<serde_json::Value> =
                aliases.iter().map(|a| serde_json::json!(a)).collect();
            Some(serde_json::json!({ "aliases": alias_vals }))
        };

        crud::add_node(
            &self.conn,
            &NewNode {
                node_type: "people".to_string(),
                name: name.to_string(),
                properties: props,
                ..Default::default()
            },
        )
    }

    pub fn create_edge(
        &self,
        source_id: &str,
        target_id: &str,
        edge_type: &str,
    ) -> TdgResult<Edge> {
        crud::add_edge(
            &self.conn,
            &NewEdge {
                source_id: source_id.to_string(),
                target_id: target_id.to_string(),
                edge_type: edge_type.to_string(),
                ..Default::default()
            },
        )
    }

    pub fn create_observation(&self, name: &str, source: &str) -> TdgResult<Node> {
        crud::add_node(
            &self.conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: name.to_string(),
                source: Some(source.to_string()),
                ..Default::default()
            },
        )
    }

    pub fn count(&self, table: &str) -> i64 {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        self.conn
            .query_row(&sql, [], |row| row.get(0))
            .unwrap_or(0)
    }

    pub fn count_nodes(&self) -> i64 {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0)
    }
}

#[cfg(test)]
pub fn arb_node_type() -> impl proptest::strategy::Strategy<Value = String> {
    let types: Vec<_> = crate::models::NODE_TYPES
        .iter()
        .map(|s| s.to_string())
        .collect();
    proptest::sample::select(types)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_in_memory_creates_schema() {
        let td = TestDb::new_in_memory();
        assert!(td.count_nodes() == 0);
        assert!(td.count("edges") == 0);
    }

    #[test]
    fn test_db_file_creates_schema() {
        let td = TestDb::new();
        assert!(td.count_nodes() == 0);
    }

    #[test]
    fn create_node_and_count() {
        let td = TestDb::new_in_memory();
        let node = td.create_node("observation", "Test Node").unwrap();
        assert_eq!(node.name, "Test Node");
        assert_eq!(td.count_nodes(), 1);
    }

    #[test]
    fn create_person_with_aliases() {
        let td = TestDb::new_in_memory();
        let person = td.create_person("Alice Smith", &["ali", "asmith"]).unwrap();
        assert_eq!(person.node_type, "people");
        let props = person.properties;
        let aliases = props.get("aliases").and_then(|v| v.as_array()).unwrap();
        assert_eq!(aliases.len(), 2);
    }

    #[test]
    fn create_edge_works() {
        let td = TestDb::new_in_memory();
        let n1 = td.create_node("skill", "Rust").unwrap();
        let n2 = td.create_node("project", "TDG").unwrap();
        let edge = td.create_edge(&n1.id, &n2.id, "DEPENDS_ON").unwrap();
        assert_eq!(edge.edge_type, "DEPENDS_ON");
    }
}
