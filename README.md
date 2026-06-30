# tdg-rust

Rust implementation of the **Teleological Developmental Graph (TDG)** — a memory infrastructure for AI agents.

> **v0.4.0** | EmbeddingGemma ONNX backend | Configurable Q4/Q8 | Non-destructive migration

## Quick Install

```bash
curl -fsSL https://raw.githubusercontent.com/ishan-parihar/tdg-rust/main/install.sh | bash
```

This downloads the pre-built binary (no compilation needed) and configures the Hermes agent.

<details>
<summary>Manual install</summary>

```bash
# Download binary
curl -LO https://github.com/ishan-parihar/tdg-rust/releases/latest/download/tdg-rust
chmod +x tdg-rust

# Move to Hermes
mkdir -p ~/.hermes/tdg-rust
mv tdg-rust ~/.hermes/tdg-rust/

# Install adapter
mkdir -p ~/.hermes/plugins/tdg
curl -fsSL https://raw.githubusercontent.com/ishan-parihar/tdg-rust/main/plugins/tdg/__init__.py -o ~/.hermes/plugins/tdg/__init__.py
curl -fsSL https://raw.githubusercontent.com/ishan-parihar/tdg-rust/main/plugins/tdg/plugin.yaml -o ~/.hermes/plugins/tdg/plugin.yaml

# Initialize database
~/.hermes/tdg-rust/tdg-rust init
```

</details>

## What is TDG?

TDG is a graph-based memory system that gives AI agents persistent, structured knowledge. Nodes represent concepts (observations, hypotheses, skills, capabilities, projects), edges represent relationships, and a drive-based propagation engine models how ideas evolve through developmental stages.

### Core Concepts

| Concept | Description |
|---------|-------------|
| **21 Node Types** | observation, telos, skill, capability, action, people, artifact, hypothesis, constraint, discovery, project, trajectory, synthesis, being, communication, event, insight, question, value, bond, narrative |
| **35 Edge Types** | DECOMPOSES_TO, ENABLES, CONTEXT, BLOCKS, SUPPORTS, CONTRADICTS, EVIDENCES, RELATES_TO, and 27 more |
| **4 Dual-Pole Drives** | eros, agape, agency, communion — each with positive/negative poles |
| **8 Developmental Stages** | Evidence-gated stage progression with age requirements |
| **7 Telos Levels** | T0 (root mission) → T6 (transcendent) hierarchy |
| **10 Catalyst Types** | External/internal event classification for graph digestion |

## Features

### Graph Engine
- SQLite WAL backend with connection pooling
- Full-text search (FTS5) with hybrid ranking
- Cosine similarity search
- Pathfinding (BFS/DFS)
- Temporal queries (valid_from/valid_to)
- Soft delete with archival
- Event-sourced temporal reconstruction (JSONL journal)

### Drive Propagation
- 3-phase pipeline: emission → stabilization → aggregation
- Quadrant modulators (UL/UR/LL/LR)
- Diagnosis: addiction, allergy, blind spot, tension
- Shannon entropy computation

### Mind Pipeline
- Consolidation engine (daily deep synthesis)
- Reflect engine (cluster → skill/discovery creation)
- Terrain context (skill discovery from graph density)
- Diagnostic engine (behavioral pattern analysis)
- Feeling engine (drive state → experiential statements)
- Pulse engine (structural gap detection)
- ONNX embeddings (EmbeddingGemma-300M, 768-dim, Q4/Q8 configurable)
- Non-destructive embedding migration (mixed dimensions supported)

### MCP Server
- 26 tools via rmcp SDK with auto schema generation
- Transports: stdio (default) + HTTP/SSE
- Lean mode (skip expensive operations)
- Trust store with SQLite persistence
- Health monitor with circuit breakers

### Safety
- Circuit breaker (Closed/Open/HalfOpen states)
- PreWriteSnapshot for transaction rollback
- Node/edge size limits (100K nodes, 500K edges)
- Type-safe error handling (TdgResult<T>)

## Installation

### From Release (Recommended)

Download the pre-built musl binary for Linux:

```bash
# Download latest release
curl -L https://github.com/ishan-parihar/tdg-rust/releases/latest/download/tdg-rust-x86_64-unknown-linux-musl -o tdg-rust
chmod +x tdg-rust
sudo mv tdg-rust /usr/local/bin/
```

### From Source

```bash
# Prerequisites: Rust 1.70+, cargo-zigbuild, zig
git clone https://github.com/ishan-parihar/tdg-rust.git
cd tdg-rust

# Standard build (no ONNX)
cargo build --release

# Build with ONNX embedding support
cargo build --release --features onnx
```

### VPS Deployment (glibc 2.36)

```bash
# Build for VPS (Debian 12, glibc 2.36)
export ORT_LIB_LOCATION=/tmp/onnxruntime-linux-x64-1.20.1/lib
cargo zigbuild --release --features onnx --target x86_64-unknown-linux-gnu.2.36

# Deploy binary and ORT library
scp target/x86_64-unknown-linux-gnu.2.36/release/tdg-rust nerd@racknerd:~/tdg-rust
scp /tmp/onnxruntime-linux-x64-1.20.1/lib/libonnxruntime.so.1.20.1 nerd@racknerd:~/libonnxruntime.so.1.20.1
```

## Usage

### CLI Commands

```bash
# Initialize database
tdg-rust init

# Start MCP server (stdio)
tdg-rust serve

# Start MCP server (HTTP on port 3001)
tdg-rust serve --port 3001

# Run audit
tdg-rust audit

# Show database stats
tdg-rust stats

# Create a node
tdg-rust create -n observation -N "Key insight" -d "Description"

# Backup database
tdg-rust backup -o backup.db
```

### Available Commands

| Command | Description |
|---------|-------------|
| `serve [--port]` | Start MCP server (stdio on 3000, HTTP on other ports) |
| `init` | Initialize database schema |
| `migrate` | Run database migrations |
| `backup -o <path>` | Backup database |
| `stats` | Show database statistics |
| `audit` | Graph integrity audit |
| `check` | Constraint vitality check |
| `unify` | Unify persistence across data sources |
| `reconcile-constraints` | Dedup constraints, repair BLOCKS edges |
| `sync-skills [-d <dir>]` | Sync skills directory to graph |
| `auto-capture -d <desc>` | Auto-capture observation |
| `create -n <type> -N <name>` | Create node from CLI |
| `maintenance-check` | Orphan + stale node detection |
| `repair-orphans` | Link or archive orphan nodes |
| `embed [--rebuild]` | Generate/update embeddings for all nodes |

### MCP Tools (26)

| Category | Tools |
|----------|-------|
| **Search** | `tdg_search` (hybrid FTS5 search) |
| **CRUD** | `tdg_create`, `tdg_update`, `tdg_get_node`, `tdg_bulk_create`, `tdg_observe`, `tdg_record_exec` |
| **Edges** | `tdg_connect`, `tdg_get_related` |
| **Events** | `tdg_query_events` |
| **Rating** | `tdg_rate_memory` |
| **Mind** | `tdg_mind_state` (stats/health/verify/detail) |
| **Synthesis** | `tdg_reflect` (LLM-powered) |
| **Trust** | `tdg_get_trust`, `tdg_adjust_trust` |
| **Health** | `tdg_health_check`, `tdg_system_health`, `tdg_graph_stats` |
| **Schema** | `tdg_get_schema` |
| **Multi-agent** | `tdg_bank` |
| **Entities** | `tdg_entity` |
| **Maintenance** | `tdg_maintenance` |
| **Persistence** | `tdg_save_mind_state`, `tdg_load_mind_state`, `tdg_get_project_context`, `tdg_set_project_context` |

## Configuration

TDG uses hierarchical configuration loading:

1. Defaults → `tdg.yaml` → `tdg.json` → `TDG_*` environment variables

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `TDG_HOME` | `~/.hermes` | Base home directory |
| `TDG_DB_PATH` | `{home}/tdg/graph.db` | SQLite database path |
| `TDG_STATE_DIR` | `{home}/state` | State files directory |
| `TDG_SKILLS_DIR` | `~/.hermes/skills` | Skills directory |
| `TDG_LEAN` | `false` | Reduced memory mode |

### Embedding Configuration

Configure the embedding system in `tdg.yaml`:

```yaml
embedding:
  model: gemma  # or minilm
  quantization: q4  # q4 or q8
  dimension: 768  # 768 for gemma, 384 for minilm
```

### Diagnostic Thresholds

Edit `config/diagnostic_thresholds.yaml` to tune behavioral analysis:

```yaml
addiction_positive_min: 7.0
allergy_negative_min: 5.0
blind_spot_pct: 10.0
drive_persistence_soft: 3
drive_persistence_strong: 5
drive_persistence_mandatory: 8
quadrant_imbalance_pct: 40.0
quadrant_persistence_cycles: 4
```

## Architecture

```
src/
├── main.rs              CLI entry point (12 subcommands)
├── lib.rs               Library root, 28 modules
├── models.rs            Core types: Node, Edge, Event, Embedding
├── db/                  SQLite persistence (pool, CRUD, schema, events, write_guard)
├── mcp/                 MCP server (stdio + HTTP), 26 tools
├── flow.rs              Dual-pole drive propagation engine
├── knowledge.rs         Catalyst lifecycle + graph hygiene
├── graph_projection.rs  SQLite → petgraph in-memory projection
├── telearchy.rs         Stage-gated telos hierarchy
├── audit.rs             5-report audit engine + Markdown export
├── circuit_breaker.rs   State machine + pre-write snapshots
├── grammar/             Node blueprints + auto-wiring
├── validation.rs        Edge validation contracts
├── plugins/             Entity extractor, hybrid retriever, preference extractor
├── llm/                 LLM trait + OpenAI/Anthropic/Ollama providers
├── mind/                Consolidation, reflection, terrain, injection, diagnostics
└── scripts/             CLI script implementations
```

## Dependencies

| Category | Crates |
|----------|--------|
| **Core** | tokio, serde/serde_json, anyhow, thiserror, tracing |
| **Database** | rusqlite (bundled, WAL mode) |
| **HTTP** | axum, tokio-stream |
| **Graph** | petgraph |
| **MCP** | rmcp (server, schemars, transport-io) |
| **LLM** | reqwest (json) |
| **CLI** | clap (derive) |
| **Config** | figment (yaml/json/env) |
| **Optional** | ort (ONNX Runtime), tokenizers (feature: onnx) |

## Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_circuit_breaker

# Run benchmarks
cargo bench
```

### Test Coverage

- 511+ tests (unit + integration)
- Property-based tests (proptest) for graph operations
- Snapshot tests (insta) for output verification
- Criterion benchmarks for performance regression

## Deployment

### Hermes Agent (Recommended)

```bash
# One-command install
curl -fsSL https://raw.githubusercontent.com/ishan-parihar/tdg-rust/main/install.sh | bash

# Or uninstall
TDG_UNINSTALL=1 bash install.sh
```

### Pre-built Binary

Download from [GitHub Releases](https://github.com/ishan-parihar/tdg-rust/releases):

```bash
# Linux x86_64
curl -LO https://github.com/ishan-parihar/tdg-rust/releases/latest/download/tdg-rust
chmod +x tdg-rust
./tdg-rust --version
```

### Docker

```bash
docker-compose up -d
```

### Build from Source (Development Only)

```bash
# Standard build
cargo build --release

# With ONNX embeddings
cargo build --release --features onnx

# For VPS deployment (glibc 2.36)
export ORT_LIB_LOCATION=/tmp/onnxruntime-linux-x64-1.20.1/lib
cargo zigbuild --release --features onnx --target x86_64-unknown-linux-gnu.2.36
```

### Render.com

```yaml
# render.yaml
services:
  - type: web
    name: tdg-mcp
    env: docker
    dockerfilePath: Dockerfile
    ports:
      - port: 3001
```

## Performance

| Metric | Value |
|--------|-------|
| Startup time | ~10ms |
| Turn overhead | <1ms |
| Binary size | ~12MB (ONNX-enabled) |
| Memory (lean mode) | <50MB RSS |
| Concurrent writes | Serialized via WriteGuard |
| Embedding speed | ~8-10 nodes/min (VPS) |

## Comparison with Python

| Metric | Python | Rust |
|--------|--------|------|
| Total lines | 29,000+ | 28,000+ |
| Tests | 576 | 511+ |
| MCP tools | 16 | 26 |
| Startup time | ~200ms | ~10ms |
| Memory usage | ~200MB | ~50MB |
| Binary size | N/A | ~12MB |
| Embeddings | MiniLM | EmbeddingGemma |

## License

MIT
