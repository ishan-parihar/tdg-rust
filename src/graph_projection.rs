use petgraph::algo::dijkstra;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use rusqlite::Connection;
use std::collections::HashMap;

use crate::db::crud;

/// In-memory graph projection built from SQLite tables.
pub struct GraphProjection {
    pub graph: DiGraph<String, String>,
    pub node_map: HashMap<String, NodeIndex>,
}

impl GraphProjection {
    /// Build projection from all active nodes and edges.
    pub fn build(conn: &Connection) -> Result<Self, String> {
        let mut graph = DiGraph::new();
        let mut node_map = HashMap::new();

        let nodes = crud::query_nodes(
            conn,
            &crate::models::NodeQuery {
                include_deleted: false,
                limit: Some(100000),
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())?;

        for node in &nodes {
            let idx = graph.add_node(node.id.clone());
            node_map.insert(node.id.clone(), idx);
        }

        let edges =
            crud::get_edges(conn, None, None, None, None, 100000).map_err(|e| e.to_string())?;

        for edge in &edges {
            if let (Some(&src), Some(&tgt)) =
                (node_map.get(&edge.source_id), node_map.get(&edge.target_id))
            {
                graph.add_edge(src, tgt, edge.edge_type.clone());
            }
        }

        Ok(Self { graph, node_map })
    }

    /// Shortest path using Dijkstra (unweighted = BFS).
    pub fn shortest_path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        let &src = self.node_map.get(from)?;
        let &tgt = self.node_map.get(to)?;

        let costs = dijkstra(&self.graph, src, Some(tgt), |_| 1u32);

        if !costs.contains_key(&tgt) {
            return None;
        }

        let mut path = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        let mut visited = HashMap::new();
        queue.push_back((src, vec![self.graph[src].clone()]));
        visited.insert(src, true);

        while let Some((node, so_far)) = queue.pop_front() {
            if node == tgt {
                path = so_far;
                break;
            }
            for edge in self.graph.edges(node) {
                let next = edge.target();
                if let std::collections::hash_map::Entry::Vacant(e) = visited.entry(next) {
                    e.insert(true);
                    let mut new_path = so_far.clone();
                    new_path.push(self.graph[next].clone());
                    queue.push_back((next, new_path));
                }
            }
        }

        Some(path)
    }

    /// Count nodes and edges.
    pub fn stats(&self) -> (usize, usize) {
        (self.graph.node_count(), self.graph.edge_count())
    }

    /// Get all neighbors of a node.
    pub fn neighbors(&self, node_id: &str) -> Vec<String> {
        if let Some(&idx) = self.node_map.get(node_id) {
            self.graph
                .neighbors(idx)
                .map(|n| self.graph[n].clone())
                .collect()
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::{init_schema, run_migrations};
    use crate::models::NewNode;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn build_projection_empty() {
        let conn = setup();
        let proj = GraphProjection::build(&conn).unwrap();
        assert_eq!(proj.stats(), (0, 0));
    }

    #[test]
    fn build_projection_with_data() {
        let conn = setup();
        let n1 = crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".into(),
                name: "T1".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let n2 = crud::add_node(
            &conn,
            &NewNode {
                node_type: "action".into(),
                name: "A1".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let _ = crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: n1.id.clone(),
                target_id: n2.id.clone(),
                edge_type: "DECOMPOSES_TO".into(),
                ..Default::default()
            },
        );

        let proj = GraphProjection::build(&conn).unwrap();
        assert_eq!(proj.stats(), (2, 1));
        assert_eq!(proj.neighbors(&n1.id), vec![n2.id.clone()]);
    }

    #[test]
    fn shortest_path_exists() {
        let conn = setup();
        let n1 = crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".into(),
                name: "T1".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let n2 = crud::add_node(
            &conn,
            &NewNode {
                node_type: "action".into(),
                name: "A1".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let n3 = crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".into(),
                name: "O1".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let _ = crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: n1.id.clone(),
                target_id: n2.id.clone(),
                edge_type: "DECOMPOSES_TO".into(),
                ..Default::default()
            },
        );
        let _ = crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: n2.id.clone(),
                target_id: n3.id.clone(),
                edge_type: "EVIDENCES".into(),
                ..Default::default()
            },
        );

        let proj = GraphProjection::build(&conn).unwrap();
        let path = proj.shortest_path(&n1.id, &n3.id).unwrap();
        assert_eq!(path.len(), 3);
    }

    #[test]
    fn shortest_path_no_route() {
        let conn = setup();
        let n1 = crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".into(),
                name: "T1".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let n2 = crud::add_node(
            &conn,
            &NewNode {
                node_type: "action".into(),
                name: "A1".into(),
                ..Default::default()
            },
        )
        .unwrap();

        let proj = GraphProjection::build(&conn).unwrap();
        assert!(proj.shortest_path(&n1.id, &n2.id).is_none());
    }
}
