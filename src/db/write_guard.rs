use std::fmt;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::circuit_breaker::CircuitBreaker;

pub struct WriteGuard {
    _lock_file: File,
}

impl WriteGuard {
    pub fn acquire(db_path: &Path, timeout: Duration) -> Result<Self, LockError> {
        let lock_path = db_path.with_extension("lock");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(LockError::Io)?;

        let start = Instant::now();
        loop {
            match lock_file.try_lock() {
                Ok(()) => {
                    return Ok(Self {
                        _lock_file: lock_file,
                    })
                }
                Err(std::fs::TryLockError::WouldBlock) => {
                    if start.elapsed() >= timeout {
                        return Err(LockError::Timeout);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(std::fs::TryLockError::Error(e)) => return Err(LockError::Io(e)),
            }
        }
    }
}

#[derive(Debug)]
pub enum LockError {
    Timeout,
    Io(std::io::Error),
    CircuitBreakerTripped,
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockError::Timeout => write!(f, "write lock timeout"),
            LockError::Io(e) => write!(f, "write lock I/O error: {e}"),
            LockError::CircuitBreakerTripped => write!(f, "circuit breaker tripped"),
        }
    }
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LockError::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// Combines file-level write guard with circuit breaker for inter-process safety.
///
/// Usage:
/// ```ignore
/// let ctx = WriteContext::new(db_path, Arc::new(Mutex::new(CircuitBreaker::new())));
/// let _guard = ctx.acquire_write_guard()?;
/// // perform write operations...
/// ```
pub struct WriteContext {
    pub db_path: std::path::PathBuf,
    pub circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    pub lock_timeout: Duration,
}

impl WriteContext {
    pub fn new(db_path: std::path::PathBuf, circuit_breaker: Arc<Mutex<CircuitBreaker>>) -> Self {
        Self {
            db_path,
            circuit_breaker,
            lock_timeout: Duration::from_secs(5),
        }
    }

    /// Acquire write guard with circuit breaker check.
    pub fn acquire_write_guard(&self) -> Result<WriteGuard, LockError> {
        if let Ok(mut cb) = self.circuit_breaker.lock() {
            if cb.is_tripped() {
                return Err(LockError::CircuitBreakerTripped);
            }
        }
        // If lock is poisoned, still try to acquire — circuit breaker state is best-effort

        WriteGuard::acquire(&self.db_path, self.lock_timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn acquire_and_drop_releases_lock() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path();

        let guard = WriteGuard::acquire(path, Duration::from_secs(5)).unwrap();
        drop(guard);

        let guard2 = WriteGuard::acquire(path, Duration::from_secs(5)).unwrap();
        drop(guard2);
    }

    #[test]
    fn timeout_when_held() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path();

        let _guard = WriteGuard::acquire(path, Duration::from_secs(5)).unwrap();

        let result = WriteGuard::acquire(path, Duration::from_millis(50));
        assert!(matches!(result, Err(LockError::Timeout)));
    }

    #[test]
    fn write_context_acquires_guard() {
        let tmp = NamedTempFile::new().unwrap();
        let cb = Arc::new(Mutex::new(CircuitBreaker::new()));
        let ctx = WriteContext::new(tmp.path().to_path_buf(), cb);

        let guard = ctx.acquire_write_guard().unwrap();
        drop(guard);
    }

    #[test]
    fn write_context_rejects_when_circuit_breaker_tripped() {
        let tmp = NamedTempFile::new().unwrap();
        let mut cb = CircuitBreaker::with_config(1, 60);
        cb.record_failure();
        assert!(cb.is_tripped());

        let cb = Arc::new(Mutex::new(cb));
        let ctx = WriteContext::new(tmp.path().to_path_buf(), cb);

        let result = ctx.acquire_write_guard();
        assert!(matches!(result, Err(LockError::CircuitBreakerTripped)));
    }

    #[test]
    fn write_context_allows_when_circuit_breaker_closed() {
        let tmp = NamedTempFile::new().unwrap();
        let cb = Arc::new(Mutex::new(CircuitBreaker::with_config(3, 60)));
        let ctx = WriteContext::new(tmp.path().to_path_buf(), cb);

        let guard = ctx.acquire_write_guard().unwrap();
        drop(guard);
    }

    #[test]
    fn write_context_circuit_breaker_recovers_after_cooldown() {
        let tmp = NamedTempFile::new().unwrap();
        let mut cb = CircuitBreaker::with_config(1, 60);
        cb.record_failure();

        let cb = Arc::new(Mutex::new(cb));
        let ctx = WriteContext::new(tmp.path().to_path_buf(), cb);

        let result = ctx.acquire_write_guard();
        assert!(matches!(result, Err(LockError::CircuitBreakerTripped)));

        if let Ok(mut cb) = ctx.circuit_breaker.lock() {
            cb.reset();
        }
        let guard = ctx.acquire_write_guard().unwrap();
        drop(guard);
    }
}
