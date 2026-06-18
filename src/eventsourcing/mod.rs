use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Config;
use crate::db::crud::{self, now_iso};
use crate::db::schema::init_schema;
use crate::error::{TdgError, TdgResult};

// ---------------------------------------------------------------------------
// Event Journal (JSONL-backed)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub event_id: String,
    pub event_action: String,
    pub timestamp: String,
    pub node_id: Option<String>,
    pub source_id: Option<String>,
    pub target_id: Option<String>,
    pub agent_id: Option<String>,
    pub payload: Value,
}

/// Thread-safe JSONL event journal.
pub struct EventJournal {
    path: PathBuf,
    events: Mutex<Vec<JournalEntry>>,
}

impl EventJournal {
    pub fn new(path: impl Into<PathBuf>) -> TdgResult<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let events = Self::load_from_disk(&path)?;
        Ok(Self {
            path,
            events: Mutex::new(events),
        })
    }

    fn load_from_disk(path: &Path) -> TdgResult<Vec<JournalEntry>> {
        let mut entries = Vec::new();
        if !path.exists() {
            return Ok(entries);
        }
        let file = File::open(path).map_err(TdgError::Io)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line.map_err(TdgError::Io)?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<JournalEntry>(line) {
                Ok(entry) => entries.push(entry),
                Err(_) => {
                    // Corrupt line: start fresh (matches Python behavior)
                    entries.clear();
                    break;
                }
            }
        }
        Ok(entries)
    }

    /// Append an event to the journal (both in-memory and on disk).
    pub fn append(
        &self,
        event_action: &str,
        node_id: Option<&str>,
        source_id: Option<&str>,
        target_id: Option<&str>,
        agent_id: Option<&str>,
        payload: Option<&Value>,
    ) -> TdgResult<String> {
        let event_id = format!("evt:{}", &uuid::Uuid::new_v4().to_string()[..12]);
        let timestamp = now_iso();
        let payload = payload
            .cloned()
            .unwrap_or(Value::Object(serde_json::Map::new()));

        let entry = JournalEntry {
            event_id: event_id.clone(),
            event_action: event_action.to_string(),
            timestamp,
            node_id: node_id.map(String::from),
            source_id: source_id.map(String::from),
            target_id: target_id.map(String::from),
            agent_id: agent_id.map(String::from),
            payload,
        };

        // Append to disk
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(TdgError::Io)?;
        let mut line = serde_json::to_string(&entry)?;
        line.push('\n');
        file.write_all(line.as_bytes()).map_err(TdgError::Io)?;

        // Append to in-memory cache
        self.events.lock().unwrap().push(entry);

        Ok(event_id)
    }

    /// Count of events in journal.
    pub fn count(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    /// Get the last event ID.
    pub fn last_event_id(&self) -> Option<String> {
        self.events
            .lock()
            .unwrap()
            .last()
            .map(|e| e.event_id.clone())
    }

    /// Stream events after a given event ID.
    pub fn stream_after(
        &self,
        after_event_id: Option<&str>,
        limit: Option<usize>,
    ) -> Vec<JournalEntry> {
        let events = self.events.lock().unwrap();
        let start = if let Some(aid) = after_event_id {
            events
                .iter()
                .position(|e| e.event_id == *aid)
                .map(|i| i + 1)
                .unwrap_or(0)
        } else {
            0
        };
        let mut result: Vec<_> = events[start..].to_vec();
        if let Some(limit) = limit {
            result.truncate(limit);
        }
        result
    }

    /// Stream events up to a timestamp.
    pub fn stream_up_to(&self, timestamp: &str) -> Vec<JournalEntry> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.timestamp.as_str() <= timestamp)
            .cloned()
            .collect()
    }

    /// Backup journal to a target path.
    pub fn backup(&self, target: impl Into<PathBuf>) -> TdgResult<PathBuf> {
        let target = target.into();
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&self.path, &target)?;
        Ok(target)
    }

    /// Restore journal from a backup.
    pub fn restore(&self, backup_path: impl AsRef<Path>) -> TdgResult<()> {
        let backup = backup_path.as_ref();
        if !backup.exists() {
            return Err(TdgError::NotFound(format!(
                "backup not found: {}",
                backup.display()
            )));
        }
        fs::copy(backup, &self.path)?;
        let events = Self::load_from_disk(&self.path)?;
        *self.events.lock().unwrap() = events;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Snapshot Manager
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub generated_at: String,
    pub tag: String,
    pub event_count: usize,
    pub node_count: i64,
    pub edge_count: i64,
    pub node_types: Value,
}

pub struct SnapshotManager {
    snapshot_dir: PathBuf,
}

impl SnapshotManager {
    pub fn new(cfg: &Config) -> Self {
        let snapshot_dir = cfg.snapshots_dir();
        let _ = fs::create_dir_all(&snapshot_dir);
        Self { snapshot_dir }
    }

    /// Save a snapshot of current DB state.
    pub fn save_snapshot(&self, conn: &rusqlite::Connection, tag: &str) -> TdgResult<PathBuf> {
        let now = now_iso();
        let uuid_short = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let filename = format!("{}_{}_{}.json", now.replace(':', "-"), tag, uuid_short);
        let path = self.snapshot_dir.join(filename);

        // Gather stats
        let node_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
        let edge_count: i64 = conn.query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))?;

        // Node type distribution
        let mut stmt = conn.prepare(
            "SELECT node_type, COUNT(*) as cnt FROM nodes GROUP BY node_type ORDER BY cnt DESC",
        )?;
        let type_map: Value = stmt
            .query_map([], |row| {
                let nt: String = row.get(0)?;
                let cnt: i64 = row.get(1)?;
                Ok((nt, cnt))
            })?
            .filter_map(|r| r.ok())
            .map(|(k, v)| (k, Value::Number(v.into())))
            .collect::<serde_json::Map<String, Value>>()
            .into();

        let event_count: usize = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap_or(0);

        let meta = SnapshotMeta {
            generated_at: now,
            tag: tag.to_string(),
            event_count,
            node_count,
            edge_count,
            node_types: type_map,
        };

        let json = serde_json::to_string_pretty(&meta)?;
        fs::write(&path, json)?;
        Ok(path)
    }

    /// Verify a snapshot file is valid JSON.
    pub fn verify_snapshot(&self, path: &Path) -> TdgResult<bool> {
        if !path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(path)?;
        match serde_json::from_str::<SnapshotMeta>(&content) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// List all snapshots, sorted by filename (chronological).
    pub fn list_snapshots(&self) -> TdgResult<Vec<PathBuf>> {
        let mut paths: Vec<PathBuf> = fs::read_dir(&self.snapshot_dir)
            .map_err(TdgError::Io)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|ext| ext == "json").unwrap_or(false))
            .collect();
        paths.sort();
        Ok(paths)
    }

    /// Load snapshot metadata.
    pub fn load_snapshot(&self, path: &Path) -> TdgResult<SnapshotMeta> {
        let content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }
}

// ---------------------------------------------------------------------------
// Replay Engine
// ---------------------------------------------------------------------------

/// Replay engine rebuilds DB state from the JSONL journal.
/// Unlike Python's NetworkX projection, Rust replays directly into SQLite.
pub struct ReplayEngine {
    journal: EventJournal,
}

impl ReplayEngine {
    pub fn new(journal: EventJournal) -> Self {
        Self { journal }
    }

    /// Replay all events into a fresh in-memory DB.
    pub fn replay_full(&self, conn: &rusqlite::Connection) -> TdgResult<usize> {
        init_schema(conn)?;
        let events = self.journal.stream_after(None, None);
        let mut applied = 0;
        for entry in &events {
            if self.apply_entry(conn, entry)? {
                applied += 1;
            }
        }
        Ok(applied)
    }

    /// Replay events up to a timestamp.
    pub fn replay_up_to(&self, conn: &rusqlite::Connection, timestamp: &str) -> TdgResult<usize> {
        init_schema(conn)?;
        let events = self.journal.stream_up_to(timestamp);
        let mut applied = 0;
        for entry in &events {
            if self.apply_entry(conn, entry)? {
                applied += 1;
            }
        }
        Ok(applied)
    }

    /// Replay from a specific event ID (exclusive).
    pub fn replay_from(
        &self,
        conn: &rusqlite::Connection,
        after_event_id: &str,
    ) -> TdgResult<usize> {
        init_schema(conn)?;
        let events = self.journal.stream_after(Some(after_event_id), None);
        let mut applied = 0;
        for entry in &events {
            if self.apply_entry(conn, entry)? {
                applied += 1;
            }
        }
        Ok(applied)
    }

    /// Verify deterministic replay: run twice, compare node/edge counts.
    pub fn verify_determinism(&self) -> TdgResult<bool> {
        let mut results = Vec::new();
        for _ in 0..2 {
            let conn = rusqlite::Connection::open_in_memory()?;
            let count = self.replay_full(&conn)?;
            let node_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
            let edge_count: i64 = conn.query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))?;
            results.push((count, node_count, edge_count));
        }
        Ok(results[0] == results[1])
    }

    fn apply_entry(&self, conn: &rusqlite::Connection, entry: &JournalEntry) -> TdgResult<bool> {
        let payload = &entry.payload;
        match entry.event_action.as_str() {
            "node_created" => {
                if let Some(_node_id) = &entry.node_id {
                    let node_type = payload
                        .get("node_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("observation");
                    let name = payload
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("replayed");
                    let new_node = crate::models::NewNode {
                        node_type: node_type.to_string(),
                        name: name.to_string(),
                        description: payload
                            .get("description")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        properties: payload.get("properties").cloned(),
                        quadrants: payload.get("quadrants").cloned(),
                        drives: payload.get("drives").cloned(),
                        lifecycle_state: payload
                            .get("lifecycle_state")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        teleological_level: payload
                            .get("teleological_level")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        developmental_stage: payload
                            .get("developmental_stage")
                            .and_then(|v| v.as_i64())
                            .map(|v| v as i32),
                        confidence: payload.get("confidence").and_then(|v| v.as_f64()),
                        source: payload
                            .get("source")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        parent_ids: payload
                            .get("parent_ids")
                            .and_then(|v| serde_json::from_value(v.clone()).ok()),
                        agent_id: payload
                            .get("agent_id")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                    };
                    let _ = crud::add_node(conn, &new_node);
                }
                Ok(true)
            }
            "edge_created" => {
                if let (Some(src), Some(tgt)) = (&entry.source_id, &entry.target_id) {
                    let edge_type = payload
                        .get("edge_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("RELATES_TO");
                    let new_edge = crate::models::NewEdge {
                        source_id: src.clone(),
                        target_id: tgt.clone(),
                        edge_type: edge_type.to_string(),
                        weight: payload.get("weight").and_then(|v| v.as_f64()),
                        properties: payload.get("properties").cloned(),
                        agent_id: entry.agent_id.clone(),
                    };
                    let _ = crud::add_edge(conn, &new_edge);
                }
                Ok(true)
            }
            "node_deleted" | "node_archived" => {
                if let Some(nid) = &entry.node_id {
                    let _ = crud::delete_node(conn, nid);
                }
                Ok(true)
            }
            "edge_deleted" | "edge_archived" => {
                // For edge deletion, we need edge_id — skip if not present
                Ok(true)
            }
            "drive_recomputed" | "graph_renormalized" | "stage_advanced" | "node_promoted" => {
                // These are metadata events; just record them in the DB events table
                if let Some(nid) = &entry.node_id {
                    let _ = crud::record_event(
                        conn,
                        &entry.event_action,
                        Some(nid),
                        entry.source_id.as_deref(),
                        entry.target_id.as_deref(),
                        Some(&entry.payload),
                    );
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_journal(dir: &Path) -> EventJournal {
        EventJournal::new(dir.join("events.jsonl")).unwrap()
    }

    #[test]
    fn journal_append_and_count() {
        let tmp = TempDir::new().unwrap();
        let journal = temp_journal(tmp.path());
        assert_eq!(journal.count(), 0);

        let id = journal
            .append("node_created", Some("n001"), None, None, None, None)
            .unwrap();
        assert!(id.starts_with("evt:"));
        assert_eq!(journal.count(), 1);
    }

    #[test]
    fn journal_persistence_across_instances() {
        let tmp = TempDir::new().unwrap();
        {
            let journal = temp_journal(tmp.path());
            journal
                .append("node_created", Some("n001"), None, None, None, None)
                .unwrap();
            journal
                .append("edge_created", None, Some("n001"), Some("n002"), None, None)
                .unwrap();
        }
        // Reload from disk
        let journal2 = EventJournal::new(tmp.path().join("events.jsonl")).unwrap();
        assert_eq!(journal2.count(), 2);
    }

    #[test]
    fn journal_stream_after() {
        let tmp = TempDir::new().unwrap();
        let journal = temp_journal(tmp.path());
        let id1 = journal.append("a", None, None, None, None, None).unwrap();
        journal.append("b", None, None, None, None, None).unwrap();
        journal.append("c", None, None, None, None, None).unwrap();

        let after = journal.stream_after(Some(&id1), None);
        assert_eq!(after.len(), 2);
        assert_eq!(after[0].event_action, "b");
        assert_eq!(after[1].event_action, "c");
    }

    #[test]
    fn journal_backup_restore() {
        let tmp = TempDir::new().unwrap();
        let journal = temp_journal(tmp.path());
        journal.append("x", None, None, None, None, None).unwrap();

        let backup_path = tmp.path().join("backup.jsonl");
        journal.backup(&backup_path).unwrap();
        assert!(backup_path.exists());

        let journal2 = EventJournal::new(tmp.path().join("events2.jsonl")).unwrap();
        assert_eq!(journal2.count(), 0);
        journal2.restore(&backup_path).unwrap();
        assert_eq!(journal2.count(), 1);
    }

    #[test]
    fn snapshot_save_and_verify() {
        let tmp = TempDir::new().unwrap();
        let cfg = crate::config::Config {
            home: tmp.path().to_path_buf(),
            tdg_dir: tmp.path().join("tdg"),
            ..crate::config::Config::default()
        };
        let mgr = SnapshotManager::new(&cfg);
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::init_schema(&conn).unwrap();

        let path = mgr.save_snapshot(&conn, "test").unwrap();
        assert!(path.exists());
        assert!(mgr.verify_snapshot(&path).unwrap());
    }

    #[test]
    fn snapshot_list() {
        let tmp = TempDir::new().unwrap();
        let cfg = crate::config::Config {
            home: tmp.path().to_path_buf(),
            tdg_dir: tmp.path().join("tdg"),
            ..crate::config::Config::default()
        };
        let mgr = SnapshotManager::new(&cfg);
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::init_schema(&conn).unwrap();

        mgr.save_snapshot(&conn, "s1").unwrap();
        mgr.save_snapshot(&conn, "s2").unwrap();
        let snaps = mgr.list_snapshots().unwrap();
        assert_eq!(snaps.len(), 2);
    }

    #[test]
    fn replay_determinism() {
        let tmp = TempDir::new().unwrap();
        let journal = temp_journal(tmp.path());
        journal
            .append(
                "node_created",
                Some("n001"),
                None,
                None,
                None,
                Some(&serde_json::json!({"node_type": "observation", "name": "test"})),
            )
            .unwrap();
        journal
            .append(
                "node_created",
                Some("n002"),
                None,
                None,
                None,
                Some(&serde_json::json!({"node_type": "action", "name": "test2"})),
            )
            .unwrap();
        journal
            .append(
                "edge_created",
                None,
                Some("n001"),
                Some("n002"),
                None,
                Some(&serde_json::json!({"edge_type": "SUPPORTS"})),
            )
            .unwrap();

        let engine = ReplayEngine::new(journal);
        assert!(engine.verify_determinism().unwrap());
    }

    #[test]
    fn replay_full_applies_events() {
        let tmp = TempDir::new().unwrap();
        let journal = temp_journal(tmp.path());
        journal
            .append(
                "node_created",
                Some("n001"),
                None,
                None,
                None,
                Some(&serde_json::json!({"node_type": "observation", "name": "obs1"})),
            )
            .unwrap();

        let engine = ReplayEngine::new(journal);
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let applied = engine.replay_full(&conn).unwrap();
        assert_eq!(applied, 1);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
