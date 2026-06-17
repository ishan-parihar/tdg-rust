# tdg-rust

Rust implementation of the **Teleological Developmental Graph (TDG)** — a memory infrastructure for AI agents.

## Overview

This project is a Rust transmutation of [tdg](https://github.com/ishanp/tdg) (Python), optimized for:

- **Low resource usage** — minimal RSS, fast startup
- **High throughput** — concurrent request handling
- **Production deployment** — MCP server on Render.com, HTTP/SSE transport
- **SaaS-ready** — per-user isolation, multi-tenancy support

## Architecture

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Core CRUD | Rust + SQLite (rusqlite) | Node/edge creation, retrieval, updates |
| FTS5 Search | SQLite FTS5 | Full-text search across nodes |
| Vector Search | rusqlite + cosine similarity | Semantic similarity search |
| MCP Transport | HTTP/SSE (axum or warp) | AI agent integration |
| Event Store | SQLite WAL | Event-sourced temporal reconstruction |
| HRR Vectors | Rust ndarray | 1024-dim phase vector algebra |

## What Stays in Python

Some advanced features will remain in the Python TDG for now:

- HRR compositional algebra (1024-dim phase vectors)
- Holonic self-model
- 16-cell drive matrix
- Complex mind injection pipeline
- LLM-powered reflection synthesis

## Deployment Target

- **Render.com** — MCP server (HTTP/SSE transport)
- **Cloudflare Workers** — Agent via Flue framework
- **OpenRouter Free Models** — LLM backend

## Status

🚧 Planning phase — Rust port in development.

## License

MIT
