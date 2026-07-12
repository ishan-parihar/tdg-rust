use rusqlite::Connection;

use crate::error::{TdgError, TdgResult};

/// Initialize the database schema: create all tables, indexes.
///
/// Mirrors `init_schema()` from `core/graph_db.py`.
/// Does NOT create event triggers — those are handled by `run_migrations()`.
pub fn init_schema(conn: &Connection) -> TdgResult<()> {
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(())
}

/// Initialize FTS5 full-text search virtual table.
pub fn init_fts(conn: &Connection) -> TdgResult<()> {
    conn.execute_batch(FTS_SQL)?;
    Ok(())
}

/// Rebuild the FTS5 index from the nodes table.
///
/// Uses the FTS5 `rebuild` command for external-content tables (`content='nodes'`).
/// A plain `DELETE FROM nodes_fts` can fail with error 267 when shadow tables are
/// inconsistent; `rebuild` re-indexes from the content table safely.
pub fn rebuild_fts(conn: &Connection) -> TdgResult<()> {
    conn.execute("INSERT INTO nodes_fts(nodes_fts) VALUES('rebuild')", [])?;
    Ok(())
}

/// Run all migrations to bring schema up to date.
///
/// Mirrors `migrate()`, `migrate_v3()`, `migrate_v4()` from `core/graph_db.py`.
/// Uses `ALTER TABLE ... ADD COLUMN` wrapped in try-continue for safety.
pub fn run_migrations(conn: &Connection) -> TdgResult<()> {
    // Drop the legacy unique index if it exists and recreate it limited to active entity nodes
    // to bypass unique constraint failures on duplicate events or legacy skill nodes in existing DBs.
    conn.execute_batch("DROP INDEX IF EXISTS idx_nodes_name_type_active").ok();
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_name_type_active \
         ON nodes(name, node_type) WHERE valid_to IS NULL AND node_type = 'entity'",
    )?;

    // Phase 1: Add missing columns to nodes
    let migrate_columns = [
        ("nodes", "parent_ids", "TEXT DEFAULT '[]'"),
        ("nodes", "agent_id", "TEXT"),
        ("nodes", "agent_path", "TEXT DEFAULT ''"),
        ("nodes", "helpful_count", "INTEGER DEFAULT 0"),
        ("nodes", "retrieval_count", "INTEGER DEFAULT 0"),
        ("edges", "updated_at", "TEXT"),
        ("edges", "agent_id", "TEXT"),
    ];

    for (table, column, typedef) in &migrate_columns {
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {typedef}");
        // Ignore "duplicate column name" errors (code 1 = generic error, but we check message)
        match conn.execute_batch(&sql) {
            Ok(()) => {}
            Err(rusqlite::Error::ExecuteReturnedResults) => {}
            Err(_) => {
                // Column likely already exists — that's fine
            }
        }
    }

    // Phase 2: Create indexes
    conn.execute_batch(MIGRATE_INDEXES)?;

    // Phase 3: Ensure events table exists
    conn.execute_batch(MIGRATE_EVENTS)?;

    conn.execute_batch(MIGRATE_TRIGGERS)?;

    // Phase 5: New tables (mutation_log, schema_meta)
    conn.execute_batch(MIGRATE_NEW_TABLES)?;

    // Phase 6: Embedding dimension column (for mixed-size vector storage)
    conn.execute_batch("ALTER TABLE embeddings ADD COLUMN dimension INTEGER DEFAULT 384")
        .ok();

    // Phase 7: Fix FTS5 schema column name mismatch (P0 critical fix)
    // Drop stale triggers first — CREATE TRIGGER IF NOT EXISTS won't replace old node_id triggers.
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS nodes_fts_ai;
         DROP TRIGGER IF EXISTS nodes_fts_ad;
         DROP TRIGGER IF EXISTS nodes_fts_au;
         DROP TABLE IF EXISTS nodes_fts;",
    )
    .ok();
    init_fts(conn)?;
    rebuild_fts(conn)?;

    // Phase 8: Graph-level diagnostic history (Phase 0.3 of refactor).
    conn.execute_batch(MIGRATE_GRAPH_HISTORY)?;

    // Phase 9: Holonic scaffolding columns (Phase 1.2 of refactor).
    // Adds synthesis_status, scale_code, tetra_ul/ur/ll/lr, octave_id to nodes.
    // All nullable with defaults — backward-compatible with existing data.
    for (table, column, typedef) in &[
        ("nodes", "synthesis_status", "TEXT DEFAULT 'ai-draft'"),
        ("nodes", "scale_code", "TEXT"),
        ("nodes", "tetra_ul", "INTEGER"),
        ("nodes", "tetra_ur", "INTEGER"),
        ("nodes", "tetra_ll", "INTEGER"),
        ("nodes", "tetra_lr", "INTEGER"),
        ("nodes", "octave_id", "TEXT"),
    ] {
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {typedef}");
        match conn.execute_batch(&sql) {
            Ok(()) => {}
            Err(rusqlite::Error::ExecuteReturnedResults) => {}
            Err(_) => { /* column already exists */ }
        }
    }

    // Backfill synthesis_status for existing nodes (default to 'ai-draft').
    conn.execute_batch(
        "UPDATE nodes SET synthesis_status = 'ai-draft' WHERE synthesis_status IS NULL OR synthesis_status = ''",
    )
    .ok();

    // Backfill scale_code for existing nodes based on node_type inference.
    for (node_type, scale_code) in &[
        ("observation", "S40"),
        ("insight", "S40"),
        ("question", "S40"),
        ("people", "S40"),
        ("being", "S40"),
        ("skill", "S40"),
        ("capability", "S40"),
        ("action", "S40"),
        ("event", "S40"),
        ("communication", "S40"),
        ("project", "S30"),
        ("trajectory", "S30"),
        ("telos", "S30"),
        ("value", "S30"),
        ("bond", "S31"),
        ("hypothesis", "S70"),
        ("synthesis", "S70"),
        ("discovery", "S70"),
        ("constraint", "S70"),
        ("narrative", "S70"),
        ("artifact", "S50"),
    ] {
        let sql = format!(
            "UPDATE nodes SET scale_code = '{scale_code}' WHERE node_type = '{node_type}' AND scale_code IS NULL"
        );
        conn.execute_batch(&sql).ok();
    }

    // Phase 10: Lesser cycle + metabolism infrastructure (Phase 2 of refactor).
    conn.execute_batch("ALTER TABLE nodes ADD COLUMN lesser_cycle_json TEXT")
        .ok();

    conn.execute_batch(MIGRATE_METABOLISM)?;

    // Phase 11: Attractor field + health metrics (Phase 3 of refactor).
    for (table, column, typedef) in &[
        ("nodes", "attractor_field_json", "TEXT"),
        ("nodes", "health_json", "TEXT"),
        ("nodes", "attractor_dirty", "INTEGER DEFAULT 0"),
        ("nodes", "health_dirty", "INTEGER DEFAULT 0"),
    ] {
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {typedef}");
        match conn.execute_batch(&sql) {
            Ok(()) => {}
            Err(rusqlite::Error::ExecuteReturnedResults) => {}
            Err(_) => { /* column already exists */ }
        }
    }

    conn.execute_batch(MIGRATE_RESONANCE_GRAPH)?;

    // Phase 12: Greater cycle state (Phase 4 of refactor).
    conn.execute_batch("ALTER TABLE nodes ADD COLUMN greater_cycle_json TEXT")
        .ok();

    // Phase 13: V/C/R/N coordinate system (audit Phase 7).
    // - realm_placement: "gross" | "subtle" | "causal"
    // - verticality_json: {"octave": N, "density": D, "sub_density": S}
    // - collectivity: "individual" | "collective" | "universal"
    // - nesting_sub, nesting_sup: directional exploration depth
    for (table, column, typedef) in &[
        ("nodes", "realm_placement", "TEXT"),
        ("nodes", "verticality_json", "TEXT"),
        ("nodes", "collectivity", "TEXT"),
        ("nodes", "nesting_sub", "INTEGER DEFAULT 0"),
        ("nodes", "nesting_sup", "INTEGER DEFAULT 0"),
    ] {
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {typedef}");
        match conn.execute_batch(&sql) {
            Ok(()) => {}
            Err(rusqlite::Error::ExecuteReturnedResults) => {}
            Err(_) => { /* column already exists */ }
        }
    }

    // Backfill realm_placement for existing nodes based on node_type inference.
    for (node_type, realm) in &[
        ("observation", "gross"),
        ("event", "gross"),
        ("artifact", "gross"),
        ("action", "gross"),
        ("people", "gross"),
        ("being", "gross"),
        ("communication", "gross"),
        ("skill", "subtle"),
        ("capability", "subtle"),
        ("hypothesis", "subtle"),
        ("synthesis", "subtle"),
        ("insight", "subtle"),
        ("discovery", "subtle"),
        ("question", "subtle"),
        ("constraint", "subtle"),
        ("narrative", "subtle"),
        ("telos", "causal"),
        ("value", "causal"),
        ("bond", "subtle"),
        ("project", "gross"),
        ("trajectory", "subtle"),
    ] {
        let sql = format!(
            "UPDATE nodes SET realm_placement = '{realm}' WHERE node_type = '{node_type}' AND realm_placement IS NULL"
        );
        conn.execute_batch(&sql).ok();
    }

    // Phase 16: Hebbian edge-weight learning — co-activation tracking.
    // - co_activation_count: how many times source+target co-activated (LTP signal)
    // - last_co_activation: timestamp of last co-activation (for LTD decay)
    for (table, column, typedef) in &[
        ("edges", "co_activation_count", "INTEGER DEFAULT 0"),
        ("edges", "last_co_activation", "TEXT"),
        ("nodes", "salience_tag", "TEXT DEFAULT 'normal'"),
    ] {
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {typedef}");
        match conn.execute_batch(&sql) {
            Ok(()) => {}
            Err(rusqlite::Error::ExecuteReturnedResults) => {}
            Err(_) => { /* column already exists */ }
        }
    }

    // Phase 23: Soft-delete support for events table.
    conn.execute_batch("ALTER TABLE events ADD COLUMN archived_at TEXT")
        .ok();

    Ok(())
}
pub fn backup_database(conn: &Connection, backup_path: &std::path::Path) -> TdgResult<()> {
    // Ensure parent directory exists
    if let Some(parent) = backup_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut backup_conn = Connection::open(backup_path)?;

    // Use SQLite's built-in online backup API via rusqlite
    let backup = rusqlite::backup::Backup::new(conn, &mut backup_conn)
        .map_err(|e| TdgError::Custom(format!("Failed to create backup: {e}")))?;

    backup
        .run_to_completion(100, std::time::Duration::from_millis(250), None)
        .map_err(|e| TdgError::Custom(format!("Backup failed: {e}")))?;

    Ok(())
}

const SCHEMA_SQL: &str = r#"
-- Nodes table
CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,
    node_type TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT DEFAULT '',
    properties_json TEXT DEFAULT '{}',
    quadrants_json TEXT DEFAULT '{}',
    drives_json TEXT DEFAULT '{}',
    lifecycle_state TEXT DEFAULT 'active',
    teleological_level TEXT,
    developmental_stage INTEGER,
    confidence REAL DEFAULT 1.0,
    source TEXT DEFAULT '',
    parent_ids TEXT DEFAULT '[]',
    agent_path TEXT DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    valid_from TEXT,
    valid_to TEXT,
    helpful_count INTEGER DEFAULT 0,
    retrieval_count INTEGER DEFAULT 0,
    agent_id TEXT,
    -- Phase 1.2: Holonic scaffolding fields
    synthesis_status TEXT DEFAULT 'ai-draft',
    scale_code TEXT,
    tetra_ul INTEGER,
    tetra_ur INTEGER,
    tetra_ll INTEGER,
    tetra_lr INTEGER,
    octave_id TEXT,
    -- Phase 2: Lesser cycle state (M·P·C·E)
    lesser_cycle_json TEXT,
    -- Phase 3: Attractor field + health
    attractor_field_json TEXT,
    health_json TEXT,
    attractor_dirty INTEGER DEFAULT 0,
    health_dirty INTEGER DEFAULT 0,
    -- Phase 4: Greater cycle state (S·T·G·Ch)
    greater_cycle_json TEXT,
    -- Phase 7: V/C/R/N coordinate system
    realm_placement TEXT,
    verticality_json TEXT,
    collectivity TEXT,
    nesting_sub INTEGER DEFAULT 0,
    nesting_sup INTEGER DEFAULT 0
);

-- Edges table
CREATE TABLE IF NOT EXISTS edges (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES nodes(id),
    target_id TEXT NOT NULL REFERENCES nodes(id),
    edge_type TEXT NOT NULL,
    weight REAL DEFAULT 1.0,
    properties_json TEXT DEFAULT '{}',
    valid_from TEXT,
    valid_to TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT,
    agent_id TEXT
);

-- Embeddings table
CREATE TABLE IF NOT EXISTS embeddings (
    node_id TEXT NOT NULL REFERENCES nodes(id),
    vector BLOB,
    model TEXT DEFAULT 'default',
    updated_at TEXT NOT NULL,
    PRIMARY KEY (node_id)
);

-- Events table (audit trail)
CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT,
    event_action TEXT,
    timestamp TEXT,
    node_id TEXT,
    source_id TEXT,
    target_id TEXT,
    payload TEXT,
    agent_id TEXT,
    archived_at TEXT
);

-- Indexes for nodes
CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(node_type);
CREATE INDEX IF NOT EXISTS idx_nodes_lifecycle ON nodes(lifecycle_state);
CREATE INDEX IF NOT EXISTS idx_nodes_source ON nodes(source);
CREATE INDEX IF NOT EXISTS idx_nodes_valid_to ON nodes(valid_to);
CREATE INDEX IF NOT EXISTS idx_nodes_created ON nodes(created_at);
CREATE INDEX IF NOT EXISTS idx_nodes_type_valid_created ON nodes(node_type, valid_to, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_nodes_agent_valid ON nodes(agent_id, valid_to);
CREATE INDEX IF NOT EXISTS idx_nodes_agent_id ON nodes(agent_id);

-- Unique index on (name, node_type) for active nodes only.
-- Prevents TOCTOU race in add_node's entity resolution: two concurrent calls
-- with the same name+type both pass the existence check and both INSERT.
-- The partial index (WHERE valid_to IS NULL) allows archived/deleted nodes
-- to share names with active ones.
CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_name_type_active
    ON nodes(name, node_type) WHERE valid_to IS NULL AND node_type = 'entity';

-- Indexes for edges
CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_id);
CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_id);
CREATE INDEX IF NOT EXISTS idx_edges_type ON edges(edge_type);
CREATE INDEX IF NOT EXISTS idx_edges_valid_to ON edges(valid_to);
CREATE INDEX IF NOT EXISTS idx_edges_created ON edges(created_at);
CREATE INDEX IF NOT EXISTS idx_edges_type_valid ON edges(edge_type, valid_to);
CREATE INDEX IF NOT EXISTS idx_edges_source_target_valid ON edges(source_id, target_id, valid_to);

-- Indexes for events
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
CREATE INDEX IF NOT EXISTS idx_events_action ON events(event_action);
CREATE INDEX IF NOT EXISTS idx_events_node ON events(node_id);
CREATE INDEX IF NOT EXISTS idx_events_source ON events(source_id);
CREATE INDEX IF NOT EXISTS idx_events_target ON events(target_id);

-- Health checks table
CREATE TABLE IF NOT EXISTS health_checks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    service TEXT NOT NULL,
    latency_ms REAL NOT NULL,
    success INTEGER NOT NULL,
    error_message TEXT,
    metadata TEXT,
    timestamp TEXT NOT NULL
);

-- Indexes for health_checks
CREATE INDEX IF NOT EXISTS idx_health_checks_service ON health_checks(service);
CREATE INDEX IF NOT EXISTS idx_health_checks_timestamp ON health_checks(timestamp);

-- Trust scores table (per-agent trust persistence)
CREATE TABLE IF NOT EXISTS trust_scores (
    agent_id TEXT PRIMARY KEY,
    score REAL NOT NULL DEFAULT 0.5,
    updated_at TEXT NOT NULL,
    source TEXT,
    reason TEXT
);

-- Graph-level diagnostic history (Phase 0.3 of refactor).
-- See MIGRATE_GRAPH_HISTORY for full rationale.
CREATE TABLE IF NOT EXISTS graph_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    recorded_at TEXT NOT NULL,
    dominant_drive TEXT,
    dominant_quadrant TEXT,
    drive_distribution TEXT,
    quadrant_distribution TEXT,
    escalation_level TEXT
);

CREATE INDEX IF NOT EXISTS idx_graph_history_recorded ON graph_history(recorded_at);

-- Phase 2: Metabolism infrastructure (Tier 2 job queue)
CREATE TABLE IF NOT EXISTS pending_metabolism (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    holon_id TEXT NOT NULL,
    job_type TEXT NOT NULL,
    payload TEXT,
    enqueued_at TEXT NOT NULL,
    priority INTEGER DEFAULT 1,
    attempts INTEGER DEFAULT 0,
    max_attempts INTEGER DEFAULT 3
);
CREATE INDEX IF NOT EXISTS idx_pending_priority ON pending_metabolism(priority DESC, enqueued_at);
CREATE INDEX IF NOT EXISTS idx_pending_holon ON pending_metabolism(holon_id);

CREATE TABLE IF NOT EXISTS failed_metabolism (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    holon_id TEXT,
    job_type TEXT,
    payload TEXT,
    error TEXT,
    failed_at TEXT NOT NULL,
    attempts INTEGER
);
CREATE INDEX IF NOT EXISTS idx_failed_holon ON failed_metabolism(holon_id);

-- Phase 3: Resonance graph (materialized top-K partners per holon)
CREATE TABLE IF NOT EXISTS resonance_graph (
    holon_id TEXT NOT NULL,
    partner_id TEXT NOT NULL,
    resonance_score REAL NOT NULL,
    complementarity REAL NOT NULL,
    gamma_compat REAL NOT NULL,
    great_way_intersect REAL NOT NULL,
    computed_at TEXT NOT NULL,
    PRIMARY KEY (holon_id, partner_id)
);
CREATE INDEX IF NOT EXISTS idx_resonance_holon_score ON resonance_graph(holon_id, resonance_score DESC);
"#;

const FTS_SQL: &str = r#"
-- FTS5 virtual table for full-text search
CREATE VIRTUAL TABLE IF NOT EXISTS nodes_fts USING fts5(
    id UNINDEXED,
    name,
    description,
    content='nodes',
    content_rowid='rowid',
    tokenize='porter unicode61'
);

-- Triggers to keep FTS in sync with nodes table
CREATE TRIGGER IF NOT EXISTS nodes_fts_ai AFTER INSERT ON nodes BEGIN
    INSERT INTO nodes_fts(rowid, id, name, description)
    VALUES (new.rowid, new.id, new.name, new.description);
END;

CREATE TRIGGER IF NOT EXISTS nodes_fts_ad AFTER DELETE ON nodes BEGIN
    INSERT INTO nodes_fts(nodes_fts, rowid, id, name, description)
    VALUES ('delete', old.rowid, old.id, old.name, old.description);
END;

CREATE TRIGGER IF NOT EXISTS nodes_fts_au AFTER UPDATE ON nodes BEGIN
    INSERT INTO nodes_fts(nodes_fts, rowid, id, name, description)
    VALUES ('delete', old.rowid, old.id, old.name, old.description);
    INSERT INTO nodes_fts(rowid, id, name, description)
    VALUES (new.rowid, new.id, new.name, new.description);
END;
"#;

const MIGRATE_INDEXES: &str = r#"
-- Ensure indexes exist
CREATE INDEX IF NOT EXISTS idx_nodes_agent_id ON nodes(agent_id);
CREATE INDEX IF NOT EXISTS idx_nodes_type_valid_created ON nodes(node_type, valid_to, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
CREATE INDEX IF NOT EXISTS idx_events_action ON events(event_action);
CREATE INDEX IF NOT EXISTS idx_events_node ON events(node_id);
"#;

const MIGRATE_EVENTS: &str = r#"
-- Ensure events table exists
CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT,
    event_action TEXT,
    timestamp TEXT,
    node_id TEXT,
    source_id TEXT,
    target_id TEXT,
    payload TEXT,
    agent_id TEXT,
    archived_at TEXT
);
"#;

const MIGRATE_NEW_TABLES: &str = r#"
-- Mutation log (audit trail for all graph mutations)
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

CREATE INDEX IF NOT EXISTS idx_mutation_timestamp ON mutation_log(timestamp);
CREATE INDEX IF NOT EXISTS idx_mutation_target ON mutation_log(target_type, target_id);

-- Schema versioning
CREATE TABLE IF NOT EXISTS schema_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR IGNORE INTO schema_meta (key, value) VALUES ('version', '1');
"#;

const MIGRATE_GRAPH_HISTORY: &str = r#"
-- Graph-level diagnostic history (Phase 8 / refactor Phase 0.3).
-- Stores one row per diagnostic cycle so the DiagnosticEngine and
-- FeelingEngine can detect persistence and stuck patterns over time.
-- The injector previously passed &[] for both histories, leaving
-- detect_drive_persistence, detect_quadrant_persistence, and
-- detect_stuck_pattern as dead code in production.
CREATE TABLE IF NOT EXISTS graph_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    recorded_at TEXT NOT NULL,
    dominant_drive TEXT,          -- e.g. "addicted", "healthy", "blind"
    dominant_quadrant TEXT,       -- e.g. "UL", "UR", "LL", "LR"
    drive_distribution TEXT,      -- JSON: {"eros": 2.1, "agape": -0.3, ...}
    quadrant_distribution TEXT,   -- JSON: {"UL": 30.0, "UR": 50.0, ...}
    escalation_level TEXT         -- e.g. "normal", "warning", "critical"
);

CREATE INDEX IF NOT EXISTS idx_graph_history_recorded ON graph_history(recorded_at);
"#;

const MIGRATE_METABOLISM: &str = r#"
-- Phase 2: Metabolism infrastructure (Tier 2 job queue).
-- The pending_metabolism table holds async jobs for the lesser cycle,
-- attractor field, health, and resonance computations. Jobs are enqueued
-- by Tier 1 write paths (tdg_connect, tdg_observe) and processed by
-- background MetabolismWorker threads.
CREATE TABLE IF NOT EXISTS pending_metabolism (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    holon_id TEXT NOT NULL,
    job_type TEXT NOT NULL,        -- 'lesser_tick', 'catalyst_injection', 'recompute_attractor', 'recompute_health', 'resonance_update'
    payload TEXT,                  -- JSON
    enqueued_at TEXT NOT NULL,
    priority INTEGER DEFAULT 1,   -- 0=low, 1=normal, 2=high
    attempts INTEGER DEFAULT 0,
    max_attempts INTEGER DEFAULT 3
);

CREATE INDEX IF NOT EXISTS idx_pending_priority ON pending_metabolism(priority DESC, enqueued_at);
CREATE INDEX IF NOT EXISTS idx_pending_holon ON pending_metabolism(holon_id);

-- Dead-letter queue for jobs that exceeded max_attempts.
-- Allows manual inspection and retry via tdg_retry_failed.
CREATE TABLE IF NOT EXISTS failed_metabolism (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    holon_id TEXT,
    job_type TEXT,
    payload TEXT,
    error TEXT,
    failed_at TEXT NOT NULL,
    attempts INTEGER
);

CREATE INDEX IF NOT EXISTS idx_failed_holon ON failed_metabolism(holon_id);
"#;

const MIGRATE_RESONANCE_GRAPH: &str = r#"
-- Phase 3: Resonance graph (materialized top-K partners per holon).
-- Precomputes R(H1, H2) for bonding predictions. Incrementally updated
-- when a holon's attractor field changes. Full rebuild every 4 hours
-- (Tier 3 schedule) to correct incremental drift.
CREATE TABLE IF NOT EXISTS resonance_graph (
    holon_id TEXT NOT NULL,
    partner_id TEXT NOT NULL,
    resonance_score REAL NOT NULL,
    complementarity REAL NOT NULL,
    gamma_compat REAL NOT NULL,
    great_way_intersect REAL NOT NULL,
    computed_at TEXT NOT NULL,
    PRIMARY KEY (holon_id, partner_id)
);

CREATE INDEX IF NOT EXISTS idx_resonance_holon_score ON resonance_graph(holon_id, resonance_score DESC);
"#;

const MIGRATE_TRIGGERS: &str = r#"
-- Event triggers for nodes
CREATE TRIGGER IF NOT EXISTS nodes_events_ai AFTER INSERT ON nodes BEGIN
    INSERT INTO events(event_id, event_action, timestamp, node_id, payload)
    VALUES (
        hex(randomblob(16)),
        'node_created',
        strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
        new.id,
        json_object('node_type', new.node_type, 'name', new.name)
    );
END;

-- Only fire on user-meaningful column changes (not retrieval_count bumps,
-- drives_json updates from flow propagation, or maintenance backfills).
-- Previously, EVERY UPDATE generated a node_updated event — including
-- record_retrieval (search hits), store_drive_state (flow propagation),
-- and Janitor/Enricher maintenance. The events table was >90% noise,
-- making the audit trail nearly useless. The WHEN clause reduces event
-- volume by an estimated 80-90%.
CREATE TRIGGER IF NOT EXISTS nodes_events_au AFTER UPDATE ON nodes
WHEN new.name != old.name
   OR new.description != old.description
   OR new.node_type != old.node_type
   OR new.lifecycle_state != old.lifecycle_state
   OR new.confidence != old.confidence
   OR new.teleological_level IS NOT old.teleological_level
   OR new.developmental_stage IS NOT old.developmental_stage
   OR new.valid_to IS NOT old.valid_to
BEGIN
    INSERT INTO events(event_id, event_action, timestamp, node_id, payload)
    VALUES (
        hex(randomblob(16)),
        'node_updated',
        strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
        new.id,
        json_object('node_type', new.node_type, 'name', new.name)
    );
END;

CREATE TRIGGER IF NOT EXISTS nodes_events_ad AFTER DELETE ON nodes BEGIN
    INSERT INTO events(event_id, event_action, timestamp, node_id, payload)
    VALUES (
        hex(randomblob(16)),
        'node_deleted',
        strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
        old.id,
        json_object('node_type', old.node_type, 'name', old.name)
    );
END;

-- Event triggers for edges
CREATE TRIGGER IF NOT EXISTS edges_events_ai AFTER INSERT ON edges BEGIN
    INSERT INTO events(event_id, event_action, timestamp, node_id, source_id, target_id, payload)
    VALUES (
        hex(randomblob(16)),
        'edge_created',
        strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
        NULL,
        new.source_id,
        new.target_id,
        json_object('edge_type', new.edge_type, 'weight', new.weight)
    );
END;

CREATE TRIGGER IF NOT EXISTS edges_events_au AFTER UPDATE ON edges BEGIN
    INSERT INTO events(event_id, event_action, timestamp, node_id, source_id, target_id, payload)
    VALUES (
        hex(randomblob(16)),
        'edge_updated',
        strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
        NULL,
        new.source_id,
        new.target_id,
        json_object('edge_type', new.edge_type, 'weight', new.weight)
    );
END;

CREATE TRIGGER IF NOT EXISTS edges_events_ad AFTER DELETE ON edges BEGIN
    INSERT INTO events(event_id, event_action, timestamp, node_id, source_id, target_id, payload)
    VALUES (
        hex(randomblob(16)),
        'edge_deleted',
        strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
        NULL,
        old.source_id,
        old.target_id,
        json_object('edge_type', old.edge_type, 'weight', old.weight)
    );
END;
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn schema_creates_all_tables() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();

        let tables: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            let rows = stmt.query_map([], |row| row.get(0)).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };

        assert!(tables.contains(&"nodes".to_string()));
        assert!(tables.contains(&"edges".to_string()));
        assert!(tables.contains(&"embeddings".to_string()));
        assert!(tables.contains(&"events".to_string()));
        assert!(tables.contains(&"nodes_fts".to_string()));
        assert!(tables.contains(&"health_checks".to_string()));
        assert!(tables.contains(&"mutation_log".to_string()));
        assert!(tables.contains(&"schema_meta".to_string()));
        // leases table removed (yagni — WriteGuard uses file locks)
        assert!(!tables.contains(&"leases".to_string()));
    }

    #[test]
    fn schema_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();
    }

    #[test]
    fn triggers_fire_on_insert() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now', 'subsec'), datetime('now', 'subsec'))",
            ["n_test001", "observation", "Test Node"],
        )
        .unwrap();

        let event_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE node_id='n_test001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(event_count, 1);
    }

    #[test]
    fn backup_works() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now', 'subsec'), datetime('now', 'subsec'))",
            ["n_backup_test", "observation", "Backup Test"],
        )
        .unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        backup_database(&conn, tmp.path()).unwrap();

        let backup_conn = Connection::open(tmp.path()).unwrap();
        let count: i32 = backup_conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn trigger_populates_payload_on_insert() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now', 'subsec'), datetime('now', 'subsec'))",
            ["n_cap001", "telos", "Capture Test"],
        )
        .unwrap();

        let row: String = conn
            .query_row(
                "SELECT payload FROM events WHERE event_action = 'node_created' AND node_id = 'n_cap001' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();

        let data: serde_json::Value = serde_json::from_str(&row).unwrap();
        assert_eq!(data["node_type"].as_str().unwrap(), "telos");
        assert_eq!(data["name"].as_str().unwrap(), "Capture Test");
    }

    #[test]
    fn trigger_populates_payload_on_update() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now', 'subsec'), datetime('now', 'subsec'))",
            ["n_upd001", "action", "Before"],
        )
        .unwrap();

        conn.execute(
            "UPDATE nodes SET name = 'After', updated_at = datetime('now', 'subsec') WHERE id = 'n_upd001'",
            [],
        )
        .unwrap();

        let row: String = conn
            .query_row(
                "SELECT payload FROM events WHERE event_action = 'node_updated' AND node_id = 'n_upd001' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();

        let data: serde_json::Value = serde_json::from_str(&row).unwrap();
        assert_eq!(data["node_type"].as_str().unwrap(), "action");
    }

    #[test]
    fn trigger_populates_payload_on_delete() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now', 'subsec'), datetime('now', 'subsec'))",
            ["n_del001", "observation", "Delete Me"],
        )
        .unwrap();

        conn.execute("DELETE FROM nodes WHERE id = 'n_del001'", [])
            .unwrap();

        let row: String = conn
            .query_row(
                "SELECT payload FROM events WHERE event_action = 'node_deleted' AND node_id = 'n_del001' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();

        let data: serde_json::Value = serde_json::from_str(&row).unwrap();
        assert_eq!(data["node_type"].as_str().unwrap(), "observation");
    }

    #[test]
    fn trigger_populates_payload_on_edge_insert() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now', 'subsec'), datetime('now', 'subsec'))",
            ["n_src01", "telos", "Source"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now', 'subsec'), datetime('now', 'subsec'))",
            ["n_tgt01", "action", "Target"],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO edges (id, source_id, target_id, edge_type, valid_from, created_at) VALUES (?1, ?2, ?3, ?4, datetime('now', 'subsec'), datetime('now', 'subsec'))",
            ["e_edge01", "n_src01", "n_tgt01", "ENABLES"],
        )
        .unwrap();

        let row: String = conn
            .query_row(
                "SELECT payload FROM events WHERE event_action = 'edge_created' AND source_id = 'n_src01' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();

        let data: serde_json::Value = serde_json::from_str(&row).unwrap();
        assert_eq!(data["edge_type"].as_str().unwrap(), "ENABLES");
    }

    #[test]
    fn trigger_populates_payload_on_edge_delete() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now', 'subsec'), datetime('now', 'subsec'))",
            ["n_s001", "telos", "S"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now', 'subsec'), datetime('now', 'subsec'))",
            ["n_t001", "action", "T"],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO edges (id, source_id, target_id, edge_type, valid_from, created_at) VALUES (?1, ?2, ?3, ?4, datetime('now', 'subsec'), datetime('now', 'subsec'))",
            ["e_del01", "n_s001", "n_t001", "ENABLES"],
        )
        .unwrap();

        conn.execute("DELETE FROM edges WHERE id = 'e_del01'", [])
            .unwrap();

        let row: String = conn
            .query_row(
                "SELECT payload FROM events WHERE event_action = 'edge_deleted' AND source_id = 'n_s001' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();

        let data: serde_json::Value = serde_json::from_str(&row).unwrap();
        assert_eq!(data["edge_type"].as_str().unwrap(), "ENABLES");
    }

    #[test]
    fn migration_replaces_legacy_fts_triggers() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        // Simulate pre-fix database: FTS table and triggers used node_id column name.
        conn.execute_batch(
            "CREATE VIRTUAL TABLE nodes_fts USING fts5(
                node_id UNINDEXED,
                name,
                description,
                content='nodes',
                content_rowid='rowid',
                tokenize='porter unicode61'
            );
            CREATE TRIGGER nodes_fts_ai AFTER INSERT ON nodes BEGIN
                INSERT INTO nodes_fts(rowid, node_id, name, description)
                VALUES (new.rowid, new.id, new.name, new.description);
            END;",
        )
        .unwrap();

        run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO nodes (id, node_type, name, description, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now', 'subsec'), datetime('now', 'subsec'))",
            rusqlite::params!["n_fts_legacy", "observation", "Legacy FTS", "desc"],
        )
        .unwrap();

        let fts_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes_fts WHERE id = 'n_fts_legacy'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 1);

        let fts_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM nodes_fts", [], |r| r.get(0))
            .unwrap();
        assert!(fts_rows >= 1);
    }
}
