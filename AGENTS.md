# AGENTS.md — TDG-Rust Project Guidelines

## Project Overview

TDG-Rust is a Teleological Developmental Graph implementation in Rust. It provides:
- Graph-based knowledge storage with nodes, edges, and embeddings
- MCP server for tool-based interaction
- CLI for graph operations (import, search, serve, embed)

## Build Requirements

### Local Development (Arch Linux)
```bash
# Standard build (no ONNX)
cargo build --release

# With ONNX embedding support
cargo build --release --features onnx
```

### VPS Deployment (Debian 12, glibc 2.36)

**CRITICAL**: Never build on the VPS. Always build locally and deploy.

The VPS runs Debian 12 with glibc 2.36. Local machines typically have newer glibc (e.g., 2.43 on Arch). Building locally produces binaries that won't run on the VPS due to glibc version mismatch.

#### Required Tools
- `cargo-zigbuild`: Install via `cargo install cargo-zigbuild`
- `zig`: Install via `pacman -S zig` (Arch) or equivalent

#### Build Command
```bash
# Clean any previous Docker/root-owned artifacts
cargo clean

# Build for VPS (glibc 2.36)
# ORT_LIB_LOCATION must point to the lib/ subdirectory of ORT 1.20.1
export ORT_LIB_LOCATION=/tmp/onnxruntime-linux-x64-1.20.1/lib
cargo zigbuild --release --features onnx --target x86_64-unknown-linux-gnu.2.36
```

#### VPS Deployment
```bash
# Binary
scp target/x86_64-unknown-linux-gnu.2.36/release/tdg-rust nerd@racknerd:~/tdg-rust

# ORT shared library (if not already deployed)
scp /tmp/onnxruntime-linux-x64-1.20.1/lib/libonnxruntime.so.1.20.1 nerd@racknerd:~/libonnxruntime.so.1.20.1
ssh nerd@racknerd "ln -sf libonnxruntime.so.1.20.1 ~/libonnxruntime.so.1; ln -sf libonnxruntime.so.1 ~/libonnxruntime.so"

# On VPS: Update MCP server binary
ssh nerd@racknerd "cp ~/tdg-rust ~/.hermes/tdg-rust/tdg-rust; chmod +x ~/.hermes/tdg-rust/tdg-rust"

# On VPS: Restart gateway
ssh nerd@racknerd "cd ~/.hermes/hermes-agent && python3 -m hermes_cli.main gateway restart"
```

#### LD_LIBRARY_PATH Configuration
The VPS must have `LD_LIBRARY_PATH` set to include the ORT library:
```bash
# In ~/.bashrc
export LD_LIBRARY_PATH=~/.hermes/tdg/lib:$LD_LIBRARY_PATH

# In ~/.hermes/config.yaml (MCP server env)
mcp_servers:
  tdg:
    env:
      LD_LIBRARY_PATH: /home/nerd/.hermes/tdg/lib
```

## Architecture

### Key Components
- `src/config/` — Configuration types and loading (figment)
- `src/db/` — SQLite database layer (schema, pool, queries)
- `src/mind/` — Embedding pipeline (ONNX/GGUF backends)
- `src/mcp/` — MCP server implementation
- `src/util/` — Math utilities (cosine similarity)

### Embedding System
- **ONNX Backend**: `EmbeddingGemma-300M` (768-dim, Q4/Q8 quantization)
- **Fallback**: `all-MiniLM-L6-v2` (384-dim)
- **Storage**: SQLite `embeddings` table with `dimension` column for mixed-size vectors
- **Migration**: Non-destructive via `tdg embed --rebuild`

### Configuration
- Config file: `tdg.yaml` (loaded from CWD)
- Key settings:
  ```yaml
  embedding:
    model: gemma  # or minilm
    quantization: q4  # q4 or q8
    dimension: 768  # 768 for gemma, 384 for minilm
  ```

## Development Rules

1. **Never build on VPS** — Always build locally and deploy
2. **Test ONNX features** — Ensure `cargo build --features onnx` succeeds before committing
3. **Config changes** — Update `tdg.yaml` and document in this file
4. **Schema changes** — Add migrations in `src/db/schema.rs`
5. **Binary compatibility** — Verify glibc requirements match VPS (2.36)

## Version History

- **v0.4.1**: MCP server integration verified, README updated
- **v0.4.0**: EmbeddingGemma ONNX backend, configurable Q4/Q8, non-destructive migration
- **v0.3.0**: Initial release
