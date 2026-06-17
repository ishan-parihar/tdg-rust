
/// Core error type for the TDG system.
#[derive(Debug, thiserror::Error)]
pub enum TdgError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Circuit breaker tripped after {threshold} consecutive failures")]
    CircuitBreakerTripped {
        threshold: usize,
    },

    #[error("Graph size limit exceeded: {0}")]
    GraphSizeLimit(String),

    #[error("Busy timeout: database is locked")]
    BusyTimeout,

    #[error("Schema migration error: {0}")]
    SchemaMigration(String),

    #[error("HRR error: {0}")]
    Hrr(String),

    #[error("Ollama error: {0}")]
    Ollama(String),

    #[error("File lock error: {0}")]
    FileLock(String),

    #[error("{0}")]
    Custom(String),
}


/// Result type alias for TDG operations.
pub type TdgResult<T> = Result<T, TdgError>;
