use std::fmt;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::{Duration, Instant};

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
                Ok(()) => return Ok(Self { _lock_file: lock_file }),
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
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockError::Timeout => write!(f, "write lock timeout"),
            LockError::Io(e) => write!(f, "write lock I/O error: {e}"),
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
}
