# TDG-Rust

![Rust](https://img.shields.io/badge/Rust-1.78+-orange?logo=rust)
![License](https://img.shields.io/badge/License-MIT-green)
![MCP](https://img.shields.io/badge/MCP-1.0-orange?logo=modelcontextprotocol)
![Memory](https://img.shields.io/badge/Memory-TDG-purple)
![Neural](https://img.shields.io/badge/Type-Neural_Infrastructure-blue)


**Teleological Developmental Graph** — a self-structuring neural memory infrastructure for AI agents. Not a database. A brain.

![TDG architecture](https://github.com/ishan-parihar/tdg-rust/raw/main/assets/readme/tdg-architecture.png)

---

## What it is

| Property | Description |
|----------|-------------|
| **Self-structuring** | Nodes form hierarchies, sequences, and semantic clusters autonomously |
| **Teleological** | Every node has a *telos* (purpose) — drive, goal, or function |
| **Developmental** | Structure evolves through interaction, not schema migration |
| **Neural-symbolic** | Vector embeddings + symbolic edges = queryable, explainable memory |
| **Agent-native** | Built for LLM agents: MCP server, tool interface, streaming updates |

---

## Quick start

```bash
# Build
cargo build --release

# Run MCP server (for agents)
./target/release/tdg-mcp

# Or run CLI
./target/release/tdg --help
```

**MCP tools for agents:**

| Tool | Purpose |
|------|---------|
| `tdg.remember` | Store observation with telos |
| `tdg.recall` | Query by embedding + symbolic constraints |
| `tdg.link` | Create semantic/teleological edges |
| `tdg.evolve` | Trigger structural reorganization |
| `tdg.inspect` | Debug graph topology |

---

## Core concepts

### Node = (Content, Embedding, Telos)

```rust
pub struct Node {
    pub id: Uuid,
    pub content: String,
    pub embedding: Vector<1024>,
    pub telos: Telos,           // Drive: Curiosity, Agency, Communion, etc.
    pub developmental_stage: u8, // 0=seed → 255=crystallized
    pub edges: Vec<Edge>,
}
```

### Telos (the "why")

Every node carries a **telos** — its generative drive:

| Telos | Archetype | Function |
|-------|-----------|----------|
| `Curiosity` | Seeker | Explore, question, gather |
| `Agency` | Builder | Act, decide, construct |
| `Communion` | Connector | Relate, synthesize, share |
| `Stability` | Guardian | Preserve, validate, bound |

### Developmental stages

```
Seed (0-31) → Sprout (32-63) → Growth (64-127) 
    → Maturation (128-191) → Crystallization (192-255)
```

Nodes crystallize when their telos is repeatedly satisfied — becoming stable knowledge.

---


## Example: Agent learns a workflow

```python
# Agent observes user's deploy workflow
tdg.remember("User runs: docker build -t app .", telos="Curiosity")
tdg.remember("Then: docker push registry/app", telos="Curiosity")
tdg.remember("Then: kubectl rollout restart deployment/app", telos="Curiosity")

# Agent links them as sequence
tdg.link(node1, node2, edge_type="Sequence")
tdg.link(node2, node3, edge_type="Sequence")

# Later: agent recalls deploy pattern
pattern = tdg.recall("deploy workflow", telos="Agency")
# Returns: [build, push, rollout] with 0.94 similarity
```

---

## Performance

| Metric | Value |
|--------|-------|
| Insert latency | < 2 ms (10K nodes) |
| Recall (k=10) | < 5 ms (100K nodes) |
| Graph traversal | < 1 ms (3 hops) |
| Memory | ~200 MB (100K nodes) |
| Persistence | Sled (ACID, embedded) |

---



## Visual proof

| Architecture | Terminal graph | Memory view |
|:---:|:---:|:---:|
| ![Arch](https://github.com/ishan-parihar/tdg-rust/raw/main/assets/readme/arch.png) | ![Graph](https://github.com/ishan-parihar/tdg-rust/raw/main/assets/readme/graph.png) | ![Memory](https://github.com/ishan-parihar/tdg-rust/raw/main/assets/readme/memory.png) |

| Consolidation | Concept map | MCP tools |
|:---:|:---:|:---:|
| ![Consolidation](https://github.com/ishan-parihar/tdg-rust/raw/main/assets/readme/consolidation.png) | ![Concepts](https://github.com/ishan-parihar/tdg-rust/raw/main/assets/readme/concepts.png) | ![MCP](https://github.com/ishan-parihar/tdg-rust/raw/main/assets/readme/mcp.png) |

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                      TDG Core (Rust)                          │
├──────────────────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐      │
│  │  Store   │  │  Index   │  │  Graph   │  │  Evolution│     │
│  │ (Sled)   │  │ (HNSW)   │  │ (Petgraph)│ │ (Scheduler)│     │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘      │
└──────────────────────────────────────────────────────────────┘
         │            │            │            │
         ▼            ▼            ▼            ▼
┌──────────────────────────────────────────────────────────────┐
│                     MCP Server (FastMCP)                      │
│  remember / recall / link / evolve / inspect                  │
└──────────────────────────────────────────────────────────────┘
```

---

## Requirements

- Rust 1.78+
- 512 MB RAM minimum
- Linux/macOS (Windows WSL2)

---

## License

MIT — see [LICENSE](LICENSE).
