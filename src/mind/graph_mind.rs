//! Graph-Level Mind — the closed-loop integration pass (Phase 12).
//!
//! This is the Tier 3 "mind pipeline integration pass" from the computational
//! design doc. It reads graph-level health aggregates, diagnoses graph-level
//! patterns, and injects catalyst into specific holons to force integration.
//!
//! ## The Closed Loop
//!
//! ```text
//! Graph state → Diagnosis → Catalyst injection → Per-holon metabolism → Updated graph state
//!      ↑                                                                          │
//!      └────────────────────  next integration pass  ◄──────────────────────────┘
//! ```
//!
//! ## Graph-Level Diagnoses
//!
//! | Pattern | Condition | Action |
//! |---------|-----------|--------|
//! | PotentiatorHypoIngestion (GoldenAllergy) | observations > 10 × hypotheses | Inject catalyst into top observation clusters (force digestion cascade) |
//! | PotentiatorHyperIngestion (GoldenHyperIngestion) | hypotheses > 5 × evidence_count | Inject catalyst into most-resonated hypothesis (force grounding) |
//! | Graph-level depolarization | mean P_z < 10 | Inject catalyst into most-connected holon (force transformation pressure) |
//! | Graph-level collapse | mean G_z < 30 | Inject catalyst into orphans (force edge creation via entity extraction) |
//! | Stagnation | all holons dormant for > 3 cycles | Inject catalyst into most-recent observation (restart metabolism) |

use crate::error::TdgResult;
use crate::metabolism::worker::{enqueue_job, JobType, PRIORITY_HIGH, PRIORITY_NORMAL};

/// The result of a graph-level mind integration pass.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct MindIntegrationReport {
    /// Number of holons with health data.
    pub holons_with_health: usize,
    /// Mean G_z across all holons.
    pub mean_g_z: f64,
    /// Mean P_z across all holons.
    pub mean_p_z: f64,
    /// Number of depolarized holons (P_z < 10).
    pub depolarized_count: usize,
    /// Number of collapsed holons (G_z < 30).
    pub collapse_count: usize,
    /// Number of observations.
    pub observation_count: i64,
    /// Number of hypotheses.
    pub hypothesis_count: i64,
    /// Number of skills.
    pub skill_count: i64,
    /// Diagnoses made this pass.
    pub diagnoses: Vec<String>,
    /// Catalyst injections enqueued.
    pub injections_enqueued: usize,
    /// Timestamp.
    pub run_at: String,
}

/// Run one graph-level mind integration pass.
///
/// This is the closed loop — it reads the graph's metabolic state, diagnoses
/// patterns, and injects catalyst to force integration. Called by the Tier 3
/// scheduler every 15-30 minutes.
pub fn run_integration(conn: &rusqlite::Connection) -> TdgResult<MindIntegrationReport> {
    let mut report = MindIntegrationReport {
        run_at: crate::db::crud::now_iso(),
        ..Default::default()
    };

    // ─── 1. Load graph-level health aggregates ──────────────────────────────
    let health_data = load_graph_health(conn)?;
    report.holons_with_health = health_data.len();

    if !health_data.is_empty() {
        let g_z_sum: f64 = health_data.iter().map(|h| h.g_z).sum();
        let p_z_sum: f64 = health_data.iter().map(|h| h.p_z).sum();
        report.mean_g_z = g_z_sum / health_data.len() as f64;
        report.mean_p_z = p_z_sum / health_data.len() as f64;
        report.depolarized_count = health_data.iter().filter(|h| h.p_z < 10.0).count();
        report.collapse_count = health_data.iter().filter(|h| h.g_z < 30.0).count();
    }

    // ─── 2. Load node type counts ───────────────────────────────────────────
    report.observation_count = count_nodes(conn, "observation")?;
    report.hypothesis_count = count_nodes(conn, "hypothesis")?;
    report.skill_count = count_nodes(conn, "skill")?;

    // ─── 3. Diagnose graph-level patterns ───────────────────────────────────

    // Pattern 1: PotentiatorHypoIngestion (GoldenAllergy)
    // Many observations but few hypotheses → no emergence happening
    if report.observation_count > 10 && report.hypothesis_count == 0 {
        report.diagnoses.push(format!(
            "PotentiatorHypoIngestion (GoldenAllergy): {} observations, {} hypotheses — no emergence. Injecting catalyst into top observation clusters.",
            report.observation_count, report.hypothesis_count
        ));
        inject_catalyst_for_observation_clusters(conn, &mut report)?;
    } else if report.observation_count > 0
        && report.hypothesis_count > 0
        && (report.observation_count / report.hypothesis_count.max(1)) > 10
    {
        report.diagnoses.push(format!(
            "PotentiatorHypoIngestion (GoldenAllergy): observation/hypothesis ratio = {} — emergence starving. Injecting catalyst for digestion.",
            report.observation_count / report.hypothesis_count.max(1)
        ));
        inject_catalyst_for_observation_clusters(conn, &mut report)?;
    }

    // Pattern 2: PotentiatorHyperIngestion (GoldenHyperIngestion)
    // Many hypotheses but few evidence → speculation without grounding
    let evidence_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges WHERE edge_type = 'EVIDENCES' AND valid_to IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if report.hypothesis_count > 5 && evidence_count < report.hypothesis_count {
        report.diagnoses.push(format!(
            "PotentiatorHyperIngestion (GoldenHyperIngestion): {} hypotheses, {} evidence edges — speculation without grounding. Injecting catalyst into most-evidenced hypothesis.",
            report.hypothesis_count, evidence_count
        ));
        inject_catalyst_for_best_hypothesis(conn, &mut report)?;
    }

    // Pattern 3: Graph-level depolarization (mean P_z < 10)
    if report.holons_with_health > 0 && report.mean_p_z < 10.0 {
        report.diagnoses.push(format!(
            "Graph-level depolarization: mean P_z = {:.1} — no directional commitment across the graph. Injecting catalyst into most-connected holon.",
            report.mean_p_z
        ));
        inject_catalyst_for_most_connected(conn, &mut report)?;
    }

    // Pattern 4: Graph-level collapse (mean G_z < 30)
    if report.holons_with_health > 0 && report.mean_g_z < 30.0 {
        report.diagnoses.push(format!(
            "Graph-level collapse: mean G_z = {:.1} — severe boundary distortion. Injecting catalyst into orphan nodes.",
            report.mean_g_z
        ));
        inject_catalyst_for_orphans(conn, &mut report)?;
    }

    // Pattern 5: Stagnation — all holons dormant
    let mut dormant_count = 0;
    let mut active_count = 0;
    let mut stmt = conn.prepare(
        "SELECT lesser_cycle_json FROM nodes
         WHERE valid_to IS NULL AND lesser_cycle_json IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |row| {
        let s: String = row.get(0)?;
        Ok(s)
    })?;

    for r in rows {
        if let Ok(json_str) = r {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                if let Some(phase_str) = val.get("phase").and_then(|p| p.as_str()) {
                    if phase_str == "dormant" {
                        dormant_count += 1;
                    } else {
                        active_count += 1;
                    }
                }
            }
        }
    }

    if dormant_count > 5 && active_count == 0 {
        report.diagnoses.push(format!(
            "Stagnation: {} holons all dormant, 0 active. Injecting catalyst into most-recent observation to restart metabolism.",
            dormant_count
        ));
        inject_catalyst_for_most_recent(conn, &mut report)?;
    }

    // ─── 4. Log the integration pass ────────────────────────────────────────
    if !report.diagnoses.is_empty() {
        tracing::info!(
            "Graph mind integration: {} diagnoses, {} injections. mean_g_z={:.1}, mean_p_z={:.1}, obs={}, hyp={}, skills={}",
            report.diagnoses.len(),
            report.injections_enqueued,
            report.mean_g_z,
            report.mean_p_z,
            report.observation_count,
            report.hypothesis_count,
            report.skill_count
        );
    }

    Ok(report)
}

/// Load graph-level health data from all holons with health_json.
struct HealthData {
    g_z: f64,
    p_z: f64,
}

fn load_graph_health(conn: &rusqlite::Connection) -> TdgResult<Vec<HealthData>> {
    let mut stmt = match conn.prepare(
        "SELECT id, health_json FROM nodes
         WHERE valid_to IS NULL AND health_json IS NOT NULL AND health_json != ''",
    ) {
        Ok(s) => s,
        Err(_) => return Ok(Vec::new()),
    };

    let rows = stmt.query_map([], |row| {
        let _id: String = row.get(0)?;
        let json: String = row.get(1)?;
        Ok(json)
    })?;

    let mut results = Vec::new();
    for row in rows {
        if let Ok(json) = row {
            if let Ok(health) = serde_json::from_str::<crate::metabolism::health::Health>(&json) {
                results.push(HealthData {
                    g_z: health.g_z,
                    p_z: health.p_z,
                });
            }
        }
    }

    Ok(results)
}

/// Count nodes of a specific type.
fn count_nodes(conn: &rusqlite::Connection, node_type: &str) -> TdgResult<i64> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL AND node_type = ?1",
            rusqlite::params![node_type],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(count)
}

/// Inject catalyst into top observation clusters (force digestion cascade).
fn inject_catalyst_for_observation_clusters(
    conn: &rusqlite::Connection,
    report: &mut MindIntegrationReport,
) -> TdgResult<()> {
    // Find observations that share MENTIONS entities (potential clusters)
    let mut stmt = conn.prepare(
        "SELECT n.id
         FROM nodes n
         WHERE n.valid_to IS NULL AND n.node_type = 'observation'
         ORDER BY n.created_at DESC
         LIMIT 5",
    )?;

    let obs_ids: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for obs_id in &obs_ids {
        let payload = serde_json::json!({
            "catalyst_amount": 1.0,
            "source": "graph_mind_golden_allergy",
        });
        let _ = enqueue_job(
            conn,
            obs_id,
            JobType::CatalystInjection,
            payload,
            PRIORITY_HIGH,
        );
        report.injections_enqueued += 1;
    }

    Ok(())
}

/// Inject catalyst into the hypothesis with the most EVIDENCES edges.
fn inject_catalyst_for_best_hypothesis(
    conn: &rusqlite::Connection,
    report: &mut MindIntegrationReport,
) -> TdgResult<()> {
    let hyp_id: Option<String> = conn
        .query_row(
            "SELECT n.id
             FROM nodes n
             LEFT JOIN edges e ON e.source_id = n.id AND e.edge_type = 'EVIDENCES' AND e.valid_to IS NULL
             WHERE n.valid_to IS NULL AND n.node_type = 'hypothesis'
             GROUP BY n.id
             ORDER BY COUNT(e.id) DESC
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    if let Some(hyp_id) = hyp_id {
        let payload = serde_json::json!({
            "catalyst_amount": 2.0,
            "source": "graph_mind_golden_hyper_ingestion",
        });
        let _ = enqueue_job(
            conn,
            &hyp_id,
            JobType::CatalystInjection,
            payload,
            PRIORITY_HIGH,
        );
        report.injections_enqueued += 1;
    }

    Ok(())
}

/// Inject catalyst into the holon with the most edges (most-connected).
fn inject_catalyst_for_most_connected(
    conn: &rusqlite::Connection,
    report: &mut MindIntegrationReport,
) -> TdgResult<()> {
    let holon_id: Option<String> = conn
        .query_row(
            "SELECT n.id
             FROM nodes n
             LEFT JOIN edges e ON (e.source_id = n.id OR e.target_id = n.id) AND e.valid_to IS NULL
             WHERE n.valid_to IS NULL
             GROUP BY n.id
             ORDER BY COUNT(e.id) DESC
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    if let Some(holon_id) = holon_id {
        let payload = serde_json::json!({
            "catalyst_amount": 3.0,
            "source": "graph_mind_depolarization",
        });
        let _ = enqueue_job(
            conn,
            &holon_id,
            JobType::CatalystInjection,
            payload,
            PRIORITY_HIGH,
        );
        report.injections_enqueued += 1;
    }

    Ok(())
}

/// Inject catalyst into orphan nodes (nodes with no edges).
fn inject_catalyst_for_orphans(
    conn: &rusqlite::Connection,
    report: &mut MindIntegrationReport,
) -> TdgResult<()> {
    let mut stmt = conn.prepare(
        "SELECT n.id FROM nodes n
         WHERE n.valid_to IS NULL
           AND n.id NOT IN (SELECT source_id FROM edges WHERE valid_to IS NULL)
           AND n.id NOT IN (SELECT target_id FROM edges WHERE valid_to IS NULL)
         LIMIT 3",
    )?;

    let orphan_ids: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for orphan_id in &orphan_ids {
        let payload = serde_json::json!({
            "catalyst_amount": 1.5,
            "source": "graph_mind_collapse",
        });
        let _ = enqueue_job(
            conn,
            orphan_id,
            JobType::CatalystInjection,
            payload,
            PRIORITY_NORMAL,
        );
        report.injections_enqueued += 1;
    }

    Ok(())
}

/// Inject catalyst into the most recently created observation.
fn inject_catalyst_for_most_recent(
    conn: &rusqlite::Connection,
    report: &mut MindIntegrationReport,
) -> TdgResult<()> {
    let obs_id: Option<String> = conn
        .query_row(
            "SELECT id FROM nodes WHERE valid_to IS NULL ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    if let Some(obs_id) = obs_id {
        let payload = serde_json::json!({
            "catalyst_amount": 2.0,
            "source": "graph_mind_stagnation",
        });
        let _ = enqueue_job(
            conn,
            &obs_id,
            JobType::CatalystInjection,
            payload,
            PRIORITY_HIGH,
        );
        report.injections_enqueued += 1;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_fts, init_schema, run_migrations};
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
    fn integration_pass_empty_graph() {
        let conn = setup_db();
        let report = run_integration(&conn).unwrap();

        assert_eq!(report.holons_with_health, 0);
        assert_eq!(report.injections_enqueued, 0);
        assert!(report.diagnoses.is_empty());
    }

    #[test]
    fn integration_pass_detects_golden_allergy() {
        let conn = setup_db();

        // Create 15 observations, 0 hypotheses → GoldenAllergy
        for i in 0..15 {
            let _ = crate::db::crud::add_node(
                &conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("Observation {}", i),
                    ..Default::default()
                },
            )
            .unwrap();
        }

        let report = run_integration(&conn).unwrap();

        assert_eq!(report.observation_count, 15);
        assert_eq!(report.hypothesis_count, 0);
        assert!(
            report
                .diagnoses
                .iter()
                .any(|d| d.contains("PotentiatorHypoIngestion")),
            "Should diagnose GoldenAllergy: {:?}",
            report.diagnoses
        );
        assert!(report.injections_enqueued > 0);
    }

    #[test]
    fn integration_pass_detects_golden_hyper_ingestion() {
        let conn = setup_db();

        // Create 6 hypotheses, 0 evidence → GoldenHyperIngestion
        for i in 0..6 {
            let _ = crate::db::crud::add_node(
                &conn,
                &NewNode {
                    node_type: "hypothesis".to_string(),
                    name: format!("Hypothesis {}", i),
                    ..Default::default()
                },
            )
            .unwrap();
        }

        let report = run_integration(&conn).unwrap();

        assert!(
            report
                .diagnoses
                .iter()
                .any(|d| d.contains("PotentiatorHyperIngestion")),
            "Should diagnose GoldenHyperIngestion: {:?}",
            report.diagnoses
        );
        assert!(report.injections_enqueued > 0);
    }

    #[test]
    fn integration_pass_enqueues_catalyst_jobs() {
        let conn = setup_db();

        // Create observations to trigger GoldenAllergy
        for i in 0..15 {
            let _ = crate::db::crud::add_node(
                &conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: format!("Obs {}", i),
                    ..Default::default()
                },
            )
            .unwrap();
        }

        let report = run_integration(&conn).unwrap();

        // Verify jobs were enqueued
        let queue_depth: i64 = conn
            .query_row("SELECT COUNT(*) FROM pending_metabolism", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        assert!(queue_depth > 0, "Should have enqueued metabolism jobs");
        assert!(report.injections_enqueued > 0);
    }

    #[test]
    fn integration_pass_no_diagnosis_on_healthy_graph() {
        let conn = setup_db();

        // Create a balanced graph: 3 observations, 1 hypothesis, 1 evidence edge
        let obs1 = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Obs 1".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let obs2 = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Obs 2".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let obs3 = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Obs 3".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let hyp = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "hypothesis".to_string(),
                name: "Hyp 1".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Wire evidence
        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: hyp.id.clone(),
                target_id: obs1.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        );

        let report = run_integration(&conn).unwrap();

        // With 3 obs and 1 hyp, ratio = 3 — below the 10x threshold
        // No diagnoses expected
        assert!(
            report.diagnoses.is_empty(),
            "Healthy graph should have no diagnoses: {:?}",
            report.diagnoses
        );
    }
}
