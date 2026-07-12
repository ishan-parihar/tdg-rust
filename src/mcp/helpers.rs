//! MCP Tools — helper functions extracted from `tools.rs`.
//!
//! These are the non-tool functions (connection management, error conversion,
//! path validation, entity upsert, health scoring, blocking offload) that
//! support the `#[tool]`-annotated methods in `tools.rs`.

use std::collections::HashMap;
use std::sync::Arc;

use rmcp::ErrorData as McpError;

use crate::db::ConnectionPool;
use crate::models::NewEdge;

pub(crate) type DriveScores = HashMap<String, (Vec<f64>, Vec<f64>, Vec<f64>)>;

/// Calculate the overall graph health score from component metrics.
pub(crate) fn calculate_health_score(
    node_count: i64,
    edge_count: i64,
    type_count: i64,
    embedding_count: i64,
    fts_count: i64,
) -> f64 {
    let node_score = if node_count > 0 { 1.0 } else { 0.0 };
    let edge_score = (edge_count as f64 / node_count.max(1) as f64).min(1.0);
    let type_score = (type_count as f64 / 20.0).min(1.0);
    let embedding_score = if node_count > 0 {
        (embedding_count as f64 / node_count as f64).min(1.0)
    } else {
        1.0
    };
    let fts_score = if node_count > 0 {
        (fts_count as f64 / node_count as f64).min(1.0)
    } else {
        1.0
    };

    node_score * 0.35
        + edge_score * 0.20
        + type_score * 0.15
        + embedding_score * 0.20
        + fts_score * 0.10
}

/// RAII guard that borrows a connection from the pool and releases it on drop.
pub(crate) struct ConnGuard {
    pool: Arc<ConnectionPool>,
    conn: Option<rusqlite::Connection>,
}

struct UnsafeSyncConnection(rusqlite::Connection);
unsafe impl Sync for UnsafeSyncConnection {}
unsafe impl Send for UnsafeSyncConnection {}

impl std::ops::Deref for ConnGuard {
    type Target = rusqlite::Connection;
    fn deref(&self) -> &Self::Target {
        if let Some(ref conn) = self.conn {
            conn
        } else {
            tracing::warn!("ConnGuard conn already taken — this indicates a bug in the calling code. Returning fallback in-memory connection.");
            static FALLBACK_CONN: std::sync::OnceLock<UnsafeSyncConnection> = std::sync::OnceLock::new();
            let wrapper = FALLBACK_CONN.get_or_init(|| {
                let conn = rusqlite::Connection::open_in_memory()
                    .expect("Failed to open fallback in-memory sqlite connection");
                UnsafeSyncConnection(conn)
            });
            &wrapper.0
        }
    }
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.release_connection(conn);
        }
    }
}

/// Get a connection wrapped in an RAII guard.
pub(crate) fn get_conn(pool: &Arc<ConnectionPool>) -> Result<ConnGuard, McpError> {
    let conn = pool
        .get_connection()
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(ConnGuard {
        pool: Arc::clone(pool),
        conn: Some(conn),
    })
}

/// Convert any Display error into an McpError.
pub(crate) fn mcp_err(e: impl std::fmt::Display) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

/// Validate a file path for export/import operations.
/// Blocks access to system directories.
pub(crate) fn validate_file_path(
    path: &str,
    for_write: bool,
) -> Result<std::path::PathBuf, McpError> {
    use std::path::Path;

    if path.trim().is_empty() {
        return Err(McpError::invalid_params("path cannot be empty", None));
    }

    let p = Path::new(path);
    if p.is_absolute() {
        let canonical = p
            .canonicalize()
            .or_else(|_| {
                if for_write {
                    if let Some(parent) = p.parent() {
                        parent.canonicalize().map(|_| p.to_path_buf())
                    } else {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            "no parent",
                        ))
                    }
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "file not found",
                    ))
                }
            })
            .map_err(|e| mcp_err(anyhow::anyhow!("Cannot resolve path '{}': {}", path, e)))?;

        let path_str = canonical.to_string_lossy().to_lowercase();
        let blocked_prefixes = [
            "/etc/", "/var/", "/usr/", "/bin/", "/sbin/", "/root/", "/proc/", "/sys/", "/dev/",
            "/boot/", "/lib/",
        ];
        for prefix in &blocked_prefixes {
            if path_str.starts_with(prefix) {
                return Err(McpError::invalid_params(
                    format!(
                        "Access denied: path '{}' is in a protected system directory",
                        path
                    ),
                    None,
                ));
            }
        }
        Ok(canonical)
    } else {
        Ok(Path::new(path).to_path_buf())
    }
}

/// Upsert an entity node and create a MENTIONS edge from the observation.
pub(crate) fn upsert_entity_and_connect(
    conn: &rusqlite::Connection,
    observation_id: &str,
    entity_name: &str,
    entity_type: &str,
) -> Result<String, McpError> {
    let name = entity_name.trim().to_string();
    if name.is_empty() {
        return Err(McpError::invalid_params(
            "entity name cannot be empty",
            None,
        ));
    }

    let existing = crate::db::crud::search(conn, &name, 1)
        .unwrap_or_default()
        .into_iter()
        .find(|(n, _)| n.node_type == entity_type && n.name == name)
        .map(|(n, _)| n);

    let entity_node = if let Some(n) = existing {
        n
    } else {
        crate::db::crud::add_node(
            conn,
            &crate::models::NewNode {
                node_type: entity_type.to_string(),
                name: name.clone(),
                source: Some("mcp_observe".to_string()),
                ..Default::default()
            },
        )
        .map_err(mcp_err)?
    };

    let existing_edges = crate::db::crud::get_edges(
        conn,
        Some(observation_id),
        Some(&entity_node.id),
        Some("MENTIONS"),
        None,
        1,
    )
    .unwrap_or_default();

    if existing_edges.is_empty() {
        if let Err(e) = crate::db::crud::add_edge(
            conn,
            &NewEdge {
                source_id: observation_id.to_string(),
                target_id: entity_node.id.clone(),
                edge_type: "MENTIONS".to_string(),
                weight: Some(1.0),
                properties: None,
                agent_id: Some("mcp_observe".to_string()),
            },
        ) {
            tracing::warn!(
                "Failed to create MENTIONS edge {} -> {}: {}",
                observation_id,
                entity_node.id,
                e
            );
        }
    }

    Ok(entity_node.id)
}

/// Offload blocking SQLite I/O to a dedicated thread pool.
pub(crate) async fn run_blocking<F, T>(f: F) -> Result<T, McpError>
where
    F: FnOnce() -> Result<T, McpError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| McpError::internal_error(format!("task join error: {e}"), None))?
}
