use petgraph::graph::{DiGraph, NodeIndex};
use rusqlite::Connection;
use std::collections::HashMap;

use crate::db::crud;
use crate::error::TdgResult;

/// In-memory graph projection built from SQLite tables.
pub struct GraphProjection {
    pub graph: DiGraph<String, String>,
    pub node_map: HashMap<String, NodeIndex>,
}

impl GraphProjection {
    /// Build projection from all active nodes and edges.
    pub fn build(conn: &Connection) -> TdgResult<Self> {
        let mut graph = DiGraph::new();
        let mut node_map = HashMap::new();

        let nodes = crud::query_nodes(
            conn,
            &crate::models::NodeQuery {
                include_deleted: false,
                limit: Some(100000),
                ..Default::default()
            },
        )?;

        for node in &nodes {
            let idx = graph.add_node(node.id.clone());
            node_map.insert(node.id.clone(), idx);
        }

        let edges = crud::get_edges(conn, None, None, None, None, 100000)?;

        for edge in &edges {
            if let (Some(&src), Some(&tgt)) =
                (node_map.get(&edge.source_id), node_map.get(&edge.target_id))
            {
                graph.add_edge(src, tgt, edge.edge_type.clone());
            }
        }

        Ok(Self { graph, node_map })
    }

}
