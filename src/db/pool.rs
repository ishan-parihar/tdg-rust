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
        conn.execute_batch(&format!("PRAGMA busy_timeout={};", self.busy_timeout))?;

        Ok(conn)
    }

    /// Get a connection from the pool, creating one if needed.
    /// Blocks if pool is full until a connection is returned.
    pub fn get_connection(&self) -> TdgResult<Connection> {
        let mut conns = self
            .connections
            .lock()
            .map_err(|e| TdgError::Custom(format!("Pool lock poisoned: {e}")))?;

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
            conns = self
                .condvar
                .wait(conns)
                .map_err(|e| TdgError::Custom(format!("Pool condvar poisoned: {e}")))?;
        }
    }

    /// Return a connection to the pool.
    pub fn release_connection(&self, conn: Connection) {
        if let Ok(mut conns) = self.connections.lock() {
            if conns.len() < self.max_connections {
                conns.push(conn);
                self.condvar.notify_one();
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
        conn.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY);")
            .unwrap();
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

    #[test]
    fn pool_wal_mode_enabled() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 2, 30000).unwrap();

        pool.with_connection(|conn| {
            let mode: String = conn
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert_eq!(mode, "wal");
            Ok(())
        })
        .unwrap();

        pool.close();
    }

    #[test]
    fn pool_foreign_keys_enabled() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 2, 30000).unwrap();

        pool.with_connection(|conn| {
            let fk: i32 = conn
                .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
                .unwrap();
            assert_eq!(fk, 1);
            Ok(())
        })
        .unwrap();

        pool.close();
    }

    #[test]
    fn pool_synchronous_normal() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 2, 30000).unwrap();

        pool.with_connection(|conn| {
            let sync_val: i32 = conn
                .query_row("PRAGMA synchronous", [], |r| r.get(0))
                .unwrap();
            assert!(sync_val >= 0 && sync_val <= 2);
            Ok(())
        })
        .unwrap();

        pool.close();
    }

    #[test]
    fn pool_reuses_connection() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 5, 30000).unwrap();

        let conn1 = pool.get_connection().unwrap();
        pool.release_connection(conn1);

        let conn2 = pool.get_connection().unwrap();
        conn2
            .execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY);")
            .unwrap();
        conn2.execute("INSERT INTO t (id) VALUES (1)", []).unwrap();
        pool.release_connection(conn2);

        let conn3 = pool.get_connection().unwrap();
        let count: i32 = conn3
            .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        pool.release_connection(conn3);

        pool.close();
    }

    #[test]
    fn pool_close_drains_connections() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 5, 30000).unwrap();

        let conn = pool.get_connection().unwrap();
        pool.release_connection(conn);

        pool.close();

        let conns = pool.connections.lock().unwrap();
        assert!(conns.is_empty());
    }

    #[test]
    fn pool_release_returns_connection() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 2, 30000).unwrap();

        let c1 = pool.get_connection().unwrap();
        pool.release_connection(c1);

        let conns = pool.connections.lock().unwrap();
        assert_eq!(conns.len(), 1);
        drop(conns);

        pool.close();
    }

    #[test]
    fn pool_with_connection_propagates_error() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 5, 30000).unwrap();

        let result: Result<(), _> =
            pool.with_connection(|_| Err(crate::error::TdgError::Custom("test error".to_string())));
        assert!(result.is_err());

        pool.close();
    }

    #[test]
    fn pool_multiple_connections_concurrent_reads() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 4, 30000).unwrap();

        pool.with_connection(|conn| {
            conn.execute_batch("CREATE TABLE data (id INTEGER PRIMARY KEY, val TEXT)")?;
            for i in 0..100 {
                conn.execute(
                    "INSERT INTO data (id, val) VALUES (?1, ?2)",
                    rusqlite::params![i, format!("val_{i}")],
                )?;
            }
            Ok(())
        })
        .unwrap();

        let mut handles = Vec::new();
        for _ in 0..4 {
            let conn = pool.get_connection().unwrap();
            handles.push(std::thread::spawn(move || {
                let count: i32 = conn
                    .query_row("SELECT COUNT(*) FROM data", [], |r| r.get(0))
                    .unwrap();
                assert_eq!(count, 100);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        pool.close();
    }

    #[test]
    fn pool_max_connections_respected() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 2, 30000).unwrap();

        let c1 = pool.get_connection().unwrap();
        let c2 = pool.get_connection().unwrap();

        let conns = pool.connections.lock().unwrap();
        let pool_size = conns.len();
        drop(conns);

        assert_eq!(pool_size, 0);

        pool.release_connection(c1);
        pool.release_connection(c2);
        pool.close();
    }

    #[test]
    fn pool_with_connection_executes_closure() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 5, 30000).unwrap();

        let result: i32 = pool
            .with_connection(|conn| {
                let val: i32 = conn.query_row("SELECT 42 + 1", [], |r| r.get(0))?;
                Ok(val)
            })
            .unwrap();
        assert_eq!(result, 43);

        pool.close();
    }

    #[test]
    fn pool_backup_creates_file() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 5, 30000).unwrap();

        pool.with_connection(|conn| {
            conn.execute_batch("CREATE TABLE test (id INTEGER)")?;
            conn.execute("INSERT INTO test (id) VALUES (1)", [])?;
            Ok(())
        })
        .unwrap();

        let backup_path = NamedTempFile::new().unwrap();
        pool.backup(backup_path.path()).unwrap();

        assert!(backup_path.path().metadata().unwrap().len() > 0);

        let backup_conn = Connection::open(backup_path.path()).unwrap();
        let count: i32 = backup_conn
            .query_row("SELECT COUNT(*) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        pool.close();
    }

    #[test]
    fn pool_busy_timeout_set() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 2, 5000).unwrap();

        pool.with_connection(|conn| {
            let timeout: i32 = conn
                .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
                .unwrap();
            assert_eq!(timeout, 5000);
            Ok(())
        })
        .unwrap();

        pool.close();
    }

    #[test]
    fn pool_cache_size_set() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let pool = ConnectionPool::new(path, 2, 30000).unwrap();

        pool.with_connection(|conn| {
            let cache: i32 = conn
                .query_row("PRAGMA cache_size", [], |r| r.get(0))
                .unwrap();
            assert_eq!(cache, -8000);
            Ok(())
        })
        .unwrap();

        pool.close();
    }
}
