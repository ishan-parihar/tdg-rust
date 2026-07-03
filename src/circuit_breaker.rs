//! Circuit breaker: state machine with threshold/cooldown for graph safety.
//!
//! Ported from Python `circuit_breaker.py` (285 lines).

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::error::{TdgError, TdgResult};

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

// ─── Global Circuit Breaker ─────────────────────────────────────────────────

static GLOBAL_CIRCUIT_BREAKER: OnceLock<Mutex<CircuitBreaker>> = OnceLock::new();

/// Access the global circuit breaker instance.
pub fn global_circuit_breaker() -> &'static Mutex<CircuitBreaker> {
    GLOBAL_CIRCUIT_BREAKER.get_or_init(|| Mutex::new(CircuitBreaker::new()))
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
            // Update last_failure so the cooldown timer extends if failures
            // keep arriving during the Open window. Previously this was a
            // no-op, allowing the breaker to transition to HalfOpen even when
            // failures were still ongoing.
            CircuitState::Open => {
                // last_failure already updated above.
            }
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

    /// Check if writes are allowed. Returns error when circuit is open.
    ///
    /// Takes `&mut self` because it may transition `Open → HalfOpen` once the
    /// cooldown has elapsed. The previous `&self` implementation only checked
    /// the current state without ever transitioning, leaving the breaker
    /// permanently stuck in `Open` until something else called `is_tripped()`.
    pub fn check_before_write(&mut self) -> TdgResult<()> {
        if self.is_tripped() {
            return Err(TdgError::CircuitBreakerTripped {
                threshold: self.threshold as usize,
            });
        }
        Ok(())
    }
}

impl Default for CircuitBreaker {
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

}
