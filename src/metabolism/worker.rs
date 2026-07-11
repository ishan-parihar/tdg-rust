//! Metabolism Worker — Tier 2 async job processor.
//!
//! Processes metabolism jobs from the `pending_metabolism` table. Each job
//! is a single holon-level operation (lesser tick, attractor recompute, etc.)
//! that runs in < 100ms on a background thread.
//!
//! ## Job lifecycle
//!
//! ```text
//! Tier 1 write ──► enqueue_job() ──► pending_metabolism table
//!                                              │
//!                                              ↓
//!                                    Worker.claim_job()
//!                                              │
//!                                    Worker.execute_job()
//!                                              │
//!                                    ┌─────────┴─────────┐
//!                                    ↓                   ↓
//!                               mark_done()         mark_failed()
//!                                                    (re-enqueue if attempts < max)
//! ```
//!
//! ## Backpressure
//!
//! If the queue depth exceeds 10K jobs, Tier 1 starts rejecting non-essential
//! writes. If it exceeds 100K, all writes except reads are rejected.

use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;

use crate::db::ConnectionPool;
use crate::error::TdgResult;

use super::lesser_cycle::{self, CycleThresholds};

// ─── Job Types ───────────────────────────────────────────────────────────────

/// The type of metabolism job to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobType {
    /// Run a lesser-cycle tick on a holon (process pending catalyst).
    LesserTick,
    /// Inject catalyst into a holon and tick if threshold crossed.
    CatalystInjection,
    /// Recompute the attractor field for a holon (Phase 3).
    RecomputeAttractor,
    /// Recompute health metrics (Phase 3).
    RecomputeHealth,
    /// Update resonance graph for a holon (Phase 3).
    ResonanceUpdate,
    /// Run a greater-cycle tick on a holon (Phase 4 — check for transformation).
    GreaterTick,
}

impl JobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LesserTick => "lesser_tick",
            Self::CatalystInjection => "catalyst_injection",
            Self::RecomputeAttractor => "recompute_attractor",
            Self::RecomputeHealth => "recompute_health",
            Self::ResonanceUpdate => "resonance_update",
            Self::GreaterTick => "greater_tick",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "lesser_tick" => Some(Self::LesserTick),
            "catalyst_injection" => Some(Self::CatalystInjection),
            "recompute_attractor" => Some(Self::RecomputeAttractor),
            "recompute_health" => Some(Self::RecomputeHealth),
            "resonance_update" => Some(Self::ResonanceUpdate),
            "greater_tick" => Some(Self::GreaterTick),
            _ => None,
        }
    }
}

/// A pending metabolism job.
#[derive(Debug, Clone)]
pub struct PendingJob {
    pub id: i64,
    pub holon_id: String,
    pub job_type: JobType,
    pub payload: serde_json::Value,
    pub priority: i32,
    pub attempts: i32,
    pub max_attempts: i32,
}

/// Job priority levels.
pub const PRIORITY_LOW: i32 = 0;
pub const PRIORITY_NORMAL: i32 = 1;
pub const PRIORITY_HIGH: i32 = 2;

/// Maximum queue depth before backpressure kicks in.
pub const BACKPRESSURE_WARNING: i64 = 10_000;
pub const BACKPRESSURE_CRITICAL: i64 = 100_000;

// ─── Job Queue Operations ────────────────────────────────────────────────────

/// Enqueue a metabolism job.
///
/// Called from Tier 1 write paths (tdg_connect, tdg_observe) to schedule
/// async metabolism work. Non-blocking — just inserts a row.
pub fn enqueue_job(
    conn: &Connection,
    holon_id: &str,
    job_type: JobType,
    payload: serde_json::Value,
    priority: i32,
) -> TdgResult<i64> {
    let now = crate::db::crud::now_iso();
    conn.execute(
        "INSERT INTO pending_metabolism (holon_id, job_type, payload, priority, enqueued_at, attempts, max_attempts)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, 3)",
        rusqlite::params![
            holon_id,
            job_type.as_str(),
            payload.to_string(),
            priority,
            now,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Get the current queue depth (number of pending jobs).
pub fn queue_depth(conn: &Connection) -> TdgResult<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pending_metabolism WHERE attempts < max_attempts",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Claim the next available job (atomic via rowid ordering).
///
/// Marks the job as "in progress" by incrementing attempts. The caller
/// is responsible for deleting the job on success or leaving it for retry
/// on failure.
pub fn claim_job(conn: &Connection) -> Result<Option<PendingJob>, rusqlite::Error> {
    // Use a transaction to atomically claim a job
    let tx = conn.unchecked_transaction()?;

    // Find the highest-priority, oldest job
    let job = tx.query_row(
        "SELECT id, holon_id, job_type, payload, priority, attempts, max_attempts
         FROM pending_metabolism
         WHERE attempts < max_attempts
         ORDER BY priority DESC, enqueued_at ASC
         LIMIT 1",
        [],
        |row| {
            let id: i64 = row.get(0)?;
            let holon_id: String = row.get(1)?;
            let job_type_str: String = row.get(2)?;
            let payload_str: String = row.get(3)?;
            let priority: i32 = row.get(4)?;
            let attempts: i32 = row.get(5)?;
            let max_attempts: i32 = row.get(6)?;

            Ok(PendingJob {
                id,
                holon_id,
                job_type: JobType::from_str(&job_type_str).unwrap_or(JobType::LesserTick),
                payload: serde_json::from_str(&payload_str).unwrap_or(serde_json::json!({})),
                priority,
                attempts,
                max_attempts,
            })
        },
    );

    let mut job = match job {
        Ok(j) => j,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Ok(None);
        }
        Err(e) => return Err(e),
    };

    // Increment attempts in DB and in the returned struct
    tx.execute(
        "UPDATE pending_metabolism SET attempts = attempts + 1 WHERE id = ?1",
        rusqlite::params![job.id],
    )?;
    job.attempts += 1;

    tx.commit()?;
    Ok(Some(job))
}

/// Mark a job as complete (delete it).
pub fn mark_done(conn: &Connection, job_id: i64) -> TdgResult<()> {
    conn.execute(
        "DELETE FROM pending_metabolism WHERE id = ?1",
        rusqlite::params![job_id],
    )?;
    Ok(())
}

/// Mark a job as failed. If attempts < max_attempts, it stays in the queue
/// for retry. Otherwise, move it to failed_metabolism.
pub fn mark_failed(conn: &Connection, job: &PendingJob, error: &str) -> TdgResult<()> {
    if job.attempts >= job.max_attempts {
        // Move to failed_metabolism table
        let now = crate::db::crud::now_iso();
        conn.execute(
            "INSERT INTO failed_metabolism (holon_id, job_type, payload, error, failed_at, attempts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                job.holon_id,
                job.job_type.as_str(),
                job.payload.to_string(),
                error,
                now,
                job.attempts,
            ],
        )?;
        // Remove from pending
        conn.execute(
            "DELETE FROM pending_metabolism WHERE id = ?1",
            rusqlite::params![job.id],
        )?;
    }
    // If attempts < max_attempts, leave in queue for retry (attempts already incremented)
    Ok(())
}

// ─── Job Execution ───────────────────────────────────────────────────────────

/// Execute a single metabolism job.
///
/// Returns Ok(()) on success, Err on failure. The caller (worker) handles
/// retry logic.
pub fn execute_job(conn: &Connection, job: &PendingJob) -> TdgResult<()> {
    match job.job_type {
        JobType::LesserTick | JobType::CatalystInjection => execute_lesser_tick(conn, job),
        JobType::RecomputeAttractor => execute_recompute_attractor(conn, job),
        JobType::RecomputeHealth => execute_recompute_health(conn, job),
        JobType::ResonanceUpdate => execute_resonance_update(conn, job),
        JobType::GreaterTick => execute_greater_tick(conn, job),
    }
}

/// Execute a greater-cycle tick job (Phase 4).
///
/// Loads the holon's greater cycle state + lesser cycle state, runs the
/// greater tick, saves the state, and handles:
/// - Transformation events → log + mark attractor dirty
/// - Choice locked → enqueue downward pressure to children
/// - Octave ascended → trigger stage advancement via telearchy
fn execute_greater_tick(conn: &Connection, job: &PendingJob) -> TdgResult<()> {
    let mut greater = crate::metabolism::greater_cycle::load_state(conn, &job.holon_id)?;
    let lesser = crate::metabolism::lesser_cycle::load_state(conn, &job.holon_id)?;

    let thresholds = crate::metabolism::greater_cycle::GreaterThresholds::default();
    let result = crate::metabolism::greater_cycle::tick(&mut greater, &lesser, &thresholds);

    // Save the updated greater cycle state
    crate::metabolism::greater_cycle::save_state(conn, &job.holon_id, &greater)?;

    // Log transformation events
    if result.transformation_fired {
        tracing::info!(
            "Holon {} TRANSFORMATION fired: crucible={:?}, significator={:.2}, pressure={:.2}",
            job.holon_id,
            greater.crucible_intensity,
            greater.significator,
            greater.transformation_pressure
        );

        // Transformation restructures the Significator → attractor field is stale
        let _ = crate::metabolism::attractor::mark_dirty(conn, &job.holon_id);
        let _ = enqueue_job(
            conn,
            &job.holon_id,
            JobType::RecomputeAttractor,
            serde_json::json!({"trigger": "greater_cycle_transformation"}),
            PRIORITY_HIGH,
        );
    }

    // Log phase transitions
    if result.transitioned {
        if let (Some(from), Some(to)) = (result.from_phase.as_ref(), result.to_phase.as_ref()) {
            tracing::debug!("Holon {} greater cycle: {} → {}", job.holon_id, from, to);
        }
    }

    // Handle octave ascended → stage advancement + downward pressure
    if result.octave_ascended {
        tracing::info!(
            "Holon {} OCTAVE ASCENDED: octave_count={}, triggering stage advancement",
            job.holon_id,
            greater.octave_count
        );

        // Trigger stage advancement via telearchy (if the holon has a developmental_stage)
        if let Some(node) = crate::db::crud::get_node(conn, &job.holon_id)? {
            if node.developmental_stage.is_some() {
                let telearchy = crate::telearchy::TelearchyEngine::new(conn);
                match telearchy.advance_stage(&job.holon_id) {
                    Ok(Some(new_stage)) => {
                        tracing::info!(
                            "Holon {} stage advanced to {:?} via Transformation event",
                            job.holon_id,
                            new_stage
                        );
                    }
                    Ok(None) => {
                        // Stage didn't advance (evidence/age gates not met) — that's fine
                        tracing::debug!(
                            "Holon {} stage not advanced (gates not met) despite Transformation",
                            job.holon_id
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Holon {} stage advancement failed after Transformation: {}",
                            job.holon_id,
                            e
                        );
                    }
                }
            }
        }

        // Send downward pressure to children
        if result.downward_pressure > 0.0 {
            // Get children via DECOMPOSES_TO edges
            let children = crate::db::crud::get_edges(
                conn,
                Some(&job.holon_id),
                None,
                Some("DECOMPOSES_TO"),
                None,
                100,
            )?;

            for edge in children {
                let payload = serde_json::json!({
                    "catalyst_amount": result.downward_pressure,
                    "source": "greater_cycle_downward",
                    "source_holon": job.holon_id,
                });
                let _ = enqueue_job(
                    conn,
                    &edge.target_id,
                    JobType::CatalystInjection,
                    payload,
                    PRIORITY_NORMAL,
                );
            }
        }
    }

    Ok(())
}

/// Execute a RecomputeAttractor job (Phase 3).
///
/// Loads the holon's lesser cycle state + drives + edge count, computes
/// the attractor field, saves it, and enqueues a RecomputeHealth job +
/// a ResonanceUpdate job (if the type_class changed).
fn execute_recompute_attractor(conn: &Connection, job: &PendingJob) -> TdgResult<()> {
    // Load the holon to get drives_json
    let node = crate::db::crud::get_node(conn, &job.holon_id)?
        .ok_or_else(|| crate::error::TdgError::NotFound(job.holon_id.clone()))?;

    // Load the lesser cycle state
    let lesser = crate::metabolism::lesser_cycle::load_state(conn, &job.holon_id)?;

    // Count active edges (for A_G magnitude)
    let edge_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges WHERE (source_id = ?1 OR target_id = ?1) AND valid_to IS NULL",
        rusqlite::params![job.holon_id],
        |row| row.get(0),
    )?;

    // Compute the attractor field
    let af = crate::metabolism::attractor::compute(
        &lesser,
        &node.drives,
        edge_count,
        lesser.transformation_pressure,
    );

    // Check if type_class changed (for resonance update)
    let old_af = crate::metabolism::attractor::load(conn, &job.holon_id)?;
    let type_changed = old_af
        .as_ref()
        .map(|o| o.type_class != af.type_class)
        .unwrap_or(true);

    // Save the attractor field
    crate::metabolism::attractor::save(conn, &job.holon_id, &af)?;

    tracing::debug!(
        "Holon {} attractor recomputed: type_class={}, pi={:?}, stable={}",
        job.holon_id,
        af.type_class,
        af.pi,
        af.is_stable()
    );

    // Enqueue health recompute (attractor changed → health needs update)
    let _ = enqueue_job(
        conn,
        &job.holon_id,
        JobType::RecomputeHealth,
        serde_json::json!({}),
        PRIORITY_NORMAL,
    );

    // Enqueue resonance update if type changed
    if type_changed {
        let _ = enqueue_job(
            conn,
            &job.holon_id,
            JobType::ResonanceUpdate,
            serde_json::json!({"type_changed": true}),
            PRIORITY_LOW,
        );
    }

    Ok(())
}

/// Execute a RecomputeHealth job (Phase 3).
///
/// Loads the holon's lesser cycle state + attractor field, computes G_z/P_z,
/// saves health, and logs state classification.
fn execute_recompute_health(conn: &Connection, job: &PendingJob) -> TdgResult<()> {
    let lesser = crate::metabolism::lesser_cycle::load_state(conn, &job.holon_id)?;

    let af = crate::metabolism::attractor::load(conn, &job.holon_id)?
        .unwrap_or_else(crate::metabolism::attractor::AttractorField::dormant);

    let health = crate::metabolism::health::Health::compute(&lesser, &af);

    // Log interesting state transitions
    if health.state == crate::metabolism::health::HealthState::Depolarized {
        tracing::info!(
            "Holon {} in SINKHOLE state: G_z={:.1}, P_z={:.1} — depolarized, needs commitment",
            job.holon_id,
            health.g_z,
            health.p_z
        );
    } else if health.state == crate::metabolism::health::HealthState::Collapse {
        tracing::info!(
            "Holon {} in COLLAPSE state: G_z={:.1} — severe boundary distortion",
            job.holon_id,
            health.g_z
        );
    }

    crate::metabolism::health::save(conn, &job.holon_id, &health)?;

    tracing::debug!(
        "Holon {} health recomputed: G_z={:.1}, P_z={:.1}, state={}",
        job.holon_id,
        health.g_z,
        health.p_z,
        health.state.as_str()
    );

    Ok(())
}

/// Execute a ResonanceUpdate job (Phase 3).
///
/// Computes resonance between this holon and up to 50 candidate partners
/// (selected by complementary type_class), stores top-10 in resonance_graph.
fn execute_resonance_update(conn: &Connection, job: &PendingJob) -> TdgResult<()> {
    let af1 = match crate::metabolism::attractor::load(conn, &job.holon_id)? {
        Some(af) if af.is_stable() => af,
        _ => {
            // Not stable or not computed — clear any existing resonance entries
            conn.execute(
                "DELETE FROM resonance_graph WHERE holon_id = ?1",
                rusqlite::params![job.holon_id],
            )?;
            return Ok(());
        }
    };

    // Find candidate partners: holons with attractor fields, excluding self.
    // Limit to 50 candidates (the lean VPS profile).
    // Prioritize holons with complementary type_class (donor ↔ acceptor).
    let mut stmt = conn.prepare(
        "SELECT n.id, n.attractor_field_json
         FROM nodes n
         WHERE n.id != ?1
           AND n.valid_to IS NULL
           AND n.attractor_field_json IS NOT NULL
           AND n.attractor_field_json != ''
         ORDER BY n.updated_at DESC
         LIMIT 50",
    )?;

    let candidates: Vec<(String, String)> = stmt
        .query_map(rusqlite::params![job.holon_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Compute resonance for each candidate
    let now = crate::db::crud::now_iso();
    let mut results: Vec<(String, f64, f64, f64, f64)> = Vec::new();

    for (partner_id, partner_json) in candidates {
        if let Some(af2) = crate::metabolism::attractor::AttractorField::from_json(&partner_json) {
            if !af2.is_stable() {
                continue;
            }

            // F5 fix: compute and store individual resonance components
            let rc = crate::metabolism::health::resonance_with_components(&af1, &af2);

            if rc.resonance > 0.0 {
                results.push((
                    partner_id,
                    rc.resonance,
                    rc.complementarity,
                    rc.gamma_compat,
                    rc.great_way_intersect,
                ));
            }
        }
    }

    // Sort by resonance score descending, keep top 10
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(10);

    // Replace existing entries for this holon
    conn.execute(
        "DELETE FROM resonance_graph WHERE holon_id = ?1",
        rusqlite::params![job.holon_id],
    )?;

    for (partner_id, score, comp, gamma_compat, gw) in &results {
        conn.execute(
            "INSERT OR REPLACE INTO resonance_graph
                (holon_id, partner_id, resonance_score, complementarity, gamma_compat, great_way_intersect, computed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                job.holon_id,
                partner_id,
                score,
                comp,
                gamma_compat,
                gw,
                now,
            ],
        )?;
    }

    tracing::debug!(
        "Holon {} resonance updated: {} partners (top score: {:.3})",
        job.holon_id,
        results.len(),
        results.first().map(|r| r.1).unwrap_or(0.0)
    );

    Ok(())
}

/// Execute a lesser-cycle tick job.
///
/// Loads the holon's lesser cycle state, runs the tick, saves the state,
/// and enqueues upward pressure to parents if needed.
fn execute_lesser_tick(conn: &Connection, job: &PendingJob) -> TdgResult<()> {
    let mut state = lesser_cycle::load_state(conn, &job.holon_id)?;

    // Extract catalyst amount from payload (for catalyst injection jobs)
    let incoming_catalyst = job
        .payload
        .get("catalyst_amount")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let thresholds = CycleThresholds::default();
    let result = lesser_cycle::tick(&mut state, incoming_catalyst, &thresholds);

    // Phase 16: Hebbian co-activation tracking (LTP signal).
    // When a holon processes catalyst from an edge, increment the co-activation
    // count on that edge. This is the "fire together" signal.
    if incoming_catalyst > 0.0 {
        if let Some(source_holon) = job.payload.get("source_holon").and_then(|v| v.as_str()) {
            let now = crate::db::crud::now_iso();
            let _ = conn.execute(
                "UPDATE edges SET co_activation_count = co_activation_count + 1, last_co_activation = ?1
                 WHERE ((source_id = ?2 AND target_id = ?3) OR (source_id = ?3 AND target_id = ?2))
                 AND valid_to IS NULL",
                rusqlite::params![now, source_holon, job.holon_id],
            );
        }
    }

    // Save the updated state
    lesser_cycle::save_state(conn, &job.holon_id, &state)?;

    // Log phase transitions as events
    if result.transitioned {
        if let (Some(from), Some(to)) = (result.from_phase.as_ref(), result.to_phase.as_ref()) {
            tracing::debug!("Holon {} lesser cycle: {} → {}", job.holon_id, from, to);
        }
    }

    // Log shadow diagnoses
    if result.shadow_diagnosed {
        tracing::info!(
            "Holon {} shadow diagnosed: matrix={:?}, potentiator={:?}",
            job.holon_id,
            state.matrix.shadow,
            state.potentiator.shadow
        );
    }

    // Enqueue upward pressure to parents if Experience crossed threshold
    if result.upward_pressure && result.upward_experience > 0.0 {
        // Load the node to get parent_ids
        if let Some(node) = crate::db::crud::get_node(conn, &job.holon_id)? {
            for parent_id in &node.parent_ids {
                let payload = serde_json::json!({
                    "catalyst_amount": result.upward_experience,
                    "source": "upward_pressure",
                    "source_holon": job.holon_id,
                });
                let _ = enqueue_job(
                    conn,
                    parent_id,
                    JobType::CatalystInjection,
                    payload,
                    PRIORITY_NORMAL,
                );
            }
        }
    }

    // Phase 3: When the lesser cycle reaches Integrating or completes a cycle,
    // the attractor field is stale (reservoirs/shadows may have changed).
    // Mark dirty and enqueue a recompute.
    if result.shadow_diagnosed || result.cycle_completed {
        let _ = crate::metabolism::attractor::mark_dirty(conn, &job.holon_id);
        let _ = enqueue_job(
            conn,
            &job.holon_id,
            JobType::RecomputeAttractor,
            serde_json::json!({"trigger": "lesser_cycle_integration"}),
            PRIORITY_NORMAL,
        );
    }

    // Phase 19: Drive-metabolism unification — drives adapt based on experience.
    // When a holon completes a cycle, its drive profile shifts slightly toward
    // the metabolic state it just processed. This makes drives_json a LEARNED
    // representation, not a hardcoded one. The brain's neural representations
    // adapt through experience; TDG's drives now do the same.
    if result.cycle_completed && state.experience_accumulated > 0.1 {
        if let Ok(Some(node)) = crate::db::crud::get_node(conn, &job.holon_id) {
            let drives = node.drives.clone();
            if let Some(drives_obj) = drives.as_object() {
                let mut updated = false;
                let mut new_drives = drives_obj.clone();

                // Adaptation rule: drives that were exercised (high catalyst processing)
                // strengthen their positive pole by a small learning rate.
                // The adaptation is proportional to experience_accumulated.
                let learning_rate = (state.experience_accumulated * 0.05).min(0.5);

                for (drive_name, drive_val) in drives_obj.iter() {
                    if let Some(drive_obj) = drive_val.as_object() {
                        let pos = drive_obj
                            .get("positive_pole")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(5.0);
                        let neg = drive_obj
                            .get("negative_pole")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(2.0);

                        // Strengthen positive pole (the drive was exercised)
                        let new_pos = (pos + learning_rate).min(10.0);
                        // Slightly weaken negative pole (resistance decreases with use)
                        let new_neg = (neg - learning_rate * 0.3).max(0.0);

                        let mut new_drive = drive_obj.clone();
                        new_drive.insert("positive_pole".to_string(), serde_json::json!(new_pos));
                        new_drive.insert("negative_pole".to_string(), serde_json::json!(new_neg));
                        new_drives.insert(drive_name.clone(), serde_json::json!(new_drive));
                        updated = true;
                    }
                }

                if updated {
                    let new_drives_json = serde_json::Value::Object(new_drives);
                    // P1 fix: route through flow::store_drive_state which acquires
                    // write_guard + checks circuit_breaker, instead of raw UPDATE.
                    let _ =
                        crate::flow::store_drive_state_pub(conn, &job.holon_id, &new_drives_json);
                    tracing::debug!(
                        "Holon {} drives adapted (learning_rate={:.3}, experience={:.3})",
                        job.holon_id,
                        learning_rate,
                        state.experience_accumulated
                    );
                }
            }
        }
    }

    Ok(())
}

// ─── The Worker ──────────────────────────────────────────────────────────────

/// Background metabolism worker pool.
///
/// Runs in a tokio task, claiming and executing jobs from the
/// `pending_metabolism` table. Designed for the 2GB VPS lean profile:
/// default 1 worker, configurable via `TDG_METABOLISM_WORKERS`.
pub struct MetabolismWorker {
    pool: Arc<ConnectionPool>,
    num_workers: usize,
    poll_interval: Duration,
}

impl MetabolismWorker {
    /// Create a new worker pool.
    ///
    /// - `pool`: shared connection pool (workers each claim their own connection)
    /// - `num_workers`: number of worker threads (default 1 for lean VPS)
    pub fn new(pool: Arc<ConnectionPool>, num_workers: usize) -> Self {
        Self {
            pool,
            num_workers: num_workers.max(1),
            poll_interval: Duration::from_millis(100),
        }
    }

    /// Create a worker with a custom poll interval (for testing).
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Run the worker pool. Blocks until the runtime is shut down.
    ///
    /// Each worker runs in a `spawn_blocking` task, claiming and executing
    /// jobs in a loop. When no jobs are available, it sleeps for
    /// `poll_interval` before retrying.
    pub async fn run(self) {
        let mut handles = Vec::new();

        for worker_id in 0..self.num_workers {
            let pool = self.pool.clone();
            let interval = self.poll_interval;
            handles.push(tokio::spawn(async move {
                tracing::info!("Metabolism worker {} started", worker_id);
                Self::worker_loop(pool, interval, worker_id).await;
            }));
        }

        // Wait for all workers (they run indefinitely until the runtime shuts down)
        for handle in handles {
            let _ = handle.await;
        }
    }

    async fn worker_loop(pool: Arc<ConnectionPool>, poll_interval: Duration, worker_id: usize) {
        loop {
            // Claim and execute a job
            let pool_clone = pool.clone();
            let result = tokio::task::spawn_blocking(move || -> TdgResult<bool> {
                let conn = pool_clone
                    .get_connection()
                    .map_err(|e| crate::error::TdgError::Custom(e.to_string()))?;

                let job = match claim_job(&conn) {
                    Ok(Some(j)) => j,
                    Ok(None) => {
                        pool_clone.release_connection(conn);
                        return Ok(false); // no job available
                    }
                    Err(e) => {
                        tracing::warn!("Worker {} failed to claim job: {}", worker_id, e);
                        pool_clone.release_connection(conn);
                        return Ok(false);
                    }
                };

                // Execute the job
                match execute_job(&conn, &job) {
                    Ok(()) => {
                        if let Err(e) = mark_done(&conn, job.id) {
                            tracing::warn!(
                                "Worker {} failed to mark job {} done: {}",
                                worker_id,
                                job.id,
                                e
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Worker {} job {} failed (attempt {}): {}",
                            worker_id,
                            job.id,
                            job.attempts,
                            e
                        );
                        if let Err(e2) = mark_failed(&conn, &job, &e.to_string()) {
                            tracing::warn!(
                                "Worker {} failed to mark job {} failed: {}",
                                worker_id,
                                job.id,
                                e2
                            );
                        }
                    }
                }

                pool_clone.release_connection(conn);
                Ok(true) // processed a job
            })
            .await;

            // If no job was available, sleep before retrying
            match result {
                Ok(Ok(true)) => {
                    // Processed a job — immediately try the next one
                    continue;
                }
                Ok(Ok(false)) => {
                    // No job available — sleep
                    tokio::time::sleep(poll_interval).await;
                }
                Ok(Err(e)) => {
                    tracing::warn!("Worker {} error: {}", worker_id, e);
                    tokio::time::sleep(poll_interval).await;
                }
                Err(e) => {
                    tracing::warn!("Worker {} spawn_blocking failed: {}", worker_id, e);
                    tokio::time::sleep(poll_interval).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_fts, init_schema, run_migrations};
    use crate::metabolism::LesserPhase;
    use crate::models::NewNode;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn enqueue_and_claim_job() {
        let conn = setup_db();

        // Create a node to tick
        let node = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Test".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Enqueue a job
        let job_id = enqueue_job(
            &conn,
            &node.id,
            JobType::CatalystInjection,
            serde_json::json!({"catalyst_amount": 0.5}),
            PRIORITY_NORMAL,
        )
        .unwrap();

        assert!(job_id > 0);
        assert_eq!(queue_depth(&conn).unwrap(), 1);

        // Claim the job
        let job = claim_job(&conn).unwrap().expect("should have a job");
        assert_eq!(job.holon_id, node.id);
        assert_eq!(job.job_type, JobType::CatalystInjection);
        assert_eq!(job.attempts, 1); // incremented by claim
    }

    #[test]
    pub fn claim_job_returns_none_when_empty() {
        let conn = setup_db();
        let job = claim_job(&conn).unwrap();
        assert!(job.is_none());
    }

    #[test]
    pub fn mark_done_removes_job() {
        let conn = setup_db();

        let node = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Test".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let job_id = enqueue_job(
            &conn,
            &node.id,
            JobType::LesserTick,
            serde_json::json!({}),
            PRIORITY_NORMAL,
        )
        .unwrap();

        assert_eq!(queue_depth(&conn).unwrap(), 1);
        mark_done(&conn, job_id).unwrap();
        assert_eq!(queue_depth(&conn).unwrap(), 0);
    }

    #[test]
    fn job_priority_ordering() {
        let conn = setup_db();

        let node = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Test".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Enqueue in reverse priority order
        enqueue_job(
            &conn,
            &node.id,
            JobType::LesserTick,
            serde_json::json!({}),
            PRIORITY_LOW,
        );
        enqueue_job(
            &conn,
            &node.id,
            JobType::LesserTick,
            serde_json::json!({}),
            PRIORITY_HIGH,
        );
        enqueue_job(
            &conn,
            &node.id,
            JobType::LesserTick,
            serde_json::json!({}),
            PRIORITY_NORMAL,
        );

        // Should claim HIGH first
        let job = claim_job(&conn).unwrap().unwrap();
        assert_eq!(job.priority, PRIORITY_HIGH);
        mark_done(&conn, job.id).unwrap();

        // Then NORMAL
        let job = claim_job(&conn).unwrap().unwrap();
        assert_eq!(job.priority, PRIORITY_NORMAL);
        mark_done(&conn, job.id).unwrap();

        // Then LOW
        let job = claim_job(&conn).unwrap().unwrap();
        assert_eq!(job.priority, PRIORITY_LOW);
    }

    #[test]
    fn execute_lesser_tick_processes_catalyst() {
        let conn = setup_db();

        let node = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Test".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Enqueue a catalyst injection job
        let job_id = enqueue_job(
            &conn,
            &node.id,
            JobType::CatalystInjection,
            serde_json::json!({"catalyst_amount": 1.0}),
            PRIORITY_NORMAL,
        )
        .unwrap();

        // Claim and execute
        let job = claim_job(&conn).unwrap().unwrap();
        execute_job(&conn, &job).unwrap();
        mark_done(&conn, job.id).unwrap();

        // Verify the lesser cycle state was updated
        let state = lesser_cycle::load_state(&conn, &node.id).unwrap();
        assert_ne!(state.phase, LesserPhase::Dormant); // should have transitioned
        assert!(state.catalyst_pending > 0.0 || state.experience_accumulated > 0.0);
    }

    #[test]
    fn upward_pressure_enqueues_parent_jobs() {
        let conn = setup_db();

        // Create parent and child
        let parent = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Parent".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let child = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Child".to_string(),
                parent_ids: Some(vec![parent.id.clone()]),
                ..Default::default()
            },
        )
        .unwrap();

        // Inject a very large catalyst into the child to ensure upward pressure
        let _ = enqueue_job(
            &conn,
            &child.id,
            JobType::CatalystInjection,
            serde_json::json!({"catalyst_amount": 20.0}),
            PRIORITY_NORMAL,
        )
        .unwrap();

        // Claim and execute the child's job
        let job = claim_job(&conn).unwrap().unwrap();
        execute_job(&conn, &job).unwrap();
        mark_done(&conn, job.id).unwrap();

        // Process the child through the full cycle (multiple ticks)
        // Use HIGH priority for child ticks so they're always claimed before
        // any parent jobs that get enqueued via upward pressure.
        let mut reached_integrating = false;
        for i in 0..160 {
            let _ = enqueue_job(
                &conn,
                &child.id,
                JobType::LesserTick,
                serde_json::json!({"catalyst_amount": 0.0}),
                PRIORITY_HIGH, // high priority so child is always claimed first
            )
            .unwrap();
            let job = claim_job(&conn).unwrap().unwrap();
            execute_job(&conn, &job).unwrap();
            mark_done(&conn, job.id).unwrap();

            let state = crate::metabolism::load_state(&conn, &child.id).unwrap();
            if state.phase == crate::metabolism::LesserPhase::Quiescent {
                reached_integrating = true;
            }
            if state.phase == crate::metabolism::LesserPhase::Dormant && state.cycle_count > 0 {
                break;
            }
        }

        assert!(
            reached_integrating,
            "Child should have reached Integrating/Quiescent phase"
        );

        // Check if upward pressure enqueued a job for the parent.
        // The parent job is enqueued when the child transitions Integrating → Quiescent.
        // Since we used HIGH priority for child ticks, the parent job (NORMAL priority)
        // should still be in the queue.
        let parent_jobs: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pending_metabolism WHERE holon_id = ?1",
                rusqlite::params![parent.id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // With 20.0 catalyst, upward pressure should have fired
        assert!(
            parent_jobs > 0,
            "Expected upward pressure to enqueue parent jobs, got {}",
            parent_jobs
        );
    }

    #[test]
    fn failed_job_retries_until_max_attempts() {
        let conn = setup_db();

        let node = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Test".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Enqueue a job
        let _ = enqueue_job(
            &conn,
            &node.id,
            JobType::LesserTick,
            serde_json::json!({}),
            PRIORITY_NORMAL,
        )
        .unwrap();

        // Claim and fail 3 times
        for attempt in 1..=3 {
            let job = claim_job(&conn).unwrap().unwrap();
            assert_eq!(job.attempts, attempt);
            mark_failed(&conn, &job, "test failure").unwrap();
        }

        // After 3 attempts, the job should be in failed_metabolism, not pending
        assert_eq!(queue_depth(&conn).unwrap(), 0);

        let failed_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM failed_metabolism", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(failed_count, 1);
    }

    #[test]
    fn queue_depth_tracks_pending_jobs() {
        let conn = setup_db();

        let node = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Test".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(queue_depth(&conn).unwrap(), 0);

        enqueue_job(
            &conn,
            &node.id,
            JobType::LesserTick,
            serde_json::json!({}),
            PRIORITY_LOW,
        );
        enqueue_job(
            &conn,
            &node.id,
            JobType::LesserTick,
            serde_json::json!({}),
            PRIORITY_LOW,
        );

        assert_eq!(queue_depth(&conn).unwrap(), 2);

        // Claim one — the job is still in the table (attempts incremented)
        // but queue_depth counts it because attempts < max_attempts.
        let job = claim_job(&conn).unwrap().unwrap();
        // Both jobs still in queue (claimed job has attempts=1 < max_attempts=3)
        assert_eq!(queue_depth(&conn).unwrap(), 2);

        mark_done(&conn, job.id).unwrap();
        assert_eq!(queue_depth(&conn).unwrap(), 1); // one still pending
    }
}
