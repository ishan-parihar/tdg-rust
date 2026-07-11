# tdg-rust

> **Teleological Developmental Graph** — a self-structuring neural memory infrastructure for AI agents. Not a database. A brain.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org)
[![Tests](https://img.shields.io/badge/tests-509%20passing-brightgreen.svg)](#testing)
[![MCP](https://img.shields.io/badge/MCP-50%20tools-blue.svg)](#mcp-tools)
[![ONNX](https://img.shields.io/badge/embeddings-EmbeddingGemma%20768d-purple.svg)](#embeddings)

TDG-rust is a memory infrastructure that gives AI agents **persistent, structured, self-organizing knowledge**. It implements the [HoloOS](https://github.com/ishanparihar/HoloOS) holonic-science ontology: every memory (holon) runs a metabolic cycle, computes its own health, forms connections through resonance, and adapts through experience — like neurons in a biological brain.

## Why TDG?

Standard vector databases store embeddings and retrieve by similarity. TDG does more:

| Feature | Vector DB | TDG |
|---------|-----------|-----|
| **Storage** | Embeddings | Graph + embeddings + metabolic state + attractor fields |
| **Retrieval** | Cosine similarity | Hybrid FTS5 + embedding + graph + resonance |
| **Health** | None | G_z (integrative efficiency) + P_z (transcendental tension) |
| **Self-organization** | None | Synaptogenesis (grows new edges from resonance) |
| **Learning** | None | Hebbian LTP/LTD (edges strengthen with co-activation) |
| **Consolidation** | None | Sleep replay + value-based forgetting |
| **Epistemics** | None | Status ladder (ai-draft → canonical) + 5-Gate validation |
| **Drive adaptation** | None | Drives learn from experience (not hardcoded) |

## Installation & Setup Guide

TDG-rust can be installed automatically via our script, or built manually from source.

### Method 1: Automatic Installation (Recommended for Hermes Agent integration)

This script installs the pre-compiled binary, sets up the ONNX Runtime library, configures the database, and patches `config.yaml` for Hermes Agent:

```bash
curl -fsSL https://raw.githubusercontent.com/ishan-parihar/tdg-rust/main/install.sh | bash
```

The installer configures a directory structure at `~/.hermes/tdg-rust` and creates a **wrapper script** at `~/.hermes/tdg-rust/tdg` which automatically injects the correct `LD_LIBRARY_PATH` so you don't encounter ONNX shared library runtime errors.

### Method 2: Manual Setup & Building from Source

For local development or custom systems, build TDG-rust from source:

#### 1. Clone the repository
```bash
git clone https://github.com/ishan-parihar/tdg-rust.git
cd tdg-rust
```

#### 2. Compile with ONNX support
To enable the inline embedding engine, compile with the `onnx` feature:
```bash
cargo build --release --features onnx
```
> [!NOTE]
> The build script automatically downloads the target-specific ONNX Runtime libraries and stores them in the build target directory. 

#### 3. Resolve ONNX Runtime library at Runtime
Since `ort` links dynamically, you must instruct your OS loader where to find `libonnxruntime.so.1` (Linux) or `libonnxruntime.dylib` (macOS).

**Linux (Bash):**
```bash
# Point to the compiled library directory in your target output
export LD_LIBRARY_PATH="$(pwd)/target/release/build/$(ls target/release/build | grep -E '^ort-[0-9a-f]+$')/out:$LD_LIBRARY_PATH"
```

**macOS:**
```bash
export DYLD_LIBRARY_PATH="$(pwd)/target/release/build/$(ls target/release/build | grep -E '^ort-[0-9a-f]+$')/out:$DYLD_LIBRARY_PATH"
```

Alternatively, you can copy the libraries into a system folder (like `/usr/local/lib`) or create a launcher script:
```bash
mkdir -p ~/.local/bin
cat << 'EOF' > ~/.local/bin/tdg
#!/usr/bin/env bash
export LD_LIBRARY_PATH="/path/to/tdg-rust/target/release/build/ort-xyz/out:$LD_LIBRARY_PATH"
exec "/path/to/tdg-rust/target/release/tdg-rust" "$@"
EOF
chmod +x ~/.local/bin/tdg
```

#### 4. Initialize Database
Initialize the schema and Full-Text Search (FTS5) indexes:
```bash
./target/release/tdg-rust init
```

#### 5. Verify & Run
```bash
# Check system stats
./target/release/tdg-rust stats

# Start the MCP server (for Cursor, Claude Desktop, or Hermes Gateway)
./target/release/tdg-rust serve

# Or start the HTTP server on a custom port
./target/release/tdg-rust serve --port 3001
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    MCP Transport                         │
│              (stdio / HTTP-SSE via rmcp)                 │
├─────────────────────────────────────────────────────────┤
│                    Agent API (50 tools)                  │
│  ContextPack · 5-Gate Validation · Synthesis Submission  │
├─────────────────────────────────────────────────────────┤
│                  Mind Pipeline                           │
│  Graph Mind · Metabolic Summary · Reflect · Consolidation│
├─────────────────────────────────────────────────────────┤
│                Neural Plasticity Engine                  │
│  Hebbian LTP/LTD · Synaptogenesis · Replay · Forgetting   │
├─────────────────────────────────────────────────────────┤
│              Metabolic Engine (Tier 2)                   │
│  Lesser Cycle (M·P·C·E) · Greater Cycle (S·T·G·Ch)     │
│  Attractor Field A(H) · Health (G_z, P_z) · Resonance    │
├─────────────────────────────────────────────────────────┤
│                Persistence (SQLite WAL)                  │
│  Nodes · Edges · Embeddings · Events · Metabolism Queue  │
└─────────────────────────────────────────────────────────┘
```

### Brain-like Capabilities

| Brain function | TDG implementation | Phase |
|---|---|---|
| **Hebbian learning** (LTP) | Edge `co_activation_count` increments on co-firing; flow rate = `base + 0.1·ln(1+count)` | 16 |
| **Synaptic decay** (LTD) | Tier 3 schedule decays `co_activation_count` by 50% for inactive edges; prunes `weight < 0.3` | 16 |
| **Synaptogenesis** | Tier 3 schedule creates `RESONATES_WITH` edges for R > 0.7 pairs | 17 |
| **Sleep replay** | Tier 3 schedule re-activates recent memories (catalyst injection) | 18 |
| **Forgetting** | Archives nodes with `retrieval_count=0`, `confidence < 0.3`, `age > 30d` | 18 |
| **Drive adaptation** | `drives_json` adapts after each cycle — positive_pole strengthens with use | 19 |
| **Graph-level mind** | Diagnoses graph patterns (GoldenAllergy, depolarization, collapse) → injects catalyst | 12 |

### Holonic-Science Primitives

| Primitive | Source | Implementation |
|---|---|---|
| Lesser cycle (M·P·C·E) | HoloOS Doc 02.1 (canonical) | `src/metabolism/lesser_cycle.rs` |
| Greater cycle (S·T·G·Ch) | HoloOS Doc 02.2 (canonical) | `src/metabolism/greater_cycle.rs` |
| Attractor field A(H) = ⟨A_M, A_P, A_G, Γ⟩ | HoloOS Doc 08.1 | `src/metabolism/attractor.rs` |
| G_z / P_z health metrics | HoloOS Doc 02.1 §6.2 | `src/metabolism/health.rs` |
| Resonance R(H1, H2) | HoloOS Doc 08.1 §8 | `src/metabolism/health.rs` |
| 22 named archetypes | HoloOS Doc 03.2 | `src/holonic_types/archetypes.rs` |
| 5-Gate Validation | HoloOS Epistemology docs 0-4 | `src/context/validation.rs` |
| ContextPack | HoloOS AGENTS.md | `src/context/context_pack.rs` |
| V/C/R/N coordinate system | HoloOS Doc 08.8.x | `src/models.rs` + `src/holon.rs` |

## MCP Tools (50)

<details>
<summary><b>Click to expand full tool list</b></summary>

| Category | Tools |
|---|---|
| **Search** | `tdg_search`, `tdg_prefetch` |
| **CRUD** | `tdg_create`, `tdg_update`, `tdg_get_node`, `tdg_bulk_create`, `tdg_observe`, `tdg_record_exec` |
| **Edges** | `tdg_connect`, `tdg_get_related` |
| **Events** | `tdg_query_events` |
| **Rating** | `tdg_rate_memory` |
| **Mind** | `tdg_mind_state`, `tdg_context`, `tdg_consolidate` |
| **Synthesis** | `tdg_reflect`, `tdg_reflect_run` |
| **Trust** | `tdg_get_trust`, `tdg_adjust_trust`, `tdg_health_check` |
| **Health** | `tdg_system_health`, `tdg_graph_health`, `tdg_graph_stats` |
| **Schema** | `tdg_get_schema` |
| **Maintenance** | `tdg_maintenance`, `tdg_enrich`, `tdg_self_manage`, `tdg_renormalize` |
| **Audit** | `tdg_audit` |
| **Persistence** | `tdg_save_mind_state`, `tdg_load_mind_state`, `tdg_get_project_context`, `tdg_set_project_context` |
| **Import/Export** | `tdg_export`, `tdg_import` |
| **Status ladder** | `tdg_elevate` (human-only) |
| **Metabolism** | `tdg_tick`, `tdg_metabolism_status` |
| **Attractor** | `tdg_attractor`, `tdg_health`, `tdg_resonance`, `tdg_resonance_partners` |
| **Greater cycle** | `tdg_greater_cycle` |
| **ContextPack** | `tdg_fetch_context` |
| **Validation** | `tdg_submit_synthesis`, `tdg_validate_synthesis` |
| **Type system** | `tdg_archetypes`, `tdg_validate_type` |

</details>

## Configuration

```bash
# Environment variables
TDG_HOME=~/.hermes              # Base directory
TDG_DB_PATH=$TDG_HOME/tdg/graph.db  # SQLite database path
TDG_LEAN=false                  # Reduced memory mode
TDG_AGENT_NAME=tdg-agent        # Agent name for provenance

# Metabolism
TDG_METABOLISM_WORKERS=1        # Worker count (default 1 for 2GB VPS)
TDG_GREATER_CYCLE_INTERVAL_SECS=600   # Greater cycle sweep (10 min)
TDG_MIND_INTEGRATION_INTERVAL_SECS=900  # Graph mind (15 min)
TDG_SYNAPTIC_DECAY_INTERVAL_SECS=3600  # LTD decay (1 hour)
TDG_SYNAPTOGENESIS_INTERVAL_SECS=1800  # Edge growth (30 min)
TDG_MEMORY_REPLAY_INTERVAL_SECS=21600  # Sleep replay (6 hours)
TDG_RESONANCE_REBUILD_INTERVAL_SECS=14400  # Full rebuild (4 hours)
```

```yaml
# tdg.yaml
embedding:
  model: gemma          # or minilm
  quantization: q4      # q4 or q8
  dimension: 768        # 768 for gemma, 384 for minilm
```

## Embeddings

| Model | Dimensions | Quantization | Features |
|-------|-----------|-------------|----------|
| EmbeddingGemma-300M | 768 | Q4 / Q8 | `--features onnx` |
| all-MiniLM-L6-v2 | 384 | quantized | Fallback |

Embeddings are generated inline on node creation (when ONNX is enabled) and backfilled by the enricher/janitor. The embedding text includes the node name, description, and top-3 edge relationships for contextual representation.

## Testing

```bash
# Run all tests
cargo test

# Run specific test suites
cargo test --lib                    # 430 unit tests
cargo test --test integration       # 8 integration tests
cargo test --test mcp_e2e           # 66 MCP end-to-end tests
cargo test --test e2e_mind_simulation  # 5 full mind-flow simulations

# With ONNX features
cargo test --features onnx

# Benchmarks
cargo bench
```

**509 tests total. Zero warnings. Zero regressions.**

## Performance

| Metric | Value |
|--------|-------|
| Startup time | ~10ms |
| Turn overhead | <1ms |
| Binary size | ~12MB (ONNX-enabled) |
| Memory (lean mode) | <50MB RSS |
| Memory (full mode) | <100MB RSS |
| Embedding speed | ~8-10 nodes/min (VPS) |
| ContextPack (cached) | <5ms |
| ContextPack (cold) | <100ms |

## Deployment

### VPS (Debian 12, glibc 2.36)

```bash
# Build (never build on VPS — cross-compile)
export ORT_LIB_LOCATION=/tmp/onnxruntime-linux-x64-1.20.1/lib
export ORT_PREFER_DYNAMIC_LINK=1
cargo zigbuild --release --features onnx --target x86_64-unknown-linux-gnu.2.36

# Deploy
scp target/x86_64-unknown-linux-gnu/release/tdg-rust nerd@vps:~/.hermes/tdg-rust/
ssh nerd@vps "TDG_HOME=~/.hermes ~/.hermes/tdg-rust/tdg-rust migrate"
```

### Docker

```bash
docker-compose up -d
```

### Hermes Agent Integration

The `plugins/tdg/` directory contains a Python adapter for the [Hermes Agent](https://github.com/nousresearch/hermes-agent) framework:

```bash
# Install adapter
mkdir -p ~/.hermes/plugins/tdg
cp plugins/tdg/__init__.py ~/.hermes/plugins/tdg/
cp plugins/tdg/plugin.yaml ~/.hermes/plugins/tdg/

# The adapter exposes 3 LLM-facing tools:
# - tdg_memory_search: hybrid FTS5 + embedding + graph search
# - tdg_memory_record: create observation nodes
# - tdg_memory_status: graph stats + metabolism queue depth
```

## Project Structure

```
src/
├── main.rs                    CLI + background schedulers (7 Tier 3 schedules)
├── models.rs                  Core types: Node, Edge, SynthesisStatus
├── holon.rs                   Holon newtype (compositional algebra)
├── scale_codes.rs             17 organisational scale codes (S00-S80)
├── metabolism/                The metabolic engine (Phases 2-4, 16-19)
│   ├── lesser_cycle.rs        M·P·C·E state machine (trusted anchor)
│   ├── greater_cycle.rs       S·T·G·Ch state machine (vertical ascent)
│   ├── attractor.rs           A(H) = ⟨A_M, A_P, A_G, Γ⟩
│   ├── health.rs              G_z, P_z, Resonance R(H1, H2)
│   └── worker.rs              Tier 2 async job pool + Hebbian tracking
├── context/                   Agent API (Phase 5)
│   ├── context_pack.rs        ContextPack (single-call context aggregation)
│   └── validation.rs          5-Gate Validation (epistemic enforcement)
├── holonic_types/             Type system (Phase 6)
│   ├── archetypes.rs          22 named archetypes
│   └── type_validation.rs     T1/T2/T3 type validation
├── mind/                      Mind pipeline
│   ├── graph_mind.rs          Graph-level mind integration (closed loop)
│   ├── injector.rs            Prompt assembly + metabolic summary
│   ├── reflect_engine.rs      Skill discovery (entity + resonance clustering)
│   ├── feeling.rs             Drive state → experiential statements
│   └── ...
├── mcp/                       MCP server (50 tools)
├── db/                        SQLite persistence (WAL, FTS5, pool)
├── flow.rs                    Drive propagation + Hebbian learned rates
└── ...
```

## Background Schedules

TDG runs 7 background schedules that keep the mind alive:

| Schedule | Default | What it does |
|----------|---------|-------------|
| SelfManager | 6h | Janitor + Enricher + Archiver + Telearchy |
| Health check | 5m | Internal DB health probe |
| Greater cycle sweep | 10m | Enqueue GreaterTick for holons with pressure |
| Graph mind integration | 15m | Diagnose graph patterns → inject catalyst |
| Synaptic decay (LTD) | 1h | Decay co_activation_count; prune dead edges |
| Synaptogenesis | 30m | Create RESONATES_WITH edges for R > 0.7 |
| Memory replay | 6h | Re-activate recent memories; forget low-value |
| Resonance rebuild | 4h | Full resonance_graph recomputation |

## License

MIT

---

*TDG-rust: The structure is the graph. The metabolism is the mind. The plasticity is the brain.*
