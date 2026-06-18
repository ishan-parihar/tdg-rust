use petgraph::graph::DiGraph;
use petgraph::visit::{IntoNodeReferences, EdgeRef};
use petgraph::dot::{Dot, Config};
use serde_json::{json, Value};

/// D3.js force-directed graph JSON export.
pub mod d3_json {
    use super::*;

    pub fn export(graph: &DiGraph<String, String>) -> Value {
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

    pub fn export_string(graph: &DiGraph<String, String>) -> String {
        serde_json::to_string_pretty(&export(graph)).expect("D3 JSON serialization failed")
    }
}

/// HTML visualization export using D3.js.
pub mod html_export {
    use super::*;

    /// Generate a complete HTML file with embedded D3.js force-directed graph.
    ///
    /// Uses D3.js v7 from CDN. No large libraries embedded.
    pub fn generate(graph: &DiGraph<String, String>) -> String {
        let data = d3_json::export_string(graph);

        format!(
            r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TDG Graph Visualization</title>
<style>
  :root {{
    --bg: #0a0a0b;
    --surface: #111113;
    --border: #27272a;
    --text: #fafafa;
    --text-secondary: #a1a1aa;
    --text-muted: #71717a;
    --accent: #6366f1;
  }}
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{
    font-family: system-ui, -apple-system, sans-serif;
    background: var(--bg);
    color: var(--text);
    overflow: hidden;
    min-height: 100vh;
  }}
  #header {{
    position: fixed; top: 0; left: 0; right: 0; z-index: 100;
    background: rgba(10, 10, 11, 0.85);
    backdrop-filter: blur(12px);
    border-bottom: 1px solid var(--border);
    padding: 12px 24px;
    display: flex; align-items: center; justify-content: space-between;
  }}
  #header h1 {{
    font-size: 14px; font-weight: 500; color: var(--text);
    display: flex; align-items: center; gap: 8px;
  }}
  #header .accent {{ color: var(--accent); }}
  #header .meta {{ font-size: 12px; color: var(--text-muted); }}
  #controls {{
    position: fixed; bottom: 24px; left: 50%; transform: translateX(-50%);
    z-index: 100; background: var(--surface);
    border: 1px solid var(--border); border-radius: 12px;
    padding: 8px 16px; display: flex; gap: 8px; align-items: center;
    backdrop-filter: blur(12px);
  }}
  #controls button {{
    background: var(--surface-2, #18181b);
    border: 1px solid var(--border); color: var(--text-secondary);
    font-family: inherit; font-size: 12px; padding: 6px 12px;
    border-radius: 6px; cursor: pointer; transition: all 0.15s ease;
  }}
  #controls button:hover, #controls button.active {{
    background: var(--accent); color: white; border-color: var(--accent);
  }}
  #canvas {{ width: 100vw; height: 100vh; display: block; }}
  #canvas svg {{ width: 100%; height: 100%; }}
  #tooltip {{
    position: fixed; z-index: 200;
    background: var(--surface); border: 1px solid var(--border);
    border-radius: 10px; padding: 12px 16px; font-size: 13px;
    line-height: 1.5; max-width: 360px; pointer-events: none;
    opacity: 0; transition: opacity 0.15s ease;
    box-shadow: 0 8px 32px rgba(0,0,0,0.4);
  }}
  #tooltip.visible {{ opacity: 1; }}
  #tooltip .tt-title {{ font-weight: 600; font-size: 14px; margin-bottom: 4px; }}
  #tooltip .tt-id {{ font-family: monospace; font-size: 10px; color: var(--text-muted); margin-bottom: 8px; }}
  #tooltip .tt-row {{
    display: flex; justify-content: space-between; gap: 12px;
    font-size: 11px; color: var(--text-secondary); padding: 2px 0;
  }}
  #tooltip .tt-row .label {{ color: var(--text-muted); }}
</style>
</head>
<body>
<div id="header">
  <h1><span class="accent">TDG</span> Graph Visualization
    <span class="meta">— <span id="node-count">0</span> nodes, <span id="edge-count">0</span> edges</span>
  </h1>
</div>
<div id="canvas"></div>
<div id="tooltip"></div>
<div id="controls">
  <button id="btn-reset" class="active">Reset View</button>
  <button id="btn-force">Force Layout</button>
  <button id="btn-tree">Tree Layout</button>
</div>

<script src="https://d3js.org/d3.v7.min.js"></script>
<script>
const tooltip = document.getElementById('tooltip');
const data = {data};

document.getElementById('node-count').textContent = data.nodes.length;
document.getElementById('edge-count').textContent = data.links.length;

const container = document.getElementById('canvas');
const width = window.innerWidth;
const height = window.innerHeight;

const svg = d3.select('#canvas').append('svg')
  .attr('width', width).attr('height', height);

// Background grid
const defs = svg.append('defs');
defs.append('pattern')
  .attr('id', 'grid').attr('width', 40).attr('height', 40)
  .attr('patternUnits', 'userSpaceOnUse')
  .append('path')
  .attr('d', 'M 40 0 L 0 0 0 40')
  .attr('fill', 'none').attr('stroke', '#1e293b').attr('stroke-width', '0.5');

svg.append('rect')
  .attr('width', width).attr('height', height).attr('fill', 'url(#grid)');

// Arrow markers
['arrow', '#334155'].forEach(([id, color]) => {{
  defs.append('marker')
    .attr('id', id).attr('viewBox', '0 0 10 10')
    .attr('refX', 22).attr('refY', 5)
    .attr('markerWidth', 6).attr('markerHeight', 6)
    .attr('orient', 'auto-start-reverse')
    .append('path').attr('d', 'M 0 0 L 10 5 L 0 10 z').attr('fill', color);
}});

const zoom = d3.zoom()
  .scaleExtent([0.1, 8])
  .on('zoom', (event) => {{ g.attr('transform', event.transform); }});

const g = svg.append('g');
svg.call(zoom);

function render(layoutMode = 'force') {{
  const nodes = data.nodes.map(d => Object.assign({{}}, d));
  const links = data.links.map(d => Object.assign({{}}, d));
  g.selectAll('*').remove();

  // Edges
  const link = g.append('g').selectAll('line')
    .data(links).join('line')
    .attr('stroke', '#334155').attr('stroke-width', 1)
    .attr('stroke-opacity', 0.6)
    .attr('marker-end', 'url(#arrow)');

  // Nodes
  const node = g.append('g').selectAll('g')
    .data(nodes).join('g')
    .attr('cursor', 'grab')
    .call(d3.drag()
      .on('start', (event, d) => {{ if (!event.active) simulation.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; }})
      .on('drag', (event, d) => {{ d.fx = event.x; d.fy = event.y; }})
      .on('end', (event, d) => {{ if (!event.active) simulation.alphaTarget(0); d.fx = null; d.fy = null; }})
    );

  node.append('circle')
    .attr('r', 20).attr('fill', 'rgba(99, 102, 241, 0.15)')
    .attr('stroke', '#6366f1').attr('stroke-width', 2);

  node.append('text')
    .attr('text-anchor', 'middle').attr('dy', 4)
    .attr('font-size', '10px').attr('fill', '#fafafa')
    .attr('pointer-events', 'none')
    .text(d => {{ const t = d.label; return t.length > 18 ? t.slice(0, 16) + '...' : t; }});

  // Hover
  node.on('mouseenter', function(event, d) {{
    d3.select(this).select('circle').transition().duration(200).attr('stroke-width', 4);
    tooltip.innerHTML = `<div class="tt-title">${{d.label}}</div><div class="tt-id">Node ${{d.id}}</div>`;
    tooltip.classList.add('visible');
    moveTooltip(event);
  }});
  node.on('mouseleave', function() {{
    d3.select(this).select('circle').transition().duration(200).attr('stroke-width', 2);
    tooltip.classList.remove('visible');
  }});
  node.on('mousemove', moveTooltip);

  function moveTooltip(event) {{
    let x = event.clientX + 16, y = event.clientY + 16;
    if (x + 360 > window.innerWidth) x = event.clientX - 376;
    if (y + 200 > window.innerHeight) y = event.clientY - 216;
    tooltip.style.left = x + 'px';
    tooltip.style.top = y + 'px';
  }}

  let simulation;
  if (layoutMode === 'force') {{
    simulation = d3.forceSimulation(nodes)
      .force('link', d3.forceLink(links).id(d => d.id).distance(100))
      .force('charge', d3.forceManyBody().strength(-250))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collision', d3.forceCollide().radius(30))
      .on('tick', () => {{
        link.attr('x1', d => d.source.x).attr('y1', d => d.source.y)
            .attr('x2', d => d.target.x).attr('y2', d => d.target.y);
        node.attr('transform', d => `translate(${{d.x}},${{d.y}})`);
      }});
  }} else {{
    // Tree layout
    const spacingX = width / (nodes.length + 1);
    nodes.forEach((n, i) => {{ n.x = (i + 1) * spacingX; n.y = height / 2; }});
    simulation = null;
    link.attr('x1', d => d.source.x).attr('y1', d => d.source.y)
        .attr('x2', d => d.target.x).attr('y2', d => d.target.y);
    node.attr('transform', d => `translate(${{d.x}},${{d.y}})`);
  }}
}}

render('force');

document.getElementById('btn-reset').onclick = () => {{
  svg.transition().duration(750).call(zoom.transform, d3.zoomIdentity);
}};
document.getElementById('btn-force').onclick = () => render('force');
document.getElementById('btn-tree').onclick = () => render('tree');
</script>
</body>
</html>"##
        )
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
    use petgraph::graph::{DiGraph, NodeIndex};
    use std::collections::HashMap;

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
        let val = d3_json::export(&graph);
        assert_eq!(val["nodes"], json!([]));
        assert_eq!(val["links"], json!([]));
    }

    #[test]
    fn d3_json_with_data() {
        let (graph, _node_map) = build_test_graph();
        let val = d3_json::export(&graph);

        assert_eq!(val["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(val["links"].as_array().unwrap().len(), 1);

        let link = &val["links"][0];
        assert_eq!(link["source"], 0);
        assert_eq!(link["target"], 1);
        assert_eq!(link["label"], "EDGE");
    }

    #[test]
    fn d3_json_round_trip() {
        let (graph, _node_map) = build_test_graph();
        let s = d3_json::export_string(&graph);
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

    #[test]
    fn html_export_generates_valid_html() {
        let (graph, _node_map) = build_test_graph();
        let html = html_export::generate(&graph);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("d3.v7.min.js"));
        assert!(html.contains("TDG Graph Visualization"));
        assert!(html.contains("Node A"));
        assert!(html.contains("Node B"));
        assert!(html.contains("EDGE"));
    }

    #[test]
    fn html_export_empty_graph() {
        let graph: DiGraph<String, String> = DiGraph::new();
        let html = html_export::generate(&graph);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("d3.v7.min.js"));
    }
}
