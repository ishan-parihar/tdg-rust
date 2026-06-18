//! # Database Layer
//!
//! SQLite-backed persistence for the TDG graph. Provides connection pooling,
//! CRUD operations, schema management, and event tracking.
//!
//! ## Submodules
//!
//! - [`pool`] — Thread-safe SQLite connection pool with WAL mode and busy
//!   timeout handling. Mirrors the Python `ConnectionPool` from `core/graph_db.py`.
//! - [`crud`] — Node and edge CRUD operations: create, read, update, delete,
//!   query with filters, and bulk operations.
//! - [`schema`] — Schema initialization, FTS5 full-text search setup, and
//!   incremental migrations.
//! - [`events`] — Event tracking: trust score computation, node rating,
//!   retrieval recording, and helper/unhelpful feedback.
//! - [`write_guard`] — Inter-process file locking for safe concurrent writes.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use tdg_rust::db::{ConnectionPool, init_schema, init_fts, run_migrations};
//!
//! let pool = ConnectionPool::new("~/.hermes/tdg/graph.db", 4, 5000).unwrap();
//! pool.with_connection(|conn| {
//!     init_schema(conn)?;
//!     init_fts(conn)?;
//!     run_migrations(conn)?;
//!     Ok(())
//! }).unwrap();
//! ```
//!
//! ## Re-exports
//!
//! - [`ConnectionPool`] — The primary connection manager
//! - [`init_schema`] — Create all tables and indexes
//! - [`init_fts`] — Create FTS5 virtual tables
//! - [`run_migrations`] — Apply incremental schema migrations

pub mod crud;
pub mod events;
pub mod pool;
pub mod schema;
pub mod write_guard;

pub use pool::ConnectionPool;
pub use schema::{init_fts, init_schema, run_migrations};
