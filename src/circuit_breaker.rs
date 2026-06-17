//! Circuit breaker: state machine with threshold/cooldown for graph safety.
//!
//! Ported from Python `circuit_breaker.py` (285 lines).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::error::TdgResult;
use crate::models::{NewNode, Node};

/// Circuit breaker states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation. Failures counted but requests allowed.
    Closed,
    /// Too many failures. Requests blocked.
    Open,
    /// Testing if service recovered. One request allowed.
    HalfOpen,
}

/// Circuit breaker with threshold and cooldown.
///
/// Transitions:
/// - CLOSED → OPEN: when failure count >= threshold
/// - OPEN → HALF_OPEN: after cooldown period
/// - HALF_OPEN → CLOSED: on success
/// - HALF_OPEN → OPEN: on failure
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    threshold: u32,
    cooldown: Duration,
    last_failure: Option<Instant>,
    success_in_half_open: bool,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with default settings (threshold=3, cooldown=30s).
    pub fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            threshold: 3,
            cooldown: Duration::from_secs(30),
            last_failure: None,
            success_in_half_open: false,
        }
    }

    /// Create with custom threshold and cooldown.
    pub fn with_config(threshold: u32, cooldown_secs: u64) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            threshold,
            cooldown: Duration::from_secs(cooldown_secs),
            last_failure: None,
            success_in_half_open: false,
        }
    }

    /// Check if the circuit breaker is tripped (requests blocked).
    pub fn is_tripped(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => false,
            CircuitState::Open => {
                // Check if cooldown has elapsed
                if let Some(last) = self.last_failure {
                    if last.elapsed() >= self.cooldown {
                        self.state = CircuitState::HalfOpen;
                        self.success_in_half_open = false;
                        return false;
                    }
                }
                true
            }
            CircuitState::HalfOpen => false, // Allow one request
        }
    }

    /// Record a failure. May trip the circuit breaker.
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(Instant::now());

        match self.state {
            CircuitState::Closed => {
                if self.failure_count >= self.threshold {
                    self.state = CircuitState::Open;
                }
            }
            CircuitState::HalfOpen => {
                // Failure in half-open → back to open
                self.state = CircuitState::Open;
            }
            CircuitState::Open => {}
        }
    }

    /// Record a success. May reset the circuit breaker.
    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                // Success in half-open → back to closed
                self.state = CircuitState::Closed;
                self.failure_count = 0;
                self.success_in_half_open = true;
            }
            CircuitState::Open => {}
        }
    }

    /// Manually reset the circuit breaker to closed state.
    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.last_failure = None;
        self.success_in_half_open = false;
    }

    /// Get current state.
    pub fn state(&self) -> CircuitState {
        self.state
    }

    /// Get current failure count.
    pub fn failure_count(&self) -> u32 {
        self.failure_count
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

/// Pre-write snapshot for transaction rollback.
///
/// Captures the state of nodes and edges before a mutation operation,
/// allowing rollback if the operation fails.
#[derive(Debug, Clone)]
pub struct PreWriteSnapshot {
    pub nodes_before: Vec<Node>,
    pub edges_before: Vec<crate::models::Edge>,
    pub nodes_created: Vec<String>,
    pub edges_created: Vec<String>,
}

impl PreWriteSnapshot {
    /// Create a new empty snapshot.
    pub fn new() -> Self {
        Self {
            nodes_before: Vec::new(),
            edges_before: Vec::new(),
            nodes_created: Vec::new(),
            edges_created: Vec::new(),
        }
    }

    /// Capture current state of specified nodes and their edges.
    pub fn capture(conn: &Connection, node_ids: &[String]) -> TdgResult<Self> {
        let mut snapshot = Self::new();

        for node_id in node_ids {
            if let Some(node) = crate::db::crud::get_node(conn, node_id)? {
                snapshot.nodes_before.push(node);
            }

            let edges = crate::db::crud::get_edges(conn, Some(node_id), None, None, None, 1000)?;
            snapshot.edges_before.extend(edges);
        }

        Ok(snapshot)
    }

    /// Restore the snapshot by deleting created nodes/edges and restoring original state.
    ///
    /// NOTE: This is a simplified rollback. In production, you'd use SQLite transactions.
    pub fn restore(&self, conn: &Connection) -> TdgResult<()> {
        // Delete created edges
        for edge_id in &self.edges_created {
            crate::db::crud::delete_edge(conn, edge_id)?;
        }

        // Delete created nodes
        for node_id in &self.nodes_created {
            crate::db::crud::hard_delete_node(conn, node_id)?;
        }

        // Re-insert original nodes (simplified - in production use transaction rollback)
        for node in &self.nodes_before {
            let new_node = NewNode {
                node_type: node.node_type.clone(),
                name: node.name.clone(),
                description: Some(node.description.clone()),
                properties: Some(node.properties.clone()),
                quadrants: Some(node.quadrants.clone()),
                drives: Some(node.drives.clone()),
                lifecycle_state: Some(node.lifecycle_state.clone()),
                teleological_level: node.teleological_level.clone(),
                developmental_stage: node.developmental_stage,
                confidence: Some(node.confidence),
                source: Some(node.source.clone()),
                parent_ids: Some(node.parent_ids.clone()),
                agent_id: node.agent_id.clone(),
            };
            crate::db::crud::add_node(conn, &new_node)?;
        }

        Ok(())
    }
}

impl Default for PreWriteSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// Transaction snapshot: rolling buffer of pre-write snapshots.
pub struct TransactionSnapshot {
    snapshots: VecDeque<PreWriteSnapshot>,
    max_size: usize,
}

impl TransactionSnapshot {
    /// Create with default max size (5).
    pub fn new() -> Self {
        Self {
            snapshots: VecDeque::new(),
            max_size: 5,
        }
    }

    /// Create with custom max size.
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            snapshots: VecDeque::new(),
            max_size,
        }
    }

    /// Push a snapshot. Drops oldest if at capacity.
    pub fn push(&mut self, snapshot: PreWriteSnapshot) {
        if self.snapshots.len() >= self.max_size {
            self.snapshots.pop_front();
        }
        self.snapshots.push_back(snapshot);
    }

    /// Pop the most recent snapshot.
    pub fn pop(&mut self) -> Option<PreWriteSnapshot> {
        self.snapshots.pop_back()
    }

    /// Get the number of stored snapshots.
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }
}

impl Default for TransactionSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_default_state() {
        let mut cb = CircuitBreaker::new();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(!cb.is_tripped());
    }

    #[test]
    fn test_circuit_breaker_trips_after_threshold() {
        let mut cb = CircuitBreaker::with_config(3, 60);

        cb.record_failure();
        assert!(!cb.is_tripped());

        cb.record_failure();
        assert!(!cb.is_tripped());

        cb.record_failure();
        assert!(cb.is_tripped());
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_success_resets() {
        let mut cb = CircuitBreaker::new();

        cb.record_failure();
        cb.record_failure();
        cb.record_success();

        assert_eq!(cb.failure_count(), 0);
        assert!(!cb.is_tripped());
    }

    #[test]
    fn test_circuit_breaker_manual_reset() {
        let mut cb = CircuitBreaker::with_config(1, 60);

        cb.record_failure();
        assert!(cb.is_tripped());

        cb.reset();
        assert!(!cb.is_tripped());
        assert_eq!(cb.failure_count(), 0);
    }

    #[test]
    fn test_pre_write_snapshot_default() {
        let snapshot = PreWriteSnapshot::new();
        assert!(snapshot.nodes_before.is_empty());
        assert!(snapshot.edges_before.is_empty());
        assert!(snapshot.nodes_created.is_empty());
        assert!(snapshot.edges_created.is_empty());
    }

    #[test]
    fn test_transaction_snapshot_buffer() {
        let mut ts = TransactionSnapshot::with_max_size(3);

        for i in 0..5 {
            let mut snapshot = PreWriteSnapshot::new();
            snapshot.nodes_created.push(format!("node_{i}"));
            ts.push(snapshot);
        }

        assert_eq!(ts.len(), 3); // Capped at max_size
        let latest = ts.pop().unwrap();
        assert_eq!(latest.nodes_created, vec!["node_4"]);
    }
}
