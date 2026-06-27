# TDG-Rust Upgrade Plan: Porting Python TDG Changes

**Date:** 2026-06-28
**Last tdg-rust commit:** 2026-06-19 01:59:15
**Python TDG changes since then:** 3 commits + 42 files uncommitted (5,240 deletions, 351 insertions)

---

## Executive Summary

The Python TDG has undergone significant cleanup and upgrades since the Rust version was last updated. This plan defines what needs to be ported to tdg-rust.

**Key insight:** Most Python changes are DELETIONS (over-engineering cleanup). The only new code is the `core/maintenance/` module (SelfManager).

---

## Change Analysis

### What Changed in Python TDG (Since 2026-06-19)

| Category | Files | Lines | Description |
|----------|-------|-------|-------------|
| **Deleted modules** | 5 | -1,206 | override_engine, deprecation_registry, circuit_breaker, score_reconciler, test files |
| **Deleted methods** | 1 | -180 | Dead methods in plugins/tdg/__init__.py |
| **Deleted scripts** | 1 | -185 | tdg_repair_orphans.py |
| **Deleted docs** | 6 | -50 | Stale plan documents |
| **YAGNI cleanup** | 3 | -341 | Speculative types, closure rules, unreachable code |
| **Shrink/dedup** | 7 | -153 | Stop-words, graph_write_lock, _edge_exists, etc. |
| **Bug fixes** | 2 | +2 | Missing import, hardcoded dates |
| **MCP consolidation** | 1 | -42 | Merged tdg_maintenance into tdg_self_manage |
| **New: maintenance** | 7 | +915 | SelfManager (monitor, janitor, enricher, archiver, orchestrator) |
| **New: constants** | 1 | +20 | Shared stop-words |
| **Config cleanup** | 2 | -13 | Deleted reflect.json, removed llm section from embeddings.json |

**Net result:** -4,891 lines (16% of codebase)

### What TDG-Rust Already Has (From README)

| Feature | Status | Python Equivalent |
|---------|--------|-------------------|
| Graph engine (SQLite WAL) | ✅ Exists | graph_db.py |
| FTS5 search | ✅ Exists | graph_db.py |
| Cosine similarity | ✅ Exists | hybrid_retriever.py |
| HRR algebra | ✅ Exists | hrr.py |
| Drive propagation | ✅ Exists | tdg_flow_engine.py |
| MCP server | ✅ Exists (26 tools) | tdg_mcp_server.py |
| Circuit breaker | ✅ Exists | core/circuit_breaker.py (DELETED in Python) |
| ONNX embeddings | ✅ Exists (feature-gated) | mind/embedding_engine.py |

---

## Upgrade Plan

### Phase 1: Remove Dead Code (Day 1)

Remove code that was deleted in Python TDG.

#### 1.1 Files to Delete from tdg-rust

| Rust File | Python Equivalent | Reason |
|-----------|-------------------|--------|
| Check if `circuit_breaker` exists | `core/circuit_breaker.py` | Deleted in Python (WAL mode already safe) |
| Check if `score_reconciler` exists | `core/score/tdg_score_reconciler.py` | Deleted in Python (nobody reads provenance) |
| Check if `deprecation_registry` exists | `core/audit/deprecation_registry.py` | Deleted in Python (YAGNI) |

**Action:** Audit tdg-rust for these modules. If they exist, delete them and remove all references.

#### 1.2 Dead Methods to Remove

| Method | Location | Reason |
|--------|----------|--------|
| `record_execution()` | plugins/tdg | Zero callers |
| `queue_prefetch()` | plugins/tdg | Zero callers |
| `on_delegation()` | plugins/tdg | Zero callers |
| `save_config()` | plugins/tdg | Zero callers |
| `get_config_schema()` | plugins/tdg | Returns hardcoded data |

**Action:** Grep tdg-rust for these method names. Remove if found.

#### 1.3 Speculative Types to Remove

| Type | Location | Reason |
|------|----------|--------|
| BeingNode, CommunicationNode, EventNode, etc. | schema | Zero real usage |
| 18 unused edge types (SENT, RECEIVED, etc.) | schema | Zero real usage |

**Action:** Audit `src/schema/` for these types. Remove enum variants and contracts.

---

### Phase 2: Add SelfManager Module (Days 2-4)

Port the new `core/maintenance/` module from Python.

#### 2.1 Module Structure

```
src/maintenance/
├── mod.rs              # Module exports
├── monitor.rs          # Health checks, metrics, action triggers
├── janitor.rs          # FTS5 fix, lifecycle validation, orphan pruning
├── enricher.rs         # Embedding backfill, stage inference
├── archiver.rs         # Event archival, MENTIONS compression, vacuum
└── orchestrator.rs     # Monitor → Janitor → Enricher → Archiver pipeline
```

#### 2.2 HealthMonitor

```rust
// src/maintenance/monitor.rs
pub struct HealthMonitor {
    db: SqliteConnection,
}

pub struct HealthReport {
    pub fts5_coverage: f64,
    pub embedding_coverage: f64,
    pub drive_coverage: f64,
    pub stage_coverage: f64,
    pub edge_noise: f64,
    pub orphan_count: i64,
    pub event_growth_rate: f64,
    pub db_size_bytes: i64,
    pub health_score: f64,
    pub actions: Vec<Action>,
    pub timestamp: String,
}

impl HealthMonitor {
    pub fn check(&self) -> Result<HealthReport> {
        let fts5 = self.check_fts5_coverage()?;
        let embedding = self.check_embedding_coverage()?;
        let drive = self.check_drive_coverage()?;
        let stage = self.check_stage_coverage()?;
        let noise = self.check_edge_noise()?;
        let orphans = self.check_orphan_count()?;
        let growth = self.check_event_growth()?;
        let size = self.check_db_size()?;
        
        let score = self.compute_score(fts5, embedding, drive, stage, noise, orphans);
        let actions = self.determine_actions(fts5, embedding, drive, stage, noise, orphans, growth, size);
        
        Ok(HealthReport {
            fts5_coverage: fts5,
            embedding_coverage: embedding,
            drive_coverage: drive,
            stage_coverage: stage,
            edge_noise: noise,
            orphan_count: orphans,
            event_growth_rate: growth,
            db_size_bytes: size,
            health_score: score,
            actions,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }
}
```

#### 2.3 Janitor

```rust
// src/maintenance/janitor.rs
pub struct Janitor {
    db: SqliteConnection,
}

pub struct JanitorReport {
    pub fts5_indexed: i64,
    pub fts5_skipped: i64,
    pub vec_embedded: i64,
    pub vec_missing: i64,
    pub lifecycle_fixed: i64,
    pub edges_pruned: i64,
    pub parents_backfilled: i64,
    pub dry_run: bool,
    pub timestamp: String,
}

impl Janitor {
    pub fn run(&self, dry_run: bool) -> Result<JanitorReport> {
        let mut report = JanitorReport::default();
        
        self.fix_fts5(&mut report, dry_run)?;
        self.fix_vec_nodes(&mut report, dry_run)?;
        self.validate_lifecycle(&mut report, dry_run)?;
        self.prune_orphaned_edges(&mut report, dry_run)?;
        self.backfill_parent_ids(&mut report, dry_run)?;
        
        Ok(report)
    }
}
```

#### 2.4 Enricher

```rust
// src/maintenance/enricher.rs
pub struct Enricher {
    db: SqliteConnection,
    embedding_engine: Option<EmbeddingEngine>,
}

pub struct EnricherReport {
    pub embeddings_enriched: i64,
    pub embeddings_failed: i64,
    pub drives_enriched: i64,
    pub stages_enriched: i64,
    pub parents_enriched: i64,
    pub dry_run: bool,
    pub timestamp: String,
}

impl Enricher {
    pub fn run(&self, dry_run: bool) -> Result<EnricherReport> {
        let mut report = EnricherReport::default();
        
        self.enrich_embeddings(&mut report, dry_run)?;
        self.enrich_drives(&mut report, dry_run)?;
        self.enrich_stages(&mut report, dry_run)?;
        self.enrich_parents(&mut report, dry_run)?;
        
        Ok(report)
    }
}
```

#### 2.5 Archiver

```rust
// src/maintenance/archiver.rs
pub struct Archiver {
    db: SqliteConnection,
}

pub struct ArchiverReport {
    pub events_archived: i64,
    pub edges_pruned: i64,
    pub mentions_compressed: i64,
    pub vacuum_freed_bytes: i64,
    pub dry_run: bool,
    pub timestamp: String,
}

impl Archiver {
    pub fn run(&self, dry_run: bool) -> Result<ArchiverReport> {
        let mut report = ArchiverReport::default();
        
        self.archive_old_events(&mut report, dry_run)?;
        self.prune_dead_edges(&mut report, dry_run)?;
        self.compress_mentions(&mut report, dry_run)?;
        self.vacuum(&mut report, dry_run)?;
        
        Ok(report)
    }
}
```

#### 2.6 Orchestrator

```rust
// src/maintenance/orchestrator.rs
pub struct SelfManager {
    db: SqliteConnection,
}

pub struct SelfManagerReport {
    pub health_before: HealthReport,
    pub janitor: JanitorReport,
    pub enricher: EnricherReport,
    pub archiver: ArchiverReport,
    pub health_after: HealthReport,
    pub health_delta: f64,
    pub dry_run: bool,
    pub timestamp: String,
    pub duration_seconds: f64,
}

impl SelfManager {
    pub fn run(&self, dry_run: bool) -> Result<SelfManagerReport> {
        let start = std::time::Instant::now();
        
        let monitor = HealthMonitor::new(&self.db);
        let health_before = monitor.check()?;
        
        let janitor = Janitor::new(&self.db);
        let janitor_report = janitor.run(dry_run)?;
        
        let enricher = Enricher::new(&self.db, None);
        let enricher_report = enricher.run(dry_run)?;
        
        let archiver = Archiver::new(&self.db);
        let archiver_report = archiver.run(dry_run)?;
        
        let health_after = monitor.check()?;
        let health_delta = health_after.health_score - health_before.health_score;
        
        Ok(SelfManagerReport {
            health_before,
            janitor: janitor_report,
            enricher: enricher_report,
            archiver: archiver_report,
            health_after,
            health_delta,
            dry_run,
            timestamp: chrono::Utc::now().to_rfc3339(),
            duration_seconds: start.elapsed().as_secs_f64(),
        })
    }
}
```

---

### Phase 3: MCP Tool Integration (Day 5)

Add `tdg_self_manage` tool to the MCP server.

#### 3.1 Tool Definition

```rust
// src/mcp/tools.rs
pub fn tdg_self_manage(dry_run: Option<bool>) -> Result<Value> {
    let dry_run = dry_run.unwrap_or(true);
    
    let db = get_graph_db()?;
    let manager = SelfManager::new(db);
    let report = manager.run(dry_run)?;
    
    Ok(serde_json::json!({
        "summary": report.summary(),
        "health_before": report.health_before.health_score,
        "health_after": report.health_after.health_score,
        "health_delta": report.health_delta,
        "janitor": report.janitor.summary(),
        "enricher": report.enricher.summary(),
        "archiver": report.archiver.summary(),
        "duration_seconds": report.duration_seconds,
    }))
}
```

#### 3.2 Tool Registration

```rust
// src/mcp/server.rs
// Add to tool registration:
tools.insert("tdg_self_manage", tdg_self_manage);
```

---

### Phase 4: Schema Updates (Day 6)

Add missing tables and columns.

#### 4.1 New Tables

```sql
-- Mutation log (from magic-context pattern)
CREATE TABLE IF NOT EXISTS mutation_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    session_id TEXT,
    mutation_type TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    old_value TEXT,
    new_value TEXT,
    agent_id TEXT
);

-- Schema versioning
CREATE TABLE IF NOT EXISTS schema_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Leases (from magic-context pattern)
CREATE TABLE IF NOT EXISTS leases (
    domain TEXT PRIMARY KEY,
    holder_id TEXT NOT NULL,
    acquired_at REAL NOT NULL,
    expires_at REAL NOT NULL,
    renewal_count INTEGER DEFAULT 0
);
```

#### 4.2 New Columns

```sql
-- Add content hash for embedding dedup
ALTER TABLE embeddings ADD COLUMN content_hash TEXT;

-- Add tiered compression columns to nodes
ALTER TABLE nodes ADD COLUMN content_t1 TEXT;
ALTER TABLE nodes ADD COLUMN content_t2 TEXT;
ALTER TABLE nodes ADD COLUMN content_t3 TEXT;
ALTER TABLE nodes ADD COLUMN content_t4 TEXT;
```

---

### Phase 5: Efficiency Improvements (Day 7)

Port the efficiency fixes from Python TDG.

#### 5.1 EventStore._persist() — O(n) → O(1)

```rust
// Before: rewrite entire JSONL
// After: append only
fn persist(&self) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&self.path)?;
    
    let event = self.events.last().ok_or_else(|| anyhow!("No events"))?;
    writeln!(file, "{}", serde_json::to_string(event)?)?;
    
    Ok(())
}
```

#### 5.2 Remove graph_write_lock No-Op

```rust
// Before:
// with graph_write_lock() { ... }
// After:
// (just the body, no lock wrapper)
```

#### 5.3 Deduplicate _edge_exists

```rust
// Move to shared module
pub fn edge_exists(db: &SqliteConnection, src: &str, tgt: &str, etype: &str) -> Result<bool> {
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM edges WHERE source_id = ? AND target_id = ? AND edge_type = ? AND valid_to IS NULL",
        params![src, tgt, etype],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}
```

---

## Implementation Order

| Phase | Task | Days | Dependencies |
|-------|------|------|--------------|
| 1 | Remove dead code | 1 | None |
| 2 | Add SelfManager module | 3 | Phase 1 |
| 3 | MCP tool integration | 1 | Phase 2 |
| 4 | Schema updates | 1 | Phase 2 |
| 5 | Efficiency improvements | 1 | None |

**Total: 7 days**

---

## Testing Strategy

### Unit Tests
- HealthMonitor.check() with various graph states
- Janitor.run() with dry_run=True/False
- Enricher.run() with missing embeddings/drives/stages
- Archiver.run() with old events and MENTIONS edges
- SelfManager.run() end-to-end

### Integration Tests
- MCP tool tdg_self_manage with real database
- Full maintenance pipeline on test database
- Schema migration from v0.2.0 to v0.3.0

### Performance Tests
- Health check latency (< 100ms target)
- Janitor full run (< 5s target)
- Enricher batch processing (< 30s target)
- Archiver vacuum (< 10s target)

---

## Version Bump

After all phases complete:

```toml
[package]
version = "0.3.0"
```

---

## Success Criteria

1. **Dead code removed:** No references to deleted Python modules
2. **SelfManager working:** `tdg_self_manage` tool returns valid reports
3. **Schema updated:** New tables and columns exist
4. **Efficiency improved:** EventStore append, no-op locks removed
5. **Tests passing:** All 626+ tests pass
6. **Clippy clean:** Zero warnings
