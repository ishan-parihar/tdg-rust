//! Session lifecycle — state machine for mind session state transitions.
//!
//! Provides [`SessionLifecycle`] which manages transitions through a defined
//! set of [`SessionState`] values, validates transitions, and keeps a full
//! history of every state change for debugging and analytics.
//!
//! This module is deliberately decoupled from persistence — use
//! [`MindStateManager`](crate::mind::state::MindStateManager) for that.

use chrono::{DateTime, Duration, Utc};

use crate::error::{TdgError, TdgResult};
use crate::mind::state::MindStateManager;

// ─── State Enum ───────────────────────────────────────────────────────────────

/// The possible states of a mind session.
///
/// The state machine defines the following valid transitions:
///
/// ```text
///           ┌──────────┐
///           │   Idle   │ ◁──── reset ────┐
///           └────┬─────┘                  │
///              start                      │
///           ┌────▼──────┐                 │
///           │   Active   │                │
///           └──┬─────┬───┘                │
///      pause  │     │  error              │
///       ┌─────▼──┐   │                    │
///       │ Paused │   │                    │
///       └─────┬──┘   │                    │
///      resume │     ┌▼──────┐             │
///              │     │ Error │─── reset ──┘
///              │     └───────┘
///        ┌─────▼─────────┐
///        │   Completed   │
///        └───────────────┘
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    /// No active session — initial state and post-reset state.
    Idle,

    /// A session is actively running with a plan description.
    Active {
        /// The plan or task being worked on.
        plan: String,
        /// When this active period began.
        started_at: DateTime<Utc>,
    },

    /// A session has been temporarily paused.
    Paused {
        /// The plan that was active when paused.
        plan: String,
        /// When the pause occurred.
        paused_at: DateTime<Utc>,
        /// Optional reason for pausing.
        reason: Option<String>,
    },

    /// A session completed successfully.
    Completed {
        /// The plan that was active when completed.
        plan: String,
        /// Total duration of the session in milliseconds.
        duration_ms: u64,
    },

    /// A session encountered an unrecoverable error.
    Error {
        /// The plan that was active when the error occurred.
        plan: String,
        /// Description of what went wrong.
        error: String,
    },
}

// ─── Event Enum ───────────────────────────────────────────────────────────────

/// Events that trigger state transitions in the session lifecycle.
#[derive(Debug, Clone, PartialEq)]
pub enum LifecycleEvent {
    /// Start a new session with a plan description.
    SessionStarted { plan: String },
    /// Pause the active session.
    SessionPaused { reason: Option<String> },
    /// Resume a paused session.
    SessionResumed,
    /// Mark the active session as successfully completed.
    SessionCompleted,
    /// Mark the active session as errored.
    SessionError { error: String },
}

// ─── Lifecycle Manager ────────────────────────────────────────────────────────

/// Manages the lifecycle of a mind session through valid state transitions.
///
/// # State machine
///
/// | From       | Event      | To        | Method       |
/// |------------|------------|-----------|--------------|
/// | `Idle`     | start      | `Active`  | [`start`]    |
/// | `Active`   | pause      | `Paused`  | [`pause`]    |
/// | `Paused`   | resume     | `Active`  | [`resume`]   |
/// | `Active`   | complete   | `Complete`| [`complete`] |
/// | `Active`   | error      | `Error`   | [`error`]    |
/// | `Error`    | reset      | `Idle`    | [`reset`]    |
/// | *any*      | reset      | `Idle`    | [`reset`]    |
///
/// Invalid transitions (e.g. `pause` from `Idle`) return
/// [`TdgError::Custom`].
///
/// [`start`]: Self::start
/// [`pause`]: Self::pause
/// [`resume`]: Self::resume
/// [`complete`]: Self::complete
/// [`error`]: Self::error
/// [`reset`]: Self::reset
pub struct SessionLifecycle {
    /// The current state of the session.
    current_state: SessionState,
    /// Ordered history of every state transition (state + timestamp).
    history: Vec<(SessionState, DateTime<Utc>)>,
    /// Optional integration with the persistent mind state manager.
    #[allow(dead_code)]
    state_manager: Option<MindStateManager>,
}

impl SessionLifecycle {
    /// Create a new lifecycle manager starting in the `Idle` state.
    ///
    /// The initial `Idle` entry is recorded in the history.
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            current_state: SessionState::Idle,
            history: vec![(SessionState::Idle, now)],
            state_manager: None,
        }
    }

    /// Create a new lifecycle manager with optional [`MindStateManager`]
    /// integration.
    ///
    /// The state manager is held for future use (e.g. synchronising the
    /// lifecycle state with the persisted [`MindState`]).
    pub fn with_state_manager(state_manager: MindStateManager) -> Self {
        let now = Utc::now();
        Self {
            current_state: SessionState::Idle,
            history: vec![(SessionState::Idle, now)],
            state_manager: Some(state_manager),
        }
    }

    // ─── State transition methods ─────────────────────────────────────────

    /// Transition from `Idle` → `Active`.
    ///
    /// Returns an error if the current state is not `Idle`.
    pub fn start(&mut self, plan: &str) -> TdgResult<()> {
        self.require_state_is(&[SessionState::Idle])?;
        let now = Utc::now();
        let new_state = SessionState::Active {
            plan: plan.to_string(),
            started_at: now,
        };
        self.apply(new_state, now);
        Ok(())
    }

    /// Transition from `Active` → `Paused`.
    ///
    /// Returns an error if the current state is not `Active`.
    pub fn pause(&mut self, reason: Option<&str>) -> TdgResult<()> {
        let plan = self.require_active()?;
        let now = Utc::now();
        let new_state = SessionState::Paused {
            plan,
            paused_at: now,
            reason: reason.map(|r| r.to_string()),
        };
        self.apply(new_state, now);
        Ok(())
    }

    /// Transition from `Paused` → `Active`.
    ///
    /// Returns an error if the current state is not `Paused`.
    pub fn resume(&mut self) -> TdgResult<()> {
        let plan = match &self.current_state {
            SessionState::Paused { plan, .. } => plan.clone(),
            _ => {
                return Err(TdgError::Custom(
                    "Cannot resume: session is not paused".into(),
                ))
            }
        };
        let now = Utc::now();
        let new_state = SessionState::Active {
            plan,
            started_at: now,
        };
        self.apply(new_state, now);
        Ok(())
    }

    /// Transition from `Active` → `Completed`.
    ///
    /// The duration is computed from the `started_at` timestamp of the active
    /// state. Returns an error if the current state is not `Active`.
    pub fn complete(&mut self) -> TdgResult<()> {
        let (plan, started_at) = match &self.current_state {
            SessionState::Active { plan, started_at } => (plan.clone(), *started_at),
            _ => {
                return Err(TdgError::Custom(
                    "Cannot complete: session is not active".into(),
                ))
            }
        };
        let now = Utc::now();
        let duration_ms = (now - started_at).num_milliseconds().max(0) as u64;
        let new_state = SessionState::Completed { plan, duration_ms };
        self.apply(new_state, now);
        Ok(())
    }

    /// Transition from `Active` → `Error`.
    ///
    /// Returns an error if the current state is not `Active`.
    pub fn error(&mut self, err: &str) -> TdgResult<()> {
        let plan = self.require_active()?;
        let now = Utc::now();
        let new_state = SessionState::Error {
            plan,
            error: err.to_string(),
        };
        self.apply(new_state, now);
        Ok(())
    }

    /// Reset the lifecycle to `Idle` regardless of the current state.
    ///
    /// This is the only transition that is valid from **any** state.
    pub fn reset(&mut self) {
        let now = Utc::now();
        self.apply(SessionState::Idle, now);
    }

    // ─── Accessors ────────────────────────────────────────────────────────

    /// Return a reference to the current session state.
    pub fn current_state(&self) -> &SessionState {
        &self.current_state
    }

    /// Return a slice of every state transition (state + timestamp) in order.
    pub fn history(&self) -> &[(SessionState, DateTime<Utc>)] {
        &self.history
    }

    /// Return the duration since the current `Active` period began, or `None`
    /// if the session is not currently active.
    pub fn duration_active(&self) -> Option<Duration> {
        match &self.current_state {
            SessionState::Active { started_at, .. } => Some(Utc::now() - *started_at),
            _ => None,
        }
    }

    // ─── Internal helpers ─────────────────────────────────────────────────

    /// Ensure the current state is one of the `allowed` states.
    fn require_state_is(&self, allowed: &[SessionState]) -> TdgResult<()> {
        if allowed.contains(&self.current_state) {
            Ok(())
        } else {
            Err(TdgError::Custom(format!(
                "Invalid transition from {:?}",
                self.current_state
            )))
        }
    }

    /// Extract the plan from `Active`, or return an error.
    fn require_active(&self) -> TdgResult<String> {
        match &self.current_state {
            SessionState::Active { plan, .. } => Ok(plan.clone()),
            _ => Err(TdgError::Custom(format!(
                "Session is not active (current state: {:?})",
                self.current_state
            ))),
        }
    }

    /// Record a new state in the history and update `current_state`.
    fn apply(&mut self, new_state: SessionState, at: DateTime<Utc>) {
        self.history.push((new_state.clone(), at));
        self.current_state = new_state;
    }
}

impl Default for SessionLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Initial state ───────────────────────────────────────────────────

    #[test]
    fn default_state_is_idle() {
        let lifecycle = SessionLifecycle::new();
        assert_eq!(lifecycle.current_state(), &SessionState::Idle);
    }

    #[test]
    fn history_starts_with_idle() {
        let lifecycle = SessionLifecycle::new();
        assert_eq!(lifecycle.history().len(), 1);
        assert_eq!(lifecycle.history()[0].0, SessionState::Idle);
    }

    #[test]
    fn default_trait_creates_idle() {
        let lifecycle = SessionLifecycle::default();
        assert_eq!(lifecycle.current_state(), &SessionState::Idle);
    }

    // ─── Valid transitions ───────────────────────────────────────────────

    #[test]
    fn start_transitions_to_active() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("test-plan").unwrap();
        match lifecycle.current_state() {
            SessionState::Active { plan, .. } => assert_eq!(plan, "test-plan"),
            other => panic!("Expected Active, got {:?}", other),
        }
    }

    #[test]
    fn pause_transitions_to_paused() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan-a").unwrap();
        lifecycle.pause(Some("need to think")).unwrap();
        match lifecycle.current_state() {
            SessionState::Paused { plan, reason, .. } => {
                assert_eq!(plan, "plan-a");
                assert_eq!(reason.as_deref(), Some("need to think"));
            }
            other => panic!("Expected Paused, got {:?}", other),
        }
    }

    #[test]
    fn pause_without_reason() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan-b").unwrap();
        lifecycle.pause(None).unwrap();
        match lifecycle.current_state() {
            SessionState::Paused { reason, .. } => assert!(reason.is_none()),
            other => panic!("Expected Paused, got {:?}", other),
        }
    }

    #[test]
    fn resume_transitions_to_active() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan-c").unwrap();
        lifecycle.pause(None).unwrap();
        lifecycle.resume().unwrap();
        match lifecycle.current_state() {
            SessionState::Active { plan, .. } => assert_eq!(plan, "plan-c"),
            other => panic!("Expected Active, got {:?}", other),
        }
    }

    #[test]
    fn complete_transitions_to_completed() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan-d").unwrap();
        lifecycle.complete().unwrap();
        match lifecycle.current_state() {
            SessionState::Completed { plan, duration_ms } => {
                assert_eq!(plan, "plan-d");
                assert!(*duration_ms >= 0);
            }
            other => panic!("Expected Completed, got {:?}", other),
        }
    }

    #[test]
    fn error_transitions_to_error() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan-e").unwrap();
        lifecycle.error("something broke").unwrap();
        match lifecycle.current_state() {
            SessionState::Error { plan, error } => {
                assert_eq!(plan, "plan-e");
                assert_eq!(error, "something broke");
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[test]
    fn reset_from_active_goes_to_idle() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan-f").unwrap();
        lifecycle.reset();
        assert_eq!(lifecycle.current_state(), &SessionState::Idle);
    }

    #[test]
    fn reset_from_paused_goes_to_idle() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan-g").unwrap();
        lifecycle.pause(None).unwrap();
        lifecycle.reset();
        assert_eq!(lifecycle.current_state(), &SessionState::Idle);
    }

    #[test]
    fn reset_from_completed_goes_to_idle() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan-h").unwrap();
        lifecycle.complete().unwrap();
        lifecycle.reset();
        assert_eq!(lifecycle.current_state(), &SessionState::Idle);
    }

    #[test]
    fn reset_from_error_goes_to_idle() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan-i").unwrap();
        lifecycle.error("oops").unwrap();
        lifecycle.reset();
        assert_eq!(lifecycle.current_state(), &SessionState::Idle);
    }

    #[test]
    fn reset_from_idle_stays_idle() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.reset();
        assert_eq!(lifecycle.current_state(), &SessionState::Idle);
    }

    // ─── Invalid transitions ─────────────────────────────────────────────

    #[test]
    fn start_from_active_fails() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        let result = lifecycle.start("another-plan");
        assert!(result.is_err());
    }

    #[test]
    fn pause_from_idle_fails() {
        let mut lifecycle = SessionLifecycle::new();
        let result = lifecycle.pause(None);
        assert!(result.is_err());
    }

    #[test]
    fn pause_from_paused_fails() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        lifecycle.pause(None).unwrap();
        let result = lifecycle.pause(None);
        assert!(result.is_err());
    }

    #[test]
    fn pause_from_completed_fails() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        lifecycle.complete().unwrap();
        let result = lifecycle.pause(None);
        assert!(result.is_err());
    }

    #[test]
    fn resume_from_idle_fails() {
        let mut lifecycle = SessionLifecycle::new();
        let result = lifecycle.resume();
        assert!(result.is_err());
    }

    #[test]
    fn resume_from_active_fails() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        let result = lifecycle.resume();
        assert!(result.is_err());
    }

    #[test]
    fn resume_from_completed_fails() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        lifecycle.complete().unwrap();
        let result = lifecycle.resume();
        assert!(result.is_err());
    }

    #[test]
    fn complete_from_idle_fails() {
        let mut lifecycle = SessionLifecycle::new();
        let result = lifecycle.complete();
        assert!(result.is_err());
    }

    #[test]
    fn complete_from_paused_fails() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        lifecycle.pause(None).unwrap();
        let result = lifecycle.complete();
        assert!(result.is_err());
    }

    #[test]
    fn error_from_idle_fails() {
        let mut lifecycle = SessionLifecycle::new();
        let result = lifecycle.error("fail");
        assert!(result.is_err());
    }

    #[test]
    fn error_from_paused_fails() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        lifecycle.pause(None).unwrap();
        let result = lifecycle.error("fail");
        assert!(result.is_err());
    }

    #[test]
    fn error_from_completed_fails() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        lifecycle.complete().unwrap();
        let result = lifecycle.error("fail");
        assert!(result.is_err());
    }

    // ─── History tracking ────────────────────────────────────────────────

    #[test]
    fn history_accumulates_transitions() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        lifecycle.pause(None).unwrap();
        lifecycle.resume().unwrap();
        lifecycle.complete().unwrap();

        // Idle → Active → Paused → Active → Completed = 5 entries.
        assert_eq!(lifecycle.history().len(), 5);
        assert_eq!(lifecycle.history()[0].0, SessionState::Idle);
        assert!(matches!(lifecycle.history()[1].0, SessionState::Active { .. }));
        assert!(matches!(lifecycle.history()[2].0, SessionState::Paused { .. }));
        assert!(matches!(lifecycle.history()[3].0, SessionState::Active { .. }));
        assert!(matches!(lifecycle.history()[4].0, SessionState::Completed { .. }));
    }

    #[test]
    fn history_timestamps_are_monotonic() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1));
        lifecycle.pause(None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1));
        lifecycle.resume().unwrap();

        for window in lifecycle.history().windows(2) {
            assert!(
                window[1].1 >= window[0].1,
                "timestamps must be monotonic"
            );
        }
    }

    #[test]
    fn reset_appends_to_history() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        lifecycle.reset();
        assert_eq!(lifecycle.history().len(), 3);
        assert_eq!(lifecycle.history()[2].0, SessionState::Idle);
    }

    // ─── Duration ────────────────────────────────────────────────────────

    #[test]
    fn duration_active_returns_none_when_not_active() {
        let lifecycle = SessionLifecycle::new();
        assert!(lifecycle.duration_active().is_none());
    }

    #[test]
    fn duration_active_returns_some_when_active() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        let duration = lifecycle.duration_active();
        assert!(duration.is_some());
        assert!(duration.unwrap().num_milliseconds() >= 0);
    }

    #[test]
    fn duration_active_increases_over_time() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        let d1 = lifecycle.duration_active().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let d2 = lifecycle.duration_active().unwrap();
        assert!(d2 > d1);
    }

    // ─── Full lifecycle scenarios ────────────────────────────────────────

    #[test]
    fn full_active_pause_resume_complete_cycle() {
        let mut lifecycle = SessionLifecycle::new();
        assert_eq!(lifecycle.current_state(), &SessionState::Idle);

        lifecycle.start("my-plan").unwrap();
        assert!(matches!(lifecycle.current_state(), SessionState::Active { .. }));

        lifecycle.pause(Some("reviewing")).unwrap();
        assert!(matches!(lifecycle.current_state(), SessionState::Paused { .. }));

        lifecycle.resume().unwrap();
        assert!(matches!(lifecycle.current_state(), SessionState::Active { .. }));

        lifecycle.complete().unwrap();
        assert!(matches!(lifecycle.current_state(), SessionState::Completed { .. }));

        // History: Idle → Active → Paused → Active → Completed = 5.
        assert_eq!(lifecycle.history().len(), 5);
    }

    #[test]
    fn full_active_error_reset_cycle() {
        let mut lifecycle = SessionLifecycle::new();

        lifecycle.start("risky-plan").unwrap();
        assert!(matches!(lifecycle.current_state(), SessionState::Active { .. }));

        lifecycle.error("critical failure").unwrap();
        assert!(matches!(lifecycle.current_state(), SessionState::Error { .. }));

        lifecycle.reset();
        assert_eq!(lifecycle.current_state(), &SessionState::Idle);

        // Can start again after reset.
        lifecycle.start("retry-plan").unwrap();
        assert!(matches!(lifecycle.current_state(), SessionState::Active { .. }));
    }

    #[test]
    fn error_message_describes_what_went_wrong() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        lifecycle.error("connection timeout after 30s").unwrap();
        match lifecycle.current_state() {
            SessionState::Error { error, .. } => {
                assert!(error.contains("connection timeout"));
            }
            _ => panic!("Expected Error state"),
        }
    }

    // ─── Integration with state_manager (compile-time only) ──────────────

    #[test]
    fn with_state_manager_creates_idle_lifecycle() {
        // We only verify the constructor compiles and returns Idle.
        // Full integration tests belong in an integration test crate.
        let cfg = crate::config::Config::from_env();
        let mgr = MindStateManager::new(cfg);
        let lifecycle = SessionLifecycle::with_state_manager(mgr);
        assert_eq!(lifecycle.current_state(), &SessionState::Idle);
    }

    // ─── Edge cases ──────────────────────────────────────────────────────

    #[test]
    fn double_reset_keeps_idle() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.reset();
        lifecycle.reset();
        assert_eq!(lifecycle.current_state(), &SessionState::Idle);
    }

    #[test]
    fn start_with_empty_plan() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("").unwrap();
        match lifecycle.current_state() {
            SessionState::Active { plan, .. } => assert_eq!(plan, ""),
            other => panic!("Expected Active, got {:?}", other),
        }
    }

    #[test]
    fn error_from_completed_returns_descriptive_message() {
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.start("plan").unwrap();
        lifecycle.complete().unwrap();
        let err = lifecycle.error("fail").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not active"), "error should mention 'not active', got: {msg}");
    }
}
