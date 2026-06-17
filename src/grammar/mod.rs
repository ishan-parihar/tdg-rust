//! Grammar modules: node auto-wiring and catalyst→node blueprint mapping.
//!
//! Ported from Python `tdg_node_validation.py`, `auto_wire.py`, `tdg_node_grammar.py`.

pub mod auto_wire;
pub mod node_grammar;

pub use auto_wire::auto_wire_edges;
pub use node_grammar::{NodeBlueprint, NodeGrammar};
pub use crate::schema::CatalystType;
