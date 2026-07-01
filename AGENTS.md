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
cargo build --features onnx

# Run tests (ONNX feature set)
cargo test --features onnx
```

### VPS Deployment (Debian 12, glibc 2.36)

**CRITICAL**: Never build on the VPS. Always build locally and deploy.

The VPS runs Debian 12 with glibc 2.36. Local machines typically have newer glibc (e.g., 2.43 on Arch). A normal local `cargo build --release` produces binaries linked against the host glibc and will not run on the VPS.

#### Required Tools
- `cargo-zigbuild`: `cargo install cargo-zigbuild`
- `zig`: `pacman -S zig` (Arch) or equivalent
- ONNX Runtime **1.20.1** prebuilt libs (for linking only; runtime `.so` lives on the VPS)

#### Download ONNX Runtime (build-time link libs)
```bash
mkdir -p /tmp/onnxruntime-linux-x64-1.20.1
curl -fsSL "https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-1.20.1.tgz" \
  | tar -xz -C /tmp/onnxruntime-linux-x64-1.20.1 --strip-components=1
```

#### Build Command (VPS-compatible binary)
```bash
export ORT_LIB_LOCATION=/tmp/onnxruntime-linux-x64-1.20.1/lib
export ORT_PREFER_DYNAMIC_LINK=1   # required: prebuilt ORT is .so-only, not static

cargo zigbuild --release --features onnx --target x86_64-unknown-linux-gnu
```

Output binary:
```
target/x86_64-unknown-linux-gnu/release/tdg-rust
```

Verify before deploy:
```bash
BINARY=target/x86_64-unknown-linux-gnu/release/tdg-rust
file "$BINARY"
strings "$BINARY" | rg "nodes_fts\(rowid, id"   # must use `id`, not `node_id`
strings "$BINARY" | rg "nodes_fts\) VALUES\('rebuild'\)"  # FTS rebuild command present
objdump -T "$BINARY" | rg GLIBC | sed 's/.*GLIBC_/GLIBC_/' | sort -Vu | tail -3
```

**Note:** `ORT_PREFER_DYNAMIC_LINK=1` is mandatory when pointing `ORT_LIB_LOCATION` at the official prebuilt tarball. Without it, `ort-sys` attempts static linking and the build fails with “could not link to the ONNX Runtime build”.

#### VPS Deployment
```bash
# From project root (adjust SSH helper path if needed)
BINARY=target/x86_64-unknown-linux-gnu/release/tdg-rust
SSH=/home/ishanp/ssh-racknerd.sh

# Copy binary
sshpass -p '...' scp -o PreferredAuthentications=password -o PubkeyAuthentication=no \
  "$BINARY" nerd@racknerd:~/tdg-rust-new
$SSH "cp ~/tdg-rust-new ~/.hermes/tdg-rust/tdg-rust && chmod +x ~/.hermes/tdg-rust/tdg-rust"

# Run migrations on the live database (repairs FTS triggers/index after upgrade)
$SSH "export LD_LIBRARY_PATH=~/.hermes/tdg/lib:\$LD_LIBRARY_PATH TDG_HOME=/home/nerd/.hermes && \
  ~/.hermes/tdg-rust/tdg-rust migrate"

# Restart Hermes gateway
$SSH "cd ~/.hermes/hermes-agent && python3 -m hermes_cli.main gateway restart"
```

ORT shared library on VPS (if not already under `~/.hermes/tdg/lib/`):
```bash
scp /tmp/onnxruntime-linux-x64-1.20.1/lib/libonnxruntime.so.1.20.1 nerd@racknerd:~/libonnxruntime.so.1.20.1
ssh nerd@racknerd "mkdir -p ~/.hermes/tdg/lib && cp ~/libonnxruntime.so.1.20.1 ~/.hermes/tdg/lib/ && \
  ln -sf libonnxruntime.so.1.20.1 ~/.hermes/tdg/lib/libonnxruntime.so.1"
```

#### LD_LIBRARY_PATH Configuration
The VPS must expose ORT to the MCP server process:
```bash
# In ~/.bashrc (interactive testing)
export LD_LIBRARY_PATH=~/.hermes/tdg/lib:$LD_LIBRARY_PATH

# In ~/.hermes/config.yaml (MCP server env)
mcp_servers:
  tdg:
    command: /home/nerd/.hermes/tdg-rust/tdg-rust
    args:
      - serve
    env:
      TDG_HOME: /home/nerd/.hermes
      LD_LIBRARY_PATH: /home/nerd/.hermes/tdg/lib
```

#### GitHub Releases
After a successful VPS build, publish the binary:
```bash
gh release create vX.Y.Z \
  target/x86_64-unknown-linux-gnu/release/tdg-rust#tdg-rust \
  --repo ishan-parihar/tdg-rust \
  --title "vX.Y.Z: short title" \
  --notes "$(cat <<'EOF'
Release notes here.
EOF
)"
```

Asset name must remain `tdg-rust` (matches `install.sh` and README download URLs).

## Architecture

### Key Components
- `src/config/` — Configuration types and loading (figment)
- `src/db/` — SQLite database layer (schema, pool, queries)
- `src/mind/` — Embedding pipeline (ONNX/GGUF backends)
- `src/mcp/` — MCP server implementation
- `src/maintenance/` — Janitor, monitor, enricher (background graph hygiene)
- `src/util/` — Math utilities (cosine similarity)

### FTS5 Full-Text Search (`nodes_fts`)

The FTS virtual table uses **external content** mode (`content='nodes'`). Important invariants:

| Item | Correct | Wrong (legacy) |
|------|---------|----------------|
| FTS column for node PK | `id UNINDEXED` | `node_id` |
| Trigger inserts | `INSERT INTO nodes_fts(rowid, id, name, description)` | `... node_id ...` |
| Janitor backfill | `f.id IS NULL`, `nodes_fts(rowid, id, ...)` | `f.node_id` |

**Migration Phase 7** (`run_migrations` in `src/db/schema.rs`):
1. `DROP TRIGGER` for `nodes_fts_ai`, `nodes_fts_ad`, `nodes_fts_au`
2. `DROP TABLE nodes_fts`
3. Recreate via `init_fts()`
4. Call `rebuild_fts()`

Dropping triggers first is required: `CREATE TRIGGER IF NOT EXISTS` does **not** replace triggers that still reference the old `node_id` column.

**Rebuild FTS** — always use the FTS5 rebuild command, not `DELETE FROM nodes_fts`:
```sql
INSERT INTO nodes_fts(nodes_fts) VALUES('rebuild');
```
`DELETE FROM nodes_fts` on external-content tables can return SQLite **error 267** (“Content in the virtual table is corrupt”) when shadow tables are inconsistent, even when `PRAGMA integrity_check` reports `ok`.

After deploying a fix release, run `tdg-rust migrate` once against the production database.

### Embedding System
- **ONNX Backend**: `EmbeddingGemma-300M` (768-dim, Q4/Q8 quantization)
- **Fallback**: `all-MiniLM-L6-v2` (384-dim)
- **Storage**: SQLite `embeddings` table with `dimension` column for mixed-size vectors
- **Migration**: Non-destructive via `tdg embed --rebuild`

### Configuration
- Config file: `tdg.yaml` (loaded from CWD or `TDG_HOME`)
- Default DB path: `$TDG_HOME/tdg/graph.db` (VPS: `/home/nerd/.hermes/tdg/graph.db`)
- Key settings:
  ```yaml
  embedding:
    model: gemma  # or minilm
    quantization: q4  # q4 or q8
    dimension: 768  # 768 for gemma, 384 for minilm
  ```

## Development Rules

1. **Never build on VPS** — Always cross-build locally with `cargo zigbuild` and deploy
2. **Test ONNX features** — `cargo test --features onnx` before release commits
3. **FTS changes** — Update schema, janitor, triggers, and `rebuild_fts()` together; add a regression test in `schema.rs`
4. **Schema changes** — Add migrations in `src/db/schema.rs`; prefer idempotent `ALTER TABLE ... ADD COLUMN` with error swallow for duplicates
5. **Binary compatibility** — Confirm max GLIBC symbol ≤ 2.36 for VPS (Debian 12)
6. **Config changes** — Update `tdg.yaml` and document here
7. **Post-release** — Publish GitHub release asset, deploy to VPS, run `migrate`, restart gateway

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| `database disk image is malformed` / error 267 on startup | Corrupt FTS5 shadow tables or stale `node_id` triggers | Deploy fixed binary; run `tdg-rust migrate` |
| `ort-sys could not link` at build time | Missing `ORT_PREFER_DYNAMIC_LINK=1` or wrong `ORT_LIB_LOCATION` | Set both env vars; use ORT 1.20.1 tarball |
| `libonnxruntime.so.1: cannot open shared object` on VPS | `LD_LIBRARY_PATH` not set for MCP process | Add to `config.yaml` `mcp_servers.tdg.env` |
| Janitor FTS backfill fails | SQL still references `node_id` in `nodes_fts` | Use `id` column (see FTS5 section) |
| Binary runs locally but not on VPS | Built against host glibc | Rebuild with `cargo zigbuild --target x86_64-unknown-linux-gnu` |

## Version History

- **v0.4.3**: FTS trigger migration fix, janitor `id` column, `rebuild_fts()` uses FTS5 `rebuild` command (fixes error 267)
- **v0.4.1**: MCP server integration verified, README updated
- **v0.4.0**: EmbeddingGemma ONNX backend, configurable Q4/Q8, non-destructive migration
- **v0.3.0**: Initial release
