use std::path::Path;
use std::sync::{Condvar, Mutex};

use rusqlite::Connection;

use crate::error::{TdgError, TdgResult};

/// Thread-safe SQLite connection pool with WAL mode.
///
/// Mirrors the Python `ConnectionPool` from `core/graph_db.py`.
pub struct ConnectionPool {
    connections: Mutex<Vec<Connection>>,
    condvar: Condvar,
    db_path: String,
    max_connections: usize,
    busy_timeout: i32,
}

impl ConnectionPool {
    /// Create a new pool. Connections are created on demand.
    pub fn new(db_path: &str, max_connections: usize, busy_timeout: i32) -> TdgResult<Self> {
        // Ensure parent directory exists
        if let Some(parent) = Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(Self {
            connections: Mutex::new(Vec::with_capacity(max_connections)),
            condvar: Condvar::new(),
            db_path: db_path.to_string(),
            max_connections,
            busy_timeout,
        })
    }

    /// Create a new connection with PRAGMA settings.
    fn make_connection(&self) -> TdgResult<Connection> {
        let conn = Connection::open(&self.db_path)?;

        // WAL mode for concurrent read/write
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        // Normal synchronous for good performance
        conn.execute_batch("PRAGMA synchronous=NORMAL;")?;
        // Enable foreign keys
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        // 8MB cache
        conn.execute_batch("PRAGMA cache_size=-8000;")?;
        // Busy timeout
        conn.execute_batch(&format!(
            "PRAGMA busy_timeout={};",
            self.busy_timeout
        ))?;

        Ok(conn)
    }

    /// Get a connection from the pool, creating one if needed.
    /// Blocks if pool is full until a connection is returned.
    pub fn get_connection(&self) -> TdgResult<Connection> {
        let mut conns = self.connections.lock().map_err(|e| {
            TdgError::Custom(format!("Pool lock poisoned: {e}"))
        })?;

        loop {
            if let Some(conn) = conns.pop() {
                // Test if connection is still alive
                if conn.execute_batch("SELECT 1;").is_ok() {
                    return Ok(conn);
                }
                // Connection is dead, try to create a new one
                if conns.len() < self.max_connections {
                    drop(conns);
                    return self.make_connection();
                }
            } else if conns.len() < self.max_connections {
                drop(conns);
                return self.make_connection();
            }

            // Pool is full and all connections are in use — wait
            conns = self.condvar.wait(conns).map_err(|e| {
                TdgError::Custom(format!("Pool condvar poisoned: {e}"))
            })?;
        }
    }

    /// Return a connection to the pool.
    pub fn release_connection(&self, conn: Connection) {
        if let Ok(mut conns) = self.connections.lock() {
            if conns.len() < self.max_connections {
                conns.push(conn);
                self.condvar.notify_one();
                return;
            }
        }
        // Connection dropped (closed) if pool is full
    }

    /// Close all connections in the pool.
    pub fn close(&self) {
        if let Ok(mut conns) = self.connections.lock() {
            for conn in conns.drain(..) {
                let _ = conn.close();
            }
        }
    }

    /// Borrow a connection, execute a closure, and return it to the pool.
    pub fn with_connection<F, R>(&self, f: F) -> TdgResult<R>
    where
        F: FnOnce(&Connection) -> TdgResult<R>,
    {
        let conn = self.get_connection()?;
        let result = f(&conn);
        self.release_connection(conn);
        result
    }

    /// Backup the database to a file using SQLite's online backup API.
    pub fn backup(&self, backup_path: &Path) -> TdgResult<()> {
        let conn = self.get_connection()?;
        crate::db::schema::backup_database(&conn, backup_path)?;
        self.release_connection(conn);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn pool_create_and_borrow() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 5, 30000).unwrap();

        let conn = pool.get_connection().unwrap();
        conn.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY);").unwrap();
        pool.release_connection(conn);

        pool.close();
    }

    #[test]
    fn pool_with_connection() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 5, 30000).unwrap();

        pool.with_connection(|conn| {
            conn.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY);")?;
            conn.execute("INSERT INTO test (id) VALUES (1)", [])?;
            let count: i32 = conn.query_row("SELECT COUNT(*) FROM test", [], |r| r.get(0))?;
            assert_eq!(count, 1);
            Ok(())
        })
        .unwrap();

        pool.close();
    }
}
