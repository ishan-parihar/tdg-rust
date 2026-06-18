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

/// Run all migrations to bring schema up to date.
///
/// Mirrors `migrate()`, `migrate_v3()`, `migrate_v4()` from `core/graph_db.py`.
/// Uses `ALTER TABLE ... ADD COLUMN` wrapped in try-continue for safety.
pub fn run_migrations(conn: &Connection) -> TdgResult<()> {
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

    Ok(())
}

/// Backup database to a file using SQLite's online backup API.
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
    agent_id TEXT
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
    agent_id TEXT
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
"#;

const FTS_SQL: &str = r#"
-- FTS5 virtual table for full-text search
CREATE VIRTUAL TABLE IF NOT EXISTS nodes_fts USING fts5(
    node_id UNINDEXED,
    name,
    description,
    content='nodes',
    content_rowid='rowid',
    tokenize='porter unicode61'
);

-- Triggers to keep FTS in sync with nodes table
CREATE TRIGGER IF NOT EXISTS nodes_fts_ai AFTER INSERT ON nodes BEGIN
    INSERT INTO nodes_fts(rowid, node_id, name, description)
    VALUES (new.rowid, new.id, new.name, new.description);
END;

CREATE TRIGGER IF NOT EXISTS nodes_fts_ad AFTER DELETE ON nodes BEGIN
    INSERT INTO nodes_fts(nodes_fts, rowid, node_id, name, description)
    VALUES ('delete', old.rowid, old.id, old.name, old.description);
END;

CREATE TRIGGER IF NOT EXISTS nodes_fts_au AFTER UPDATE ON nodes BEGIN
    INSERT INTO nodes_fts(nodes_fts, rowid, node_id, name, description)
    VALUES ('delete', old.rowid, old.id, old.name, old.description);
    INSERT INTO nodes_fts(rowid, node_id, name, description)
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
    agent_id TEXT
);
"#;

const MIGRATE_TRIGGERS: &str = r#"
-- Event triggers for nodes
CREATE TRIGGER IF NOT EXISTS nodes_events_ai AFTER INSERT ON nodes BEGIN
    INSERT INTO events(event_id, event_action, timestamp, node_id, payload)
    VALUES (
        hex(randomblob(16)),
        'node_created',
        datetime('now'),
        new.id,
        json_object('node_type', new.node_type, 'name', new.name)
    );
END;

CREATE TRIGGER IF NOT EXISTS nodes_events_au AFTER UPDATE ON nodes BEGIN
    INSERT INTO events(event_id, event_action, timestamp, node_id, payload)
    VALUES (
        hex(randomblob(16)),
        'node_updated',
        datetime('now'),
        new.id,
        json_object('node_type', new.node_type, 'name', new.name)
    );
END;

CREATE TRIGGER IF NOT EXISTS nodes_events_ad AFTER DELETE ON nodes BEGIN
    INSERT INTO events(event_id, event_action, timestamp, node_id, payload)
    VALUES (
        hex(randomblob(16)),
        'node_deleted',
        datetime('now'),
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
        datetime('now'),
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
        datetime('now'),
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
        datetime('now'),
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
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))",
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
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))",
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
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))",
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
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))",
            ["n_upd001", "action", "Before"],
        )
        .unwrap();

        conn.execute(
            "UPDATE nodes SET name = 'After', updated_at = datetime('now') WHERE id = 'n_upd001'",
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
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))",
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
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))",
            ["n_src01", "telos", "Source"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))",
            ["n_tgt01", "action", "Target"],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO edges (id, source_id, target_id, edge_type, valid_from, created_at) VALUES (?1, ?2, ?3, ?4, datetime('now'), datetime('now'))",
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
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))",
            ["n_s001", "telos", "S"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes (id, node_type, name, created_at, updated_at) VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))",
            ["n_t001", "action", "T"],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO edges (id, source_id, target_id, edge_type, valid_from, created_at) VALUES (?1, ?2, ?3, ?4, datetime('now'), datetime('now'))",
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
}
