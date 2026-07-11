use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rmcp::ErrorData as McpError;

use crate::db::pool::ConnectionPool;

#[derive(Debug, Clone)]
pub(crate) struct TrustEntry {
    pub score: f64,
    pub updated_at: String,
    pub source: Option<String>,
    pub reason: Option<String>,
}

pub(crate) struct TrustStore {
    entries: Mutex<HashMap<String, TrustEntry>>,
    pool: Arc<ConnectionPool>,
}

impl TrustStore {
    pub fn new(pool: Arc<ConnectionPool>) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            pool,
        }
    }

    // ponytail: infrastructure for future MCP trust tools, not yet wired
    #[allow(dead_code)]
    pub fn set_trust(
        &self,
        agent_name: &str,
        score: f64,
        reason: Option<String>,
    ) -> Result<(), McpError> {
        let score = score.clamp(0.0, 1.0);
        let now = crate::db::crud::now_iso();

        // Persist to DB using with_connection (panic-safe — always releases
        // the connection back to the pool, even if set_trust panics).
        // Previously used manual get_connection/release_connection which leaked
        // the connection on panic.
        self.pool
            .with_connection(|conn| {
                crate::db::crud::set_trust(conn, agent_name, score, reason.as_deref())
            })
            .map_err(|e| {
                McpError::internal_error(format!("Failed to persist trust: {}", e), None)
            })?;

        let mut entries = self
            .entries
            .lock()
            .map_err(|e| McpError::internal_error(format!("Lock poisoned: {}", e), None))?;
        entries.insert(
            agent_name.to_string(),
            TrustEntry {
                score,
                updated_at: now,
                source: None,
                reason,
            },
        );
        Ok(())
    }

    pub fn get_trust(&self, agent_name: &str) -> Result<Option<TrustEntry>, McpError> {
        {
            let entries = self
                .entries
                .lock()
                .map_err(|e| McpError::internal_error(format!("Lock poisoned: {}", e), None))?;
            if let Some(entry) = entries.get(agent_name) {
                return Ok(Some(entry.clone()));
            }
        }

        // Use with_connection for panic safety.
        let result = self
            .pool
            .with_connection(|conn| {
                let score = crate::db::crud::get_trust(conn, agent_name)?;
                let has_record = has_trust_record(conn, agent_name);
                Ok((score, has_record))
            })
            .map_err(|e| McpError::internal_error(format!("DB error: {}", e), None))?;

        let (score, has_record) = result;
        if (score - 0.5).abs() > f64::EPSILON || has_record {
            let entry = TrustEntry {
                score,
                updated_at: crate::db::crud::now_iso(),
                source: None,
                reason: None,
            };
            let mut entries = self
                .entries
                .lock()
                .map_err(|e| McpError::internal_error(format!("Lock poisoned: {}", e), None))?;
            entries.insert(agent_name.to_string(), entry.clone());
            return Ok(Some(entry));
        }

        Ok(None)
    }

    pub fn adjust_trust(
        &self,
        agent_name: &str,
        delta: f64,
        reason: Option<String>,
        source: Option<String>,
    ) -> Result<f64, McpError> {
        let current = self.get_trust(agent_name)?.map(|e| e.score).unwrap_or(0.5);
        let new_score = (current + delta).clamp(0.0, 1.0);
        let now = crate::db::crud::now_iso();

        // Persist using with_connection for panic safety.
        self.pool
            .with_connection(|conn| {
                crate::db::crud::set_trust(conn, agent_name, new_score, reason.as_deref())
            })
            .map_err(|e| {
                McpError::internal_error(format!("Failed to persist trust: {}", e), None)
            })?;

        let mut entries = self
            .entries
            .lock()
            .map_err(|e| McpError::internal_error(format!("Lock poisoned: {}", e), None))?;
        let entry = entries.entry(agent_name.to_string()).or_insert(TrustEntry {
            score: 0.5,
            updated_at: now.clone(),
            source: None,
            reason: None,
        });
        entry.score = new_score;
        entry.updated_at = now;
        if let Some(r) = reason {
            entry.reason = Some(r);
        }
        if let Some(s) = source {
            entry.source = Some(s);
        }
        Ok(entry.score)
    }
}

fn has_trust_record(conn: &rusqlite::Connection, agent_name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM trust_scores WHERE agent_id = ?1",
        rusqlite::params![agent_name],
        |row| row.get::<_, i64>(0),
    )
    .map(|c| c > 0)
    .unwrap_or(false)
}
