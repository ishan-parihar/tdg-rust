//! TDG Knowledge Digestion & Hygiene Engine v2.0
//!
//! Port of `core/knowledge/tdg_knowledge_engine.py`.
//!
//! Manages the catalyst lifecycle:
//! `raw → classified → linked → evaluated → integrated → archived/discarded`
//!
//! Also handles graph hygiene: orphan detection, dangling edge pruning,
//! stale node archival, and hygiene reporting.

use std::collections::HashMap;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db::crud::{get_edges, get_node, now_iso, record_event};
use crate::error::TdgResult;
use crate::models::Node;

// ─── Constants ───────────────────────────────────────────────────────────────

pub const DEFAULT_ORPHAN_THRESHOLD_DAYS: i64 = 30;
pub const DEFAULT_STALE_THRESHOLD_DAYS: i64 = 60;
pub const DEFAULT_INTEGRATION_DECAY_DAYS: i64 = 14;

/// Structural node types that catalysts can link to.
pub const STRUCTURAL_TARGET_TYPES: &[&str] = &["hypothesis", "constraint", "telos"];

/// Edge types used for catalyst linkage validation.
pub const CATALYST_LINK_EDGES: &[&str] = &[
    "EVIDENCES",
    "SUPPORTS",
    "CONTEXT",
    "DIGESTS_TO",
    "RELATES_TO",
];

// ─── Data Models ─────────────────────────────────────────────────────────────

/// Catalyst type classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalystType {
    Signal,
    Insight,
    Feedback,
    Metric,
    Observation,
    Unknown,
}

impl CatalystType {
    pub fn as_str(&self) -> &str {
        match self {
            CatalystType::Signal => "signal",
            CatalystType::Insight => "insight",
            CatalystType::Feedback => "feedback",
            CatalystType::Metric => "metric",
            CatalystType::Observation => "observation",
            CatalystType::Unknown => "unknown",
        }
    }

    pub fn parse_from_str(s: &str) -> Self {
        match s {
            "signal" => CatalystType::Signal,
            "insight" => CatalystType::Insight,
            "feedback" => CatalystType::Feedback,
            "metric" => CatalystType::Metric,
            "observation" => CatalystType::Observation,
            _ => CatalystType::Unknown,
        }
    }
}

/// Lifecycle status of a catalyst.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalystStatus {
    Raw,
    Classified,
    Linked,
    Evaluated,
    Integrated,
    Archived,
    Discarded,
}

impl CatalystStatus {
    pub fn as_str(&self) -> &str {
        match self {
            CatalystStatus::Raw => "raw",
            CatalystStatus::Classified => "classified",
            CatalystStatus::Linked => "linked",
            CatalystStatus::Evaluated => "evaluated",
            CatalystStatus::Integrated => "integrated",
            CatalystStatus::Archived => "archived",
            CatalystStatus::Discarded => "discarded",
        }
    }
}

/// Decay policy for catalyst archival.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayPolicy {
    pub archive_after_days: i64,
    pub stale_after_days: i64,
}

impl Default for DecayPolicy {
    fn default() -> Self {
        Self {
            archive_after_days: DEFAULT_STALE_THRESHOLD_DAYS,
            stale_after_days: DEFAULT_ORPHAN_THRESHOLD_DAYS,
        }
    }
}

/// Enrichment profile for a catalyst node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalystProfile {
    pub node_id: String,
    pub catalyst_type: CatalystType,
    pub status: CatalystStatus,
    pub integration_quality: f64,
    pub archive_after: Option<String>,
    pub linked_hypotheses: Vec<String>,
    pub linked_constraints: Vec<String>,
    pub linked_teloi: Vec<String>,
    pub staleness_days: Option<i64>,
}

impl CatalystProfile {
    pub fn new(node_id: &str) -> Self {
        Self {
            node_id: node_id.to_string(),
            catalyst_type: CatalystType::Unknown,
            status: CatalystStatus::Raw,
            integration_quality: 0.0,
            archive_after: None,
            linked_hypotheses: Vec::new(),
            linked_constraints: Vec::new(),
            linked_teloi: Vec::new(),
            staleness_days: None,
        }
    }
}

/// Full archival record for a node moved to archives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivalRecord {
    pub node_id: String,
    pub node_type: String,
    pub name: String,
    pub properties: serde_json::Value,
    pub parent_ids: Vec<String>,
    pub archived_at: String,
    pub archival_reason: String,
    pub created_at: String,
    /// Edge types and targets this node was connected to before archival.
    pub edge_history: Vec<serde_json::Value>,
    /// IDs of nodes this catalyst was linked to (hypotheses, constraints, teloi).
    pub linkage_history: Vec<String>,
}

/// Hygiene report for the entire graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HygieneReport {
    pub total_nodes: i64,
    pub total_edges: i64,
    pub total_observations: i64,
    pub orphan_count: i64,
    pub orphan_ids: Vec<String>,
    pub stale_count: i64,
    pub stale_ids: Vec<String>,
    pub dangling_edge_count: i64,
    pub dangling_edge_ids: Vec<String>,
    pub recently_archived: i64,
    pub lifecycle_distribution: HashMap<String, i64>,
    pub recommendations: Vec<String>,
}

// ─── Catalyst Lifecycle ──────────────────────────────────────────────────────

/// Infer catalyst type from node metadata.
fn infer_catalyst_type(node: &Node) -> CatalystType {
    let name_lower = node.name.to_lowercase();
    let desc_lower = node.description.to_lowercase();
    let combined = format!("{name_lower} {desc_lower}");

    // Keyword matching heuristic
    if combined.contains("signal") || combined.contains("alert") || combined.contains("warning") {
        CatalystType::Signal
    } else if combined.contains("insight")
        || combined.contains("discovery")
        || combined.contains("realization")
    {
        CatalystType::Insight
    } else if combined.contains("feedback")
        || combined.contains("review")
        || combined.contains("comment")
    {
        CatalystType::Feedback
    } else if combined.contains("metric")
        || combined.contains("measure")
        || combined.contains("stat")
    {
        CatalystType::Metric
    } else if node.node_type == "observation" {
        CatalystType::Observation
    } else {
        CatalystType::Unknown
    }
}

/// Phase 1: Classify a raw catalyst node.
///
/// Transitions: raw → classified
pub fn classify_catalyst(conn: &Connection, node_id: &str) -> TdgResult<serde_json::Value> {
    let node = get_node(conn, node_id)?
        .ok_or_else(|| crate::error::TdgError::Custom(format!("Node {node_id} not found")))?;

    if node.node_type != "observation" {
        return Err(crate::error::TdgError::Custom(
            "Only observation nodes can be classified as catalysts".to_string(),
        ));
    }

    let catalyst_type = infer_catalyst_type(&node);
    let policy = DecayPolicy::default();

    // Compute archive_after from created_at
    let archive_after = if let Ok(created) = chrono::NaiveDateTime::parse_from_str(
        node.created_at.replace('Z', "").as_str(),
        "%Y-%m-%dT%H:%M:%S%.f",
    ) {
        let archive_date = created + chrono::Duration::days(policy.archive_after_days);
        Some(archive_date.format("%Y-%m-%dT%H:%M:%SZ").to_string())
    } else {
        None
    };

    // Update node properties with catalyst metadata
    let mut properties = node.properties.clone();
    if let Some(obj) = properties.as_object_mut() {
        obj.insert(
            "catalyst_type".to_string(),
            serde_json::json!(catalyst_type.as_str()),
        );
        obj.insert(
            "catalyst_status".to_string(),
            serde_json::json!("classified"),
        );
        if let Some(ref aa) = archive_after {
            obj.insert("archive_after".to_string(), serde_json::json!(aa));
        }
    }

    let now = now_iso();
    conn.execute(
        "UPDATE nodes SET properties_json = ?1, updated_at = ?2 WHERE id = ?3 AND valid_to IS NULL",
        params![properties.to_string(), now, node_id],
    )?;

    record_event(
        conn,
        "catalyst_classified",
        Some(node_id),
        None,
        None,
        Some(&serde_json::json!({
            "catalyst_type": catalyst_type.as_str(),
            "archive_after": archive_after,
        })),
    )?;

    Ok(serde_json::json!({
        "node_id": node_id,
        "catalyst_type": catalyst_type.as_str(),
        "status": "classified",
        "archive_after": archive_after,
    }))
}

/// Phase 2: Link catalyst to structural nodes in the graph.
///
/// Analyzes outgoing edges to find connections to hypotheses, constraints, teloi.
pub fn link_catalyst_to_structure(
    conn: &Connection,
    node_id: &str,
) -> TdgResult<serde_json::Value> {
    let node = get_node(conn, node_id)?
        .ok_or_else(|| crate::error::TdgError::Custom(format!("Node {node_id} not found")))?;

    let outgoing = get_edges(conn, Some(node_id), None, None, None, 500)?;

    let mut linked_hypotheses = Vec::new();
    let mut linked_constraints = Vec::new();
    let mut linked_teloi = Vec::new();
    let mut valid_links = 0;
    let mut total_links = 0;

    for edge in &outgoing {
        total_links += 1;

        // Check if this edge connects to a structural node
        if let Some(target) = get_node(conn, &edge.target_id)? {
            match target.node_type.as_str() {
                "hypothesis" => linked_hypotheses.push(edge.target_id.clone()),
                "constraint" => linked_constraints.push(edge.target_id.clone()),
                "telos" => linked_teloi.push(edge.target_id.clone()),
                _ => {}
            }
            if CATALYST_LINK_EDGES.contains(&edge.edge_type.as_str()) {
                valid_links += 1;
            }
        }
    }

    // Update node properties
    let mut properties = node.properties.clone();
    if let Some(obj) = properties.as_object_mut() {
        obj.insert("catalyst_status".to_string(), serde_json::json!("linked"));
        obj.insert(
            "linked_hypotheses".to_string(),
            serde_json::json!(linked_hypotheses),
        );
        obj.insert(
            "linked_constraints".to_string(),
            serde_json::json!(linked_constraints),
        );
        obj.insert("linked_teloi".to_string(), serde_json::json!(linked_teloi));
        obj.insert("link_count".to_string(), serde_json::json!(valid_links));
    }

    let now = now_iso();
    conn.execute(
        "UPDATE nodes SET properties_json = ?1, updated_at = ?2 WHERE id = ?3 AND valid_to IS NULL",
        params![properties.to_string(), now, node_id],
    )?;

    record_event(
        conn,
        "catalyst_linked",
        Some(node_id),
        None,
        None,
        Some(&serde_json::json!({
            "hypotheses": linked_hypotheses,
            "constraints": linked_constraints,
            "teloi": linked_teloi,
            "valid_links": valid_links,
        })),
    )?;

    Ok(serde_json::json!({
        "node_id": node_id,
        "status": "linked",
        "hypotheses": linked_hypotheses,
        "constraints": linked_constraints,
        "teloi": linked_teloi,
        "valid_links": valid_links,
        "total_links": total_links,
    }))
}

/// Phase 3: Evaluate integration quality of a catalyst.
///
/// Score: 0.0 (isolated) → 1.0 (deeply integrated).
/// Factors: edge ratio, structural links, time-decay.
pub fn evaluate_integration_quality(
    conn: &Connection,
    node_id: &str,
) -> TdgResult<serde_json::Value> {
    let node = get_node(conn, node_id)?
        .ok_or_else(|| crate::error::TdgError::Custom(format!("Node {node_id} not found")))?;

    let properties = &node.properties;

    // Extract linkage data from properties
    let _link_count = properties
        .get("link_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as f64;

    let hyp_count = properties
        .get("linked_hypotheses")
        .and_then(|v| v.as_array())
        .map_or(0.0, |a| a.len() as f64);

    let con_count = properties
        .get("linked_constraints")
        .and_then(|v| v.as_array())
        .map_or(0.0, |a| a.len() as f64);

    let telos_count = properties
        .get("linked_teloi")
        .and_then(|v| v.as_array())
        .map_or(0.0, |a| a.len() as f64);

    // Score components
    // 1. Structural link ratio (0.0-0.4): weighted links / max possible
    let structural_score = ((hyp_count * 1.0 + con_count * 0.8 + telos_count * 1.5) / 5.0).min(0.4);

    // 2. Edge density (0.0-0.3): how many edges this node has
    let outgoing = get_edges(conn, Some(node_id), None, None, None, 500)?;
    let incoming = get_edges(conn, None, Some(node_id), None, None, 500)?;
    let total_edges = (outgoing.len() + incoming.len()) as f64;
    let density_score = (total_edges / 10.0).min(0.3);

    // 3. Time-decay (0.0-0.3): newer nodes get a bonus
    let age_score = if let Ok(created) = chrono::NaiveDateTime::parse_from_str(
        node.created_at.replace('Z', "").as_str(),
        "%Y-%m-%dT%H:%M:%S%.f",
    ) {
        let now = chrono::Utc::now().naive_utc();
        let age_days = (now - created).num_days() as f64;
        // Decay over 14 days
        (1.0 - (age_days / DEFAULT_INTEGRATION_DECAY_DAYS as f64).min(1.0)) * 0.3
    } else {
        0.15 // default mid-score
    };

    let integration_quality = structural_score + density_score + age_score;

    // Determine status based on quality
    let new_status = if integration_quality >= 0.7 {
        "integrated"
    } else if integration_quality >= 0.3 {
        "evaluated"
    } else {
        "linked"
    };

    // Update node properties
    let mut properties = node.properties.clone();
    if let Some(obj) = properties.as_object_mut() {
        obj.insert(
            "integration_quality".to_string(),
            serde_json::json!((integration_quality * 1000.0).round() / 1000.0),
        );
        obj.insert("catalyst_status".to_string(), serde_json::json!(new_status));
    }

    let now = now_iso();
    conn.execute(
        "UPDATE nodes SET properties_json = ?1, updated_at = ?2 WHERE id = ?3 AND valid_to IS NULL",
        params![properties.to_string(), now, node_id],
    )?;

    record_event(
        conn,
        "catalyst_evaluated",
        Some(node_id),
        None,
        None,
        Some(&serde_json::json!({
            "integration_quality": integration_quality,
            "structural_score": structural_score,
            "density_score": density_score,
            "age_score": age_score,
            "new_status": new_status,
        })),
    )?;

    Ok(serde_json::json!({
        "node_id": node_id,
        "integration_quality": (integration_quality * 1000.0).round() / 1000.0,
        "structural_score": structural_score,
        "density_score": density_score,
        "age_score": age_score,
        "status": new_status,
    }))
}

/// End-to-end catalyst lifecycle: classify → link → evaluate.
pub fn process_catalyst_lifecycle(
    conn: &Connection,
    node_id: &str,
) -> TdgResult<serde_json::Value> {
    let classified = classify_catalyst(conn, node_id)?;
    let linked = link_catalyst_to_structure(conn, node_id)?;
    let evaluated = evaluate_integration_quality(conn, node_id)?;

    Ok(serde_json::json!({
        "node_id": node_id,
        "classified": classified,
        "linked": linked,
        "evaluated": evaluated,
        "status": "lifecycle_complete",
    }))
}

// ─── Graph Hygiene ───────────────────────────────────────────────────────────

/// Detect orphan nodes: nodes with no active edges.
pub fn detect_orphans(conn: &Connection) -> TdgResult<serde_json::Value> {
    let mut stmt =
        conn.prepare("SELECT id, node_type, name, created_at FROM nodes WHERE valid_to IS NULL")?;

    let rows: Vec<(String, String, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut disconnected = Vec::new();
    let mut unlinked_observations = Vec::new();

    for (id, node_type, name, created_at) in &rows {
        // Count edges (both directions)
        let edge_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE (source_id = ?1 OR target_id = ?1) AND valid_to IS NULL",
            params![id],
            |row| row.get(0),
        )?;

        if edge_count == 0 {
            let age_days = chrono::NaiveDateTime::parse_from_str(
                created_at.replace('Z', "").as_str(),
                "%Y-%m-%dT%H:%M:%S%.f",
            )
            .ok()
            .map(|created| {
                let now = chrono::Utc::now().naive_utc();
                (now - created).num_days()
            })
            .unwrap_or(0);

            let severity = if age_days > DEFAULT_ORPHAN_THRESHOLD_DAYS {
                "critical"
            } else {
                "warning"
            };

            disconnected.push(serde_json::json!({
                "node_id": id,
                "node_type": node_type,
                "name": name,
                "edge_count": edge_count,
                "age_days": age_days,
                "severity": severity,
            }));
        } else if node_type == "observation" {
            // Check for structural links
            let structural_links: i64 = conn.query_row(
                "SELECT COUNT(*) FROM edges e JOIN nodes n ON e.target_id = n.id
                 WHERE e.source_id = ?1 AND e.valid_to IS NULL
                 AND n.node_type IN ('hypothesis', 'constraint', 'telos')",
                params![id],
                |row| row.get(0),
            )?;

            if structural_links == 0 {
                unlinked_observations.push(serde_json::json!({
                    "node_id": id,
                    "name": name,
                    "total_edges": edge_count,
                    "structural_links": 0,
                }));
            }
        }
    }

    Ok(serde_json::json!({
        "disconnected": disconnected,
        "unlinked_observations": unlinked_observations,
        "total_disconnected": disconnected.len(),
        "total_unlinked_observations": unlinked_observations.len(),
    }))
}

/// Prune edges that point to non-existent (hard-deleted) nodes.
pub fn prune_dangling_edges(conn: &Connection) -> TdgResult<serde_json::Value> {
    let mut pruned = 0i64;
    let mut pruned_ids = Vec::new();

    // Find edges with dangling source_id
    let mut stmt = conn.prepare(
        "SELECT e.id FROM edges e LEFT JOIN nodes n ON e.source_id = n.id
         WHERE n.id IS NULL AND e.valid_to IS NULL",
    )?;
    let dangling_source: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for eid in dangling_source {
        let now = now_iso();
        conn.execute(
            "UPDATE edges SET valid_to = ?1 WHERE id = ?2",
            params![now, eid],
        )?;
        pruned += 1;
        pruned_ids.push(eid);
    }

    // Find edges with dangling target_id
    let mut stmt = conn.prepare(
        "SELECT e.id FROM edges e LEFT JOIN nodes n ON e.target_id = n.id
         WHERE n.id IS NULL AND e.valid_to IS NULL",
    )?;
    let dangling_target: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for eid in dangling_target {
        let now = now_iso();
        conn.execute(
            "UPDATE edges SET valid_to = ?1 WHERE id = ?2",
            params![now, eid],
        )?;
        pruned += 1;
        pruned_ids.push(eid);
    }

    if pruned > 0 {
        record_event(
            conn,
            "dangling_edges_pruned",
            None,
            None,
            None,
            Some(&serde_json::json!({
                "pruned_count": pruned,
                "edge_ids": pruned_ids,
            })),
        )?;
    }

    Ok(serde_json::json!({
        "pruned_count": pruned,
        "edge_ids": pruned_ids,
    }))
}

/// Archive stale nodes that have passed their archive_after deadline.
pub fn archive_stale_nodes(
    conn: &Connection,
    days_threshold: Option<i64>,
) -> TdgResult<serde_json::Value> {
    let threshold = days_threshold.unwrap_or(DEFAULT_STALE_THRESHOLD_DAYS);
    let mut archived = 0i64;
    let mut archived_ids = Vec::new();

    let mut stmt = conn.prepare(
        "SELECT id, name, node_type, properties_json, parent_ids, created_at
         FROM nodes WHERE valid_to IS NULL AND node_type = 'observation'",
    )?;

    let rows: Vec<(String, String, String, String, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let now = chrono::Utc::now().naive_utc();

    for (id, name, node_type, properties_json, _parent_ids_json, created_at) in &rows {
        let properties: serde_json::Value =
            serde_json::from_str(properties_json).unwrap_or(serde_json::json!({}));

        let should_archive =
            if let Some(archive_after) = properties.get("archive_after").and_then(|v| v.as_str()) {
                if let Ok(aa) = chrono::NaiveDateTime::parse_from_str(
                    archive_after.replace('Z', "").as_str(),
                    "%Y-%m-%dT%H:%M:%S%.f",
                ) {
                    now > aa
                } else {
                    false
                }
            } else {
                // Check age-based staleness
                if let Ok(created) = chrono::NaiveDateTime::parse_from_str(
                    created_at.replace('Z', "").as_str(),
                    "%Y-%m-%dT%H:%M:%S%.f",
                ) {
                    let age_days = (now - created).num_days();
                    age_days > threshold
                } else {
                    false
                }
            };

        if should_archive {
            // Soft-archived: update lifecycle_state
            let now_str = now_iso();
            conn.execute(
                "UPDATE nodes SET lifecycle_state = 'archived', updated_at = ?1 WHERE id = ?2",
                params![now_str, id],
            )?;

            record_event(
                conn,
                "node_archived",
                Some(id),
                None,
                None,
                Some(&serde_json::json!({
                    "name": name,
                    "node_type": node_type,
                    "reason": "stale_or_expired",
                })),
            )?;

            archived += 1;
            archived_ids.push(id.clone());
        }
    }

    Ok(serde_json::json!({
        "archived_count": archived,
        "archived_ids": archived_ids,
    }))
}

/// Enforce observation lifecycle: archive critical orphan observations.
pub fn enforce_observation_lifecycle(conn: &Connection) -> TdgResult<serde_json::Value> {
    let orphans = detect_orphans(conn)?;
    let disconnected = orphans
        .get("disconnected")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut enforced = 0i64;
    let mut enforced_ids = Vec::new();

    for orphan in &disconnected {
        let severity = orphan
            .get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let node_id = orphan.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
        let node_type = orphan
            .get("node_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if severity == "critical" && node_type == "observation" {
            // Archive this critical orphan
            let now = now_iso();
            conn.execute(
                "UPDATE nodes SET lifecycle_state = 'archived', updated_at = ?1 WHERE id = ?2",
                params![now, node_id],
            )?;

            record_event(
                conn,
                "observation_archived_lifecycle",
                Some(node_id),
                None,
                None,
                Some(&serde_json::json!({
                    "reason": "critical_orphan",
                })),
            )?;

            enforced += 1;
            enforced_ids.push(node_id.to_string());
        }
    }

    Ok(serde_json::json!({
        "enforced_count": enforced,
        "enforced_ids": enforced_ids,
    }))
}

/// Reverse archival: restore an archived node to active status.
pub fn reverse_archival(conn: &Connection, node_id: &str) -> TdgResult<serde_json::Value> {
    let node = get_node(conn, node_id)?
        .or_else(|| {
            // Try to find even if soft-deleted (archived might mean lifecycle_state = 'archived')
            get_node_including_deleted(conn, node_id).ok().flatten()
        })
        .ok_or_else(|| crate::error::TdgError::Custom(format!("Node {node_id} not found")))?;

    let now = now_iso();

    // Reset lifecycle_state to active
    conn.execute(
        "UPDATE nodes SET lifecycle_state = 'active', valid_to = NULL, updated_at = ?1 WHERE id = ?2",
        params![now, node_id],
    )?;

    record_event(
        conn,
        "node_archival_reversed",
        Some(node_id),
        None,
        None,
        Some(&serde_json::json!({
            "previous_state": node.lifecycle_state,
        })),
    )?;

    Ok(serde_json::json!({
        "node_id": node_id,
        "status": "restored",
        "lifecycle_state": "active",
    }))
}

/// Generate a comprehensive hygiene report.
pub fn generate_hygiene_report(conn: &Connection) -> TdgResult<HygieneReport> {
    // Basic counts
    let total_nodes: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL",
        [],
        |row| row.get(0),
    )?;
    let total_edges: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges WHERE valid_to IS NULL",
        [],
        |row| row.get(0),
    )?;
    let total_observations: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL AND node_type = 'observation'",
        [],
        |row| row.get(0),
    )?;

    // Orphans
    let orphans = detect_orphans(conn)?;
    let orphan_ids: Vec<String> = orphans
        .get("disconnected")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("node_id")?.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Dangling edges
    let mut stmt = conn.prepare(
        "SELECT e.id FROM edges e
         LEFT JOIN nodes ns ON e.source_id = ns.id
         LEFT JOIN nodes nt ON e.target_id = nt.id
         WHERE (ns.id IS NULL OR nt.id IS NULL) AND e.valid_to IS NULL",
    )?;
    let dangling_edge_ids: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    // Stale nodes
    let stale_result = archive_stale_nodes(conn, Some(DEFAULT_STALE_THRESHOLD_DAYS))?;
    let stale_ids: Vec<String> = stale_result
        .get("archived_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Recently archived
    let recently_archived: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL AND lifecycle_state = 'archived'",
        [],
        |row| row.get(0),
    )?;

    // Lifecycle distribution
    let mut stmt = conn.prepare(
        "SELECT lifecycle_state, COUNT(*) FROM nodes WHERE valid_to IS NULL GROUP BY lifecycle_state",
    )?;
    let lifecycle_distribution: HashMap<String, i64> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Recommendations
    let mut recommendations = Vec::new();
    if orphan_ids.len() > 5 {
        recommendations.push(format!(
            "Consider removing {} disconnected nodes or linking them to the graph",
            orphan_ids.len()
        ));
    }
    if !dangling_edge_ids.is_empty() {
        recommendations.push(format!(
            "Prune {} dangling edges pointing to deleted nodes",
            dangling_edge_ids.len()
        ));
    }
    if total_observations > 0 && total_observations as f64 / total_nodes as f64 > 0.7 {
        recommendations.push(
            "High ratio of observations — consider converting some to insights or hypotheses"
                .to_string(),
        );
    }

    Ok(HygieneReport {
        total_nodes,
        total_edges,
        total_observations,
        orphan_count: orphan_ids.len() as i64,
        orphan_ids,
        stale_count: stale_ids.len() as i64,
        stale_ids,
        dangling_edge_count: dangling_edge_ids.len() as i64,
        dangling_edge_ids,
        recently_archived,
        lifecycle_distribution,
        recommendations,
    })
}

/// Combined hygiene pipeline: prune dangling → archive stale → enforce lifecycle → report.
pub fn run_full_hygiene_cycle(conn: &Connection, lean: bool) -> TdgResult<HygieneReport> {
    let pruned = prune_dangling_edges(conn)?;
    let pruned_count = pruned
        .get("pruned_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let archived = if lean {
        serde_json::json!({"archived_count": 0, "archived_ids": []})
    } else {
        archive_stale_nodes(conn, None)?
    };
    let archived_count = archived
        .get("archived_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let enforced = if lean {
        serde_json::json!({"enforced_count": 0, "enforced_ids": []})
    } else {
        enforce_observation_lifecycle(conn)?
    };
    let enforced_count = enforced
        .get("enforced_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let mut report = generate_hygiene_report(conn)?;

    report.recommendations.insert(0, format!(
        "Hygiene cycle complete: {pruned_count} dangling edges pruned, {archived_count} stale nodes archived, {enforced_count} critical orphans enforced"
    ));

    record_event(
        conn,
        "hygiene_cycle_complete",
        None,
        None,
        None,
        Some(&serde_json::json!({
            "pruned": pruned_count,
            "archived": archived_count,
            "enforced": enforced_count,
            "lean": lean,
        })),
    )?;

    Ok(report)
}

use crate::db::crud::get_node_including_deleted;

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

    fn add_observation(conn: &Connection, name: &str) -> Node {
        crate::db::crud::add_node(
            conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: name.to_string(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn infer_catalyst_type_test() {
        let node = Node {
            id: "test".to_string(),
            node_type: "observation".to_string(),
            name: "Signal Alert: High CPU".to_string(),
            description: "".to_string(),
            properties: serde_json::json!({}),
            quadrants: serde_json::json!({}),
            drives: serde_json::json!({}),
            lifecycle_state: "active".to_string(),
            teleological_level: None,
            developmental_stage: None,
            confidence: 1.0,
            source: "".to_string(),
            parent_ids: vec![],
            agent_path: "".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
            valid_from: None,
            valid_to: None,
            helpful_count: 0,
            retrieval_count: 0,
            agent_id: None,
        };
        assert_eq!(infer_catalyst_type(&node), CatalystType::Signal);
    }

    #[test]
    fn classify_catalyst_basic() {
        let conn = setup_db();
        let obs = add_observation(&conn, "Insight: Rust is fast");

        let result = classify_catalyst(&conn, &obs.id).unwrap();
        assert_eq!(result["node_id"], obs.id);
        assert_eq!(result["status"], "classified");
    }

    #[test]
    fn classify_rejects_non_observation() {
        let conn = setup_db();
        let telos = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Not Observation".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(classify_catalyst(&conn, &telos.id).is_err());
    }

    #[test]
    fn link_catalyst_basic() {
        let conn = setup_db();
        let obs = add_observation(&conn, "Feedback Node");
        let hyp = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "hypothesis".to_string(),
                name: "Test Hypothesis".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Link observation → hypothesis
        crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: obs.id.clone(),
                target_id: hyp.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let result = link_catalyst_to_structure(&conn, &obs.id).unwrap();
        assert_eq!(result["status"], "linked");
        let hyps = result["hypotheses"].as_array().unwrap();
        assert_eq!(hyps.len(), 1);
    }

    #[test]
    fn evaluate_integration_basic() {
        let conn = setup_db();
        let obs = add_observation(&conn, "Well Connected");
        let hyp = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "hypothesis".to_string(),
                name: "H1".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: obs.id.clone(),
                target_id: hyp.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // First link, then evaluate
        link_catalyst_to_structure(&conn, &obs.id).unwrap();
        let result = evaluate_integration_quality(&conn, &obs.id).unwrap();
        assert!(result["integration_quality"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn detect_orphans_basic() {
        let conn = setup_db();
        add_observation(&conn, "Orphan Node");
        add_observation(&conn, "Connected Node");

        let result = detect_orphans(&conn).unwrap();
        let disconnected = result["disconnected"].as_array().unwrap();
        assert!(!disconnected.is_empty());
    }

    #[test]
    fn prune_dangling_edges_basic() {
        let conn = setup_db();
        let obs = add_observation(&conn, "Source");
        let target = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "hypothesis".to_string(),
                name: "Target".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let _edge = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: obs.id.clone(),
                target_id: target.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Temporarily disable FK constraints to delete node while leaving edge dangling
        conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
        conn.execute("DELETE FROM nodes WHERE id = ?1", params![target.id])
            .unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();

        let result = prune_dangling_edges(&conn).unwrap();
        assert_eq!(result["pruned_count"], 1);
    }

    #[test]
    fn reverse_archival_basic() {
        let conn = setup_db();
        let obs = add_observation(&conn, "Archived Node");

        // Archive it
        conn.execute(
            "UPDATE nodes SET lifecycle_state = 'archived' WHERE id = ?1",
            params![obs.id],
        )
        .unwrap();

        let result = reverse_archival(&conn, &obs.id).unwrap();
        assert_eq!(result["status"], "restored");
        assert_eq!(result["lifecycle_state"], "active");
    }

    #[test]
    fn hygiene_report_basic() {
        let conn = setup_db();
        add_observation(&conn, "Node 1");
        add_observation(&conn, "Node 2");
        crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Telos".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let report = generate_hygiene_report(&conn).unwrap();
        assert_eq!(report.total_nodes, 3);
        assert!(!report.lifecycle_distribution.is_empty());
    }

    #[test]
    fn process_lifecycle_end_to_end() {
        let conn = setup_db();
        let obs = add_observation(&conn, "Signal Discovery");
        let hyp = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "hypothesis".to_string(),
                name: "Linked Hypothesis".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: obs.id.clone(),
                target_id: hyp.id.clone(),
                edge_type: "EVIDENCES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let result = process_catalyst_lifecycle(&conn, &obs.id).unwrap();
        assert_eq!(result["status"], "lifecycle_complete");
    }
}
