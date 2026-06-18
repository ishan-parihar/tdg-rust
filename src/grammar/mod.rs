//! # Node Grammar
//!
//! Defines how catalysts (input signals) map to node blueprints and how
//! edges are auto-wired when nodes are created. Ported from the Python TDG
//! modules `tdg_node_validation.py`, `auto_wire.py`, and `tdg_node_grammar.py`.
//!
//! ## Submodules
//!
//! - [`auto_wire`] — Automatically creates edges between a parent and child
//!   node based on `NODE_CONTRACT` `auto_wire_on_parent` rules. Handles
//!   direction mapping (parent→child vs child→parent) for each edge type.
//! - [`node_grammar`] — Maps [`CatalystType`]
//!   values to [`NodeBlueprint`] definitions that specify node type, telos
//!   level, title template, quadrant, and validation requirements.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tdg_rust::grammar::auto_wire_edges;
//! use tdg_rust::schema::CatalystType;
//! # use tdg_rust::db::ConnectionPool;
//!
//! # let pool = ConnectionPool::new(":memory:", 1, 5000).unwrap();
//! # pool.with_connection(|conn| {
//! #     tdg_rust::init_schema(conn)?;
//! #     tdg_rust::run_migrations(conn)?;
//! // Auto-wire edges when creating a node
//! # Ok(())
//! # }).unwrap();
//! ```

pub mod auto_wire;
pub mod node_grammar;

pub use crate::schema::CatalystType;
pub use auto_wire::auto_wire_edges;
pub use node_grammar::{NodeBlueprint, NodeGrammar};
