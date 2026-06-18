use petgraph::graph::DiGraph;
use petgraph::visit::IntoNodeReferences;
use rustworkx_core::centrality;
use std::collections::{HashMap, HashSet, VecDeque};

/// Betweenness centrality for all nodes, mapped by node label.
pub fn betweenness_centrality(graph: &DiGraph<String, String>) -> HashMap<String, f64> {
    let scores = centrality::betweenness_centrality(graph, true, false, 50);
    graph
        .node_references()
        .map(|(idx, weight)| (weight.clone(), scores[idx.index()].unwrap_or(0.0)))
        .collect()
}

/// Out-degree centrality for all nodes, mapped by node label.
pub fn degree_centrality(graph: &DiGraph<String, String>) -> HashMap<String, f64> {
    let scores = centrality::degree_centrality(graph, None);
    graph
        .node_references()
        .map(|(idx, weight)| (weight.clone(), scores[idx.index()]))
        .collect()
}

/// Check weak connectivity via BFS on the underlying undirected graph.
pub fn is_connected(graph: &DiGraph<String, String>) -> bool {
    if graph.node_count() == 0 {
        return true;
    }
    let start = graph.node_indices().next().unwrap();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(start);
    visited.insert(start);
    while let Some(node) = queue.pop_front() {
        // Follow both outgoing and incoming edges for weak connectivity
        for neighbor in graph.neighbors_undirected(node) {
            if visited.insert(neighbor) {
                queue.push_back(neighbor);
            }
        }
    }
    visited.len() == graph.node_count()
}

/// Edge count / max possible edges.
pub fn graph_density(graph: &DiGraph<String, String>) -> f64 {
    let n = graph.node_count() as f64;
    let m = graph.edge_count() as f64;
    if n <= 1.0 {
        return 0.0;
    }
    m / (n * (n - 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use petgraph::graph::NodeIndex;

    fn chain(n: usize) -> DiGraph<String, String> {
        let mut g = DiGraph::new();
        for i in 0..n {
            g.add_node(format!("N{i}"));
        }
        for i in 0..n - 1 {
            let src = NodeIndex::new(i);
            let tgt = NodeIndex::new(i + 1);
            g.add_edge(src, tgt, "edge".into());
        }
        g
    }

    #[test]
    fn betweenness_empty_graph() {
        let g: DiGraph<String, String> = DiGraph::new();
        let scores = betweenness_centrality(&g);
        assert!(scores.is_empty());
    }

    #[test]
    fn betweenness_linear_chain_middle_highest() {
        let g = chain(5);
        let scores = betweenness_centrality(&g);
        let middle = scores.get("N2").copied().unwrap();
        let end = scores.get("N0").copied().unwrap();
        assert!(middle > end, "middle={middle} should be > end={end}");
    }

    #[test]
    fn degree_star_center_highest() {
        // Star: center=0, leaves 1..5, all edges center->leaf
        let mut g = DiGraph::new();
        g.add_node("center".into());
        for i in 1..=5 {
            g.add_node(format!("L{i}"));
        }
        let center = NodeIndex::new(0);
        for i in 1..=5 {
            g.add_edge(center, NodeIndex::new(i), "e".into());
        }
        let scores = degree_centrality(&g);
        let center_score = scores["center"];
        let leaf_score = scores["L1"];
        assert!(center_score > leaf_score);
    }

    #[test]
    fn connected_true() {
        let g = chain(4);
        assert!(is_connected(&g));
    }

    #[test]
    fn connected_false() {
        let mut g: DiGraph<String, String> = DiGraph::new();
        g.add_node("A".into());
        g.add_node("B".into());
        // No edges -> disconnected
        assert!(!is_connected(&g));
    }

    #[test]
    fn density_empty() {
        let g: DiGraph<String, String> = DiGraph::new();
        assert_eq!(graph_density(&g), 0.0);
    }

    #[test]
    fn density_complete() {
        // 3 nodes, all 6 directed edges
        let mut g = DiGraph::new();
        g.add_node("A".into());
        g.add_node("B".into());
        g.add_node("C".into());
        let a = NodeIndex::new(0);
        let b = NodeIndex::new(1);
        let c = NodeIndex::new(2);
        g.add_edge(a, b, "e".into());
        g.add_edge(a, c, "e".into());
        g.add_edge(b, a, "e".into());
        g.add_edge(b, c, "e".into());
        g.add_edge(c, a, "e".into());
        g.add_edge(c, b, "e".into());
        assert_eq!(graph_density(&g), 1.0);
    }

    #[test]
    fn density_single_node() {
        let mut g = DiGraph::new();
        g.add_node("A".into());
        assert_eq!(graph_density(&g), 0.0);
    }
}
