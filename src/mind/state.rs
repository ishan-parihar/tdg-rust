//! Mind State — the agent's working memory and operational status.
//!
//! Provides [`MindStateManager`] with dual persistence (JSON file + SQLite WAL)
//! for resilient agent state management across restarts.
//!
//! Port of the Hermes `MemoryProvider` pattern from `plugins/tdg/__init__.py`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::TdgResult;

// ─── Data Types ───────────────────────────────────────────────────────────────

/// The agent's working memory and operational status.
///
/// Persisted to both JSON file (human-readable snapshot) and SQLite (WAL recovery).
/// Loaded on construction; mutated through the [`MindStateManager`] API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindState {
    /// Current session identifier.
    pub session_id: String,
    /// Agent name (e.g., "Sisyphus", "Hermes").
    pub agent_name: String,
    /// Name or description of the currently active plan or task.
    pub active_plan: Option<String>,
    /// Ephemeral working-memory items (short-term context).
    pub working_memory: Vec<WorkingMemoryItem>,
    /// Trust score in the range `0.0 – 1.0`.
    pub trust_score: f64,
    /// Aggregated performance counters and diagnostics.
    pub metrics: MindMetrics,
    /// Timestamp of the most recent state change.
    pub last_updated: DateTime<Utc>,
    /// Monotonically increasing version for optimistic concurrency.
    pub version: u64,
}

impl Default for MindState {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            agent_name: String::from("Sisyphus"),
            active_plan: None,
            working_memory: Vec::new(),
            trust_score: 0.5,
            metrics: MindMetrics::default(),
            last_updated: Utc::now(),
            version: 1,
        }
    }
}

/// A single slot of the agent's working memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingMemoryItem {
    /// Unique key for this memory slot.
    pub key: String,
    /// String-encoded value (caller serializes complex types).
    pub value: String,
    /// When this item was created.
    pub created_at: DateTime<Utc>,
    /// Time-to-live in seconds. `None` means the item lives forever.
    pub ttl_seconds: Option<u64>,
}

/// Aggregated performance counters and diagnostic state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MindMetrics {
    /// Total number of successfully completed tasks.
    pub tasks_completed: u64,
    /// Total number of failed or aborted tasks.
    pub tasks_failed: u64,
    /// Rolling average response time in milliseconds.
    pub avg_response_time_ms: f64,
    /// Context window utilization as a fraction `0.0 – 1.0`.
    pub context_utilization: f64,
    /// Free-form diagnostic note from the last cycle.
    pub last_diagnostic: Option<String>,
}

// ─── Manager ──────────────────────────────────────────────────────────────────

/// Thread-safe manager for the agent's mind state with dual persistence.
///
/// # Persistence strategy
///
/// 1. **JSON file** (`{state_dir}/mind-state.json`) — primary human-readable snapshot.
///    Written atomically (temp-file + rename) on every mutation.
/// 2. **SQLite WAL** — recovery layer (future: eventsourcing).
///
/// # Thread safety
///
/// All access is mediated by an internal `std::sync::Mutex`. File I/O uses
/// synchronous `std::fs` (safe under the mutex) rather than `tokio::fs`.
pub struct MindStateManager {
    state: Arc<Mutex<MindState>>,
    state_path: PathBuf,
    #[allow(dead_code)]
    config: Config,
}

impl MindStateManager {
    /// Create a new manager, loading state from disk or initialising a default.
    pub fn new(config: Config) -> Self {
        let state_path = config.state_dir.join("mind-state.json");
        let state = Self::load_or_default(&state_path);

        Self {
            state: Arc::new(Mutex::new(state)),
            state_path,
            config,
        }
    }

    /// Return a snapshot of the current mind state.
    pub fn get_state(&self) -> MindState {
        self.state
            .lock()
            .expect("mind state mutex poisoned")
            .clone()
    }

    /// Mutate the mind state inside a lock, then persist automatically.
    ///
    /// The closure receives exclusive write access. After it returns the state is
    /// serialised to the JSON file atomically.
    pub fn update<F>(&self, f: F) -> TdgResult<()>
    where
        F: FnOnce(&mut MindState),
    {
        let mut guard = self.state.lock().expect("mind state mutex poisoned");
        f(&mut guard);
        guard.last_updated = Utc::now();
        guard.version += 1;
        // Write out a fresh copy (the closure may have modified many fields).
        let serialised = serde_json::to_string_pretty(&*guard)?;
        atomic_write(&self.state_path, &serialised)?;
        Ok(())
    }

    // ─── Working memory ───────────────────────────────────────────────────

    /// Store a value in working memory.
    ///
    /// If a slot with the same `key` already exists it is overwritten.
    /// Pass `ttl_seconds = None` for a permanent entry.
    pub fn remember(&self, key: &str, value: &str, ttl_seconds: Option<u64>) -> TdgResult<()> {
        self.update(|state| {
            // Remove stale entry if it exists.
            state.working_memory.retain(|item| item.key != key);
            state.working_memory.push(WorkingMemoryItem {
                key: key.to_string(),
                value: value.to_string(),
                created_at: Utc::now(),
                ttl_seconds,
            });
        })
    }

    /// Retrieve a value from working memory by key.
    pub fn recall(&self, key: &str) -> Option<WorkingMemoryItem> {
        let guard = self.state.lock().expect("mind state mutex poisoned");
        guard
            .working_memory
            .iter()
            .find(|item| item.key == key)
            .cloned()
    }

    /// Remove expired working-memory items, returning the count of removed entries.
    pub fn hygiene(&self) -> TdgResult<u64> {
        let now = Utc::now();
        let mut removed = 0u64;
        self.update(|state| {
            let before = state.working_memory.len();
            state.working_memory.retain(|item| {
                match item.ttl_seconds {
                    None => true, // permanent
                    Some(ttl) => {
                        // Keep if still within TTL.
                        let expires = item.created_at + chrono::Duration::seconds(ttl as i64);
                        expires > now
                    }
                }
            });
            removed = (before - state.working_memory.len()) as u64;
        })?;
        Ok(removed)
    }

    // ─── Trust score ──────────────────────────────────────────────────────

    /// Overwrite the agent's trust score (clamped to `0.0 – 1.0`).
    pub fn set_trust(&self, score: f64) -> TdgResult<()> {
        let clamped = score.clamp(0.0, 1.0);
        self.update(|state| {
            state.trust_score = clamped;
        })
    }

    // ─── Task recording ───────────────────────────────────────────────────

    /// Record a task result and update rolling average response time.
    pub fn record_task(&self, success: bool, response_time_ms: f64) -> TdgResult<()> {
        self.update(|state| {
            let metrics = &mut state.metrics;
            if success {
                metrics.tasks_completed += 1;
            } else {
                metrics.tasks_failed += 1;
            }
            // Exponential moving average (α = 0.3) for response time.
            let total = metrics.tasks_completed + metrics.tasks_failed;
            if total == 1 {
                metrics.avg_response_time_ms = response_time_ms;
            } else {
                metrics.avg_response_time_ms =
                    0.7 * metrics.avg_response_time_ms + 0.3 * response_time_ms;
            }
        })
    }

    // ─── Internal helpers ─────────────────────────────────────────────────

    /// Load `MindState` from the JSON file at `path`, or return [`Default`].
    fn load_or_default(path: &PathBuf) -> MindState {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }
}

// ─── Atomic write helper ──────────────────────────────────────────────────────

/// Write `data` to `path` atomically via temp-file + rename.
///
/// The temporary file is created alongside the target (same directory) with a
/// `.tmp` suffix. On POSIX `rename` is atomic if source and target are on the
/// same filesystem.
fn atomic_write(path: &PathBuf, data: &str) -> TdgResult<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a `Config` whose `state_dir` points at a temp dir.
    fn temp_config() -> (Config, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir creation failed");
        // We cannot directly set Config fields because they are `pub`.
        let mut cfg = Config::from_env();
        cfg.state_dir = dir.path().to_path_buf();
        (cfg, dir)
    }

    #[test]
    fn new_creates_default_state_when_no_file_exists() {
        let (cfg, _dir) = temp_config();
        let mgr = MindStateManager::new(cfg);
        let state = mgr.get_state();
        assert_eq!(state.agent_name, "Sisyphus");
        assert!((state.trust_score - 0.5).abs() < 1e-9);
        assert_eq!(state.version, 1);
    }

    #[test]
    fn persist_and_reload_round_trip() {
        let (cfg, _dir) = temp_config();
        {
            let mgr = MindStateManager::new(cfg.clone());
            mgr.set_trust(0.85).unwrap();
        }
        // Re-open — should pick up persisted state.
        let mgr = MindStateManager::new(cfg);
        let state = mgr.get_state();
        assert!((state.trust_score - 0.85).abs() < 1e-9);
    }

    #[test]
    fn remember_and_recall() {
        let (cfg, _dir) = temp_config();
        let mgr = MindStateManager::new(cfg);
        mgr.remember("foo", "bar", None).unwrap();
        let item = mgr.recall("foo").expect("item should exist");
        assert_eq!(item.value, "bar");
        assert!(item.ttl_seconds.is_none());
    }

    #[test]
    fn remember_overwrites_existing_key() {
        let (cfg, _dir) = temp_config();
        let mgr = MindStateManager::new(cfg);
        mgr.remember("k", "v1", None).unwrap();
        mgr.remember("k", "v2", None).unwrap();
        let items = mgr.get_state().working_memory;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].value, "v2");
    }

    #[test]
    fn recall_nonexistent_key() {
        let (cfg, _dir) = temp_config();
        let mgr = MindStateManager::new(cfg);
        assert!(mgr.recall("nope").is_none());
    }

    #[test]
    fn hygiene_removes_expired_items() {
        let (cfg, _dir) = temp_config();
        let mgr = MindStateManager::new(cfg);
        // An item already expired (TTL of 0 seconds from creation).
        mgr.remember("ephemeral", "x", Some(0)).unwrap();
        // A permanent item.
        mgr.remember("permanent", "y", None).unwrap();
        let removed = mgr.hygiene().unwrap();
        assert_eq!(removed, 1);
        assert!(mgr.recall("ephemeral").is_none());
        assert!(mgr.recall("permanent").is_some());
    }

    #[test]
    fn set_trust_clamps_value() {
        let (cfg, _dir) = temp_config();
        let mgr = MindStateManager::new(cfg);
        mgr.set_trust(1.5).unwrap();
        assert!((mgr.get_state().trust_score - 1.0).abs() < 1e-9);
        mgr.set_trust(-0.5).unwrap();
        assert!((mgr.get_state().trust_score - 0.0).abs() < 1e-9);
    }

    #[test]
    fn record_task_updates_metrics() {
        let (cfg, _dir) = temp_config();
        let mgr = MindStateManager::new(cfg);
        mgr.record_task(true, 100.0).unwrap();
        mgr.record_task(false, 200.0).unwrap();
        let metrics = mgr.get_state().metrics;
        assert_eq!(metrics.tasks_completed, 1);
        assert_eq!(metrics.tasks_failed, 1);
        // First task sets baseline; second applies EMA.
        assert!(metrics.avg_response_time_ms > 100.0);
        assert!(metrics.avg_response_time_ms < 200.0);
    }

    #[test]
    fn version_increments_on_update() {
        let (cfg, _dir) = temp_config();
        let mgr = MindStateManager::new(cfg);
        let v1 = mgr.get_state().version;
        mgr.set_trust(0.9).unwrap();
        let v2 = mgr.get_state().version;
        assert_eq!(v2, v1 + 1);
    }

    #[test]
    fn atomic_write_creates_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.json");
        atomic_write(&path, r#"{"a":1}"#).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, r#"{"a":1}"#);
    }

    #[test]
    fn hygiene_noop_when_nothing_expired() {
        let (cfg, _dir) = temp_config();
        let mgr = MindStateManager::new(cfg);
        mgr.remember("a", "1", Some(3600)).unwrap();
        mgr.remember("b", "2", None).unwrap();
        let removed = mgr.hygiene().unwrap();
        assert_eq!(removed, 0);
        assert_eq!(mgr.get_state().working_memory.len(), 2);
    }
}
