use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::{IntoNodeReferences, EdgeRef};
use petgraph::dot::{Dot, Config};
use serde_json::{json, Value};
use std::collections::HashMap;

/// D3.js force-directed graph JSON export.
pub mod d3_json {
    use super::*;

    /// Export graph as D3.js-compatible JSON value.
    pub fn export(graph: &DiGraph<String, String>, _node_map: &HashMap<String, NodeIndex>) -> Value {
        let nodes: Vec<Value> = graph.node_references()
            .map(|(idx, weight)| {
                json!({
                    "id": idx.index(),
                    "label": weight,
                    "group": 1
                })
            })
            .collect();

        let links: Vec<Value> = graph.edge_references()
            .map(|edge| {
                json!({
                    "source": edge.source().index(),
                    "target": edge.target().index(),
                    "value": 1,
                    "label": edge.weight()
                })
            })
            .collect();

        json!({
            "nodes": nodes,
            "links": links
        })
    }

    /// Export graph as pretty-printed D3.js JSON string.
    pub fn export_string(graph: &DiGraph<String, String>, node_map: &HashMap<String, NodeIndex>) -> String {
        serde_json::to_string_pretty(&export(graph, node_map)).unwrap()
    }
}

/// DOT (Graphviz) format export.
pub mod dot_export {
    use super::*;

    /// Export graph as DOT format string.
    pub fn export(graph: &DiGraph<String, String>) -> String {
        format!("{:?}", Dot::with_config(graph, &[Config::EdgeNoLabel]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use petgraph::graph::DiGraph;

    fn build_test_graph() -> (DiGraph<String, String>, HashMap<String, NodeIndex>) {
        let mut graph = DiGraph::new();
        let mut node_map = HashMap::new();

        let n0 = graph.add_node("Node A".to_string());
        let n1 = graph.add_node("Node B".to_string());
        node_map.insert("Node A".to_string(), n0);
        node_map.insert("Node B".to_string(), n1);

        graph.add_edge(n0, n1, "EDGE".to_string());

        (graph, node_map)
    }

    #[test]
    fn d3_json_empty_graph() {
        let graph: DiGraph<String, String> = DiGraph::new();
        let node_map = HashMap::new();
        let val = d3_json::export(&graph, &node_map);
        assert_eq!(val["nodes"], json!([]));
        assert_eq!(val["links"], json!([]));
    }

    #[test]
    fn d3_json_with_data() {
        let (graph, node_map) = build_test_graph();
        let val = d3_json::export(&graph, &node_map);

        assert_eq!(val["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(val["links"].as_array().unwrap().len(), 1);

        let link = &val["links"][0];
        assert_eq!(link["source"], 0);
        assert_eq!(link["target"], 1);
        assert_eq!(link["label"], "EDGE");
    }

    #[test]
    fn d3_json_round_trip() {
        let (graph, node_map) = build_test_graph();
        let s = d3_json::export_string(&graph, &node_map);
        let parsed: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["links"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn dot_export_empty_graph() {
        let graph: DiGraph<String, String> = DiGraph::new();
        let dot = dot_export::export(&graph);
        assert!(dot.contains("digraph"));
    }

    #[test]
    fn dot_export_with_nodes() {
        let (graph, _node_map) = build_test_graph();
        let dot = dot_export::export(&graph);
        assert!(dot.contains("Node A"));
        assert!(dot.contains("Node B"));
    }
}
