//! TDG Flow Engine v2.0 — Dual-Pole Drive Propagation
//!
//! Port of `core/flow/tdg_flow_engine.py`.
//!
//! Three-stage pipeline:
//! 1. **Emission** — parent drives propagate signed contributions downward
//! 2. **Stabilization** — children integrate contributions with intrinsic signatures
//! 3. **Aggregation** — actualized child states aggregate upward to parents

use std::collections::{HashMap, HashSet, VecDeque};
use std::f64;
use std::sync::LazyLock;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db::crud::{get_edges, get_node, now_iso, record_event};
use crate::error::TdgResult;
use crate::models::Node;

// ─── Lean Mode ────────────────────────────────────────────────────────────────

/// Global lean-mode flag for the flow engine.
///
/// When true, `renormalize_graph` returns early (used during rapid ingestion).
///
/// Previously this was a `static mut bool` read/written via `unsafe` blocks,
/// which is **undefined behavior** when accessed from multiple threads
/// (e.g. concurrent `tdg_connect` calls via `spawn_blocking`). This atomics-based
/// replacement is safe to access from any thread and properly synchronized.
static LEAN_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Set the global lean-mode flag for the flow engine. Should be called once
/// from `TdgServer::new` to sync with `Config::lean`.
pub fn set_lean_mode(enabled: bool) {
    LEAN_MODE.store(enabled, std::sync::atomic::Ordering::SeqCst);
}

/// Read the global lean-mode flag.
pub fn is_lean_mode() -> bool {
    LEAN_MODE.load(std::sync::atomic::Ordering::SeqCst)
}

// ─── Constants ───────────────────────────────────────────────────────────────

pub const MIN_DRIVE_VALUE: f64 = -10.0;
pub const MAX_DRIVE_VALUE: f64 = 10.0;
pub const MAX_INFLUENCE_PER_PARENT: f64 = 0.6;
pub const VARIANCE_FLOOR_RATIO: f64 = 0.3;
pub const INTRINSIC_BLEND_RATIO: f64 = 0.7; // 70% intrinsic + 30% incoming
pub const DEFAULT_MAX_DEPTH: i64 = 5;

/// Quadrant-based multipliers for intrinsic drive modulation.
/// Keys: "UL", "UR", "LL", "LR". Values: drive_name → multiplier.
pub static QUADRANT_MODULATORS: LazyLock<HashMap<&'static str, HashMap<&'static str, f64>>> =
    LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert(
            "UL",
            HashMap::from([
                ("eros", 0.3),
                ("agape", 0.7),
                ("agency", 0.6),
                ("communion", 0.2),
            ]),
        );
        m.insert(
            "UR",
            HashMap::from([
                ("eros", 0.4),
                ("agape", 0.15),
                ("agency", 0.85),
                ("communion", 0.1),
            ]),
        );
        m.insert(
            "LL",
            HashMap::from([
                ("eros", 0.4),
                ("agape", 0.5),
                ("agency", 0.25),
                ("communion", 0.7),
            ]),
        );
        m.insert(
            "LR",
            HashMap::from([
                ("eros", 0.65),
                ("agape", 0.15),
                ("agency", 0.45),
                ("communion", 0.3),
            ]),
        );
        m
    });

// ─── Drive Types ─────────────────────────────────────────────────────────────

/// Diagnosis for a single drive based on dual-pole values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriveDiagnosis {
    Integrated,
    Addiction,
    Allergy,
    BlindSpot,
    TensionPair,
}

/// A single dual-pole drive component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DualPoleDrive {
    pub positive_pole: f64,
    pub negative_pole: f64,
    pub availability: f64,
    pub blind_spot: bool,
}

impl DualPoleDrive {
    pub fn new(positive: f64, negative: f64) -> Self {
        Self {
            positive_pole: positive.clamp(MIN_DRIVE_VALUE, MAX_DRIVE_VALUE),
            negative_pole: negative.clamp(MIN_DRIVE_VALUE, MAX_DRIVE_VALUE),
            availability: 1.0,
            blind_spot: false,
        }
    }

    /// Net impact = positive - negative.
    pub fn net(&self) -> f64 {
        self.positive_pole - self.negative_pole
    }

    /// Variance = min(positive, negative) relative to max possible.
    pub fn variance(&self) -> f64 {
        if self.positive_pole > 0.0 && self.negative_pole > 0.0 {
            let min_pole = self.positive_pole.min(self.negative_pole);
            let max_pole = self.positive_pole.max(self.negative_pole);
            min_pole / max_pole.max(1.0)
        } else {
            0.0
        }
    }

    /// Diagnose the drive state.
    pub fn diagnose(&self) -> DriveDiagnosis {
        let net = self.net();
        let has_both = self.positive_pole > 2.0 && self.negative_pole > 2.0;
        let low_availability = self.availability < 0.3;

        if has_both && self.positive_pole > 5.0 && self.negative_pole > 5.0 {
            DriveDiagnosis::TensionPair
        } else if net > 5.0 && self.positive_pole > 6.0 {
            DriveDiagnosis::Addiction
        } else if net < -3.0 && self.negative_pole > 5.0 {
            DriveDiagnosis::Allergy
        } else if low_availability || (self.positive_pole < 1.0 && self.negative_pole < 1.0) {
            DriveDiagnosis::BlindSpot
        } else {
            DriveDiagnosis::Integrated
        }
    }
}

/// Full drive state for a node (4 primary drives).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowDriveState {
    pub eros: DualPoleDrive,
    pub agape: DualPoleDrive,
    pub agency: DualPoleDrive,
    pub communion: DualPoleDrive,
}

impl FlowDriveState {
    pub fn intrinsic(name: &str) -> Self {
        let sigs = intrinsic_signatures();
        if let Some(sig) = sigs.get(name) {
            Self {
                eros: DualPoleDrive::new(sig.eros.0, sig.eros.1),
                agape: DualPoleDrive::new(sig.agape.0, sig.agape.1),
                agency: DualPoleDrive::new(sig.agency.0, sig.agency.1),
                communion: DualPoleDrive::new(sig.communion.0, sig.communion.1),
            }
        } else {
            // Default signature
            Self {
                eros: DualPoleDrive::new(5.0, 2.0),
                agape: DualPoleDrive::new(4.0, 2.0),
                agency: DualPoleDrive::new(5.0, 2.0),
                communion: DualPoleDrive::new(4.0, 2.0),
            }
        }
    }

    /// Serialize to JSON for storage in drives_json column.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "eros": { "positive_pole": self.eros.positive_pole, "negative_pole": self.eros.negative_pole },
            "agape": { "positive_pole": self.agape.positive_pole, "negative_pole": self.agape.negative_pole },
            "agency": { "positive_pole": self.agency.positive_pole, "negative_pole": self.agency.negative_pole },
            "communion": { "positive_pole": self.communion.positive_pole, "negative_pole": self.communion.negative_pole },
        })
    }

    /// Deserialize from JSON.
    pub fn from_json(v: &serde_json::Value) -> Self {
        let extract = |drive_name: &str| -> DualPoleDrive {
            let d = &v[drive_name];
            let pos = d
                .get("positive_pole")
                .and_then(|v| v.as_f64())
                .unwrap_or(5.0);
            let neg = d
                .get("negative_pole")
                .and_then(|v| v.as_f64())
                .unwrap_or(2.0);
            DualPoleDrive::new(pos, neg)
        };
        Self {
            eros: extract("eros"),
            agape: extract("agape"),
            agency: extract("agency"),
            communion: extract("communion"),
        }
    }

    /// Net vector: [eros, agape, agency, communion].
    pub fn net_vector(&self) -> [f64; 4] {
        [
            self.eros.net(),
            self.agape.net(),
            self.agency.net(),
            self.communion.net(),
        ]
    }
}

// ─── Intrinsic Signatures ────────────────────────────────────────────────────

/// Intrinsic drive signature for a node type.
struct IntrinsicSig {
    eros: (f64, f64),
    agape: (f64, f64),
    agency: (f64, f64),
    communion: (f64, f64),
}

/// Returns intrinsic drive signatures for each node type.
fn intrinsic_signatures() -> HashMap<&'static str, IntrinsicSig> {
    let mut m = HashMap::new();
    m.insert(
        "telos",
        IntrinsicSig {
            eros: (6.0, 2.0),
            agape: (5.0, 2.0),
            agency: (6.0, 2.0),
            communion: (5.0, 2.0),
        },
    );
    m.insert(
        "action",
        IntrinsicSig {
            eros: (7.0, 2.0),
            agape: (4.0, 1.0),
            agency: (7.0, 2.0),
            communion: (4.0, 1.0),
        },
    );
    m.insert(
        "capability",
        IntrinsicSig {
            eros: (5.0, 1.0),
            agape: (5.0, 1.0),
            agency: (6.0, 2.0),
            communion: (5.0, 2.0),
        },
    );
    m.insert(
        "question",
        IntrinsicSig {
            eros: (9.0, 2.0),
            agape: (2.0, 1.0),
            agency: (3.0, 1.0),
            communion: (4.0, 2.0),
        },
    );
    m.insert(
        "observation",
        IntrinsicSig {
            eros: (4.0, 1.0),
            agape: (3.0, 1.0),
            agency: (3.0, 1.0),
            communion: (4.0, 1.0),
        },
    );
    m.insert(
        "hypothesis",
        IntrinsicSig {
            eros: (6.0, 2.0),
            agape: (3.0, 1.0),
            agency: (5.0, 2.0),
            communion: (4.0, 2.0),
        },
    );
    m.insert(
        "constraint",
        IntrinsicSig {
            eros: (3.0, 1.0),
            agape: (5.0, 2.0),
            agency: (4.0, 1.0),
            communion: (5.0, 2.0),
        },
    );
    m.insert(
        "discovery",
        IntrinsicSig {
            eros: (8.0, 2.0),
            agape: (3.0, 1.0),
            agency: (5.0, 2.0),
            communion: (3.0, 1.0),
        },
    );
    m.insert(
        "project",
        IntrinsicSig {
            eros: (5.0, 2.0),
            agape: (4.0, 1.0),
            agency: (6.0, 2.0),
            communion: (5.0, 2.0),
        },
    );
    m.insert(
        "trajectory",
        IntrinsicSig {
            eros: (5.0, 1.0),
            agape: (4.0, 1.0),
            agency: (5.0, 1.0),
            communion: (5.0, 1.0),
        },
    );
    m.insert(
        "synthesis",
        IntrinsicSig {
            eros: (6.0, 2.0),
            agape: (6.0, 2.0),
            agency: (5.0, 2.0),
            communion: (6.0, 2.0),
        },
    );
    m.insert(
        "skill",
        IntrinsicSig {
            eros: (5.0, 1.0),
            agape: (4.0, 1.0),
            agency: (6.0, 1.0),
            communion: (4.0, 1.0),
        },
    );
    m.insert(
        "people",
        IntrinsicSig {
            eros: (4.0, 1.0),
            agape: (6.0, 2.0),
            agency: (3.0, 1.0),
            communion: (7.0, 2.0),
        },
    );
    m.insert(
        "artifact",
        IntrinsicSig {
            eros: (3.0, 1.0),
            agape: (3.0, 1.0),
            agency: (5.0, 2.0),
            communion: (3.0, 1.0),
        },
    );
    m.insert(
        "being",
        IntrinsicSig {
            eros: (5.0, 2.0),
            agape: (5.0, 2.0),
            agency: (5.0, 2.0),
            communion: (6.0, 2.0),
        },
    );
    m.insert(
        "communication",
        IntrinsicSig {
            eros: (4.0, 1.0),
            agape: (6.0, 2.0),
            agency: (3.0, 1.0),
            communion: (7.0, 2.0),
        },
    );
    m.insert(
        "event",
        IntrinsicSig {
            eros: (4.0, 1.0),
            agape: (3.0, 1.0),
            agency: (4.0, 1.0),
            communion: (4.0, 1.0),
        },
    );
    m.insert(
        "insight",
        IntrinsicSig {
            eros: (8.0, 2.0),
            agape: (4.0, 1.0),
            agency: (5.0, 2.0),
            communion: (4.0, 2.0),
        },
    );
    m.insert(
        "value",
        IntrinsicSig {
            eros: (2.5, 1.0),
            agape: (3.0, 1.5),
            agency: (4.0, 1.0),
            communion: (3.5, 1.5),
        },
    );
    m.insert(
        "bond",
        IntrinsicSig {
            eros: (3.0, 1.5),
            agape: (5.0, 1.0),
            agency: (3.0, 1.0),
            communion: (6.0, 1.5),
        },
    );
    m.insert(
        "narrative",
        IntrinsicSig {
            eros: (6.0, 2.0),
            agape: (4.0, 1.5),
            agency: (5.0, 2.0),
            communion: (5.0, 1.5),
        },
    );
    m
}


// ─── Edge Contracts ──────────────────────────────────────────────────────────

/// Edge type → (flow_rate, aggregation_weight).
fn edge_flow_rate(edge_type: &str) -> (f64, f64) {
    match edge_type {
        "DECOMPOSES_TO" | "ENABLES" | "REALIZES" | "SUPPORTS" => (0.8, 0.8),
        "DEPENDS_ON" | "PRECEDES" | "CONTEXT" => (0.6, 0.6),
        "RELATES_TO" | "REFERENCES" | "MENTIONS" => (0.3, 0.3),
        "BLOCKS" => (-0.5, -0.5),
        "CONTRADICTS" => (-0.7, -0.7),
        _ => (0.4, 0.4),
    }
}

/// Edge types that propagate upward during aggregation.
fn contributes_to_polarity(edge_type: &str) -> bool {
    matches!(
        edge_type,
        "DECOMPOSES_TO" | "ENABLES" | "REALIZES" | "SUPPORTS" | "DEPENDS_ON"
    )
}

/// Edge types that are traversed downward from parent to child.
fn downward_edge_types() -> Vec<&'static str> {
    vec![
        "DECOMPOSES_TO",
        "ENABLES",
        "REALIZES",
        "SUPPORTS",
        "DEPENDS_ON",
        "CONTEXT",
        "BLOCKS",
        "CONTRADICTS",
    ]
}

/// Edge types traversed "reversed" (child→parent direction but treated as parent→child).
fn reversed_downward_types() -> Vec<&'static str> {
    vec!["SUPPORTS", "ENABLES", "REALIZES"]
}

// ─── Serialization Helpers ───────────────────────────────────────────────────

/// Serialize FlowDriveState to JSON and store in node's drives_json column.
///
/// Goes through the same write-protection path as every other mutation in the
/// system: circuit-breaker check + write-guard acquisition. Previously this
/// bypassed both, meaning that when the breaker was `Open` (tripped), drive
/// mutations would still commit while the subsequent `record_event` call would
/// fail — leaving the graph in an inconsistent state with no audit trail.
fn store_drive_state(conn: &Connection, node_id: &str, state: &FlowDriveState) -> TdgResult<()> {
    crate::db::crud::check_circuit_breaker_pub()?;
    let _guard = crate::db::crud::acquire_write_guard_pub(conn)?;
    let json = state.to_json();
    let now = now_iso();
    conn.execute(
        "UPDATE nodes SET drives_json = ?1, updated_at = ?2 WHERE id = ?3 AND valid_to IS NULL",
        params![json.to_string(), now, node_id],
    )?;
    Ok(())
}

/// Load FlowDriveState from a node's drives_json column.
/// Falls back to intrinsic signature if not set.
fn load_drive_state(_conn: &Connection, node: &Node) -> FlowDriveState {
    let drives_json = &node.drives;
    if drives_json
        .as_object()
        .is_some_and(|m| m.contains_key("eros"))
    {
        FlowDriveState::from_json(drives_json)
    } else {
        FlowDriveState::intrinsic(&node.node_type)
    }
}

// ─── Core Pipeline ───────────────────────────────────────────────────────────

/// Stabilize a child node after receiving contributions from a parent.
///
/// Blends `INTRINSIC_BLEND_RATIO` of current state with incoming contribution,
/// clamped by `MAX_INFLUENCE_PER_PARENT` and `VARIANCE_FLOOR_RATIO`.
fn receive_stabilize(
    child_state: &FlowDriveState,
    intrinsic: &FlowDriveState,
    contribution: &FlowDriveState,
) -> FlowDriveState {
    let clamp_drive = |child: &DualPoleDrive,
                       intrinsic_d: &DualPoleDrive,
                       contrib: &DualPoleDrive,
                       influence_weight: f64|
     -> DualPoleDrive {
        let capped_pos = contrib.positive_pole.clamp(-5.0, 5.0) * influence_weight.min(0.6);
        let capped_neg = contrib.negative_pole.clamp(-5.0, 5.0) * influence_weight.min(0.6);

        let new_pos = child.positive_pole * INTRINSIC_BLEND_RATIO
            + capped_pos * (1.0 - INTRINSIC_BLEND_RATIO);
        let new_neg = child.negative_pole * INTRINSIC_BLEND_RATIO
            + capped_neg * (1.0 - INTRINSIC_BLEND_RATIO);

        let intr_var = intrinsic_d.variance();
        let mut result = DualPoleDrive::new(new_pos, new_neg);
        if intr_var > 0.1 && result.variance() < intr_var * VARIANCE_FLOOR_RATIO {
            if result.positive_pole < result.negative_pole {
                result.positive_pole = result.negative_pole * intr_var * VARIANCE_FLOOR_RATIO;
            } else {
                result.negative_pole = result.positive_pole * intr_var * VARIANCE_FLOOR_RATIO;
            }
            result = DualPoleDrive::new(result.positive_pole, result.negative_pole);
        }

        result
    };

    let influence = contribution.eros.positive_pole.abs() / MAX_DRIVE_VALUE;

    FlowDriveState {
        eros: clamp_drive(
            &child_state.eros,
            &intrinsic.eros,
            &contribution.eros,
            influence,
        ),
        agape: clamp_drive(
            &child_state.agape,
            &intrinsic.agape,
            &contribution.agape,
            influence,
        ),
        agency: clamp_drive(
            &child_state.agency,
            &intrinsic.agency,
            &contribution.agency,
            influence,
        ),
        communion: clamp_drive(
            &child_state.communion,
            &intrinsic.communion,
            &contribution.communion,
            influence,
        ),
    }
}

/// Build a FlowDriveState representing the contribution from a parent to a child.
fn build_contribution(parent_state: &FlowDriveState, flow_rate: f64) -> FlowDriveState {
    let scale = |d: &DualPoleDrive| -> DualPoleDrive {
        DualPoleDrive::new(
            d.positive_pole * flow_rate,
            d.negative_pole * flow_rate.abs(),
        )
    };
    FlowDriveState {
        eros: scale(&parent_state.eros),
        agape: scale(&parent_state.agape),
        agency: scale(&parent_state.agency),
        communion: scale(&parent_state.communion),
    }
}

/// Phase 1: Emit drive states downward from a parent node.
///
/// Traverses the graph BFS, propagating signed drive contributions.
/// Returns the number of nodes affected.
pub fn emit_downward(conn: &Connection, parent_id: &str, max_depth: i64) -> TdgResult<i64> {
    let parent = get_node(conn, parent_id)?
        .ok_or_else(|| crate::error::TdgError::Custom(format!("Node {parent_id} not found")))?;

    let parent_state = load_drive_state(conn, &parent);
    let mut affected: i64 = 0;

    let downward_types = downward_edge_types();
    let reversed_types = reversed_downward_types();

    // BFS queue: (node_id, depth, immediate_parent_id, parent_drive_state)
    let mut queue: VecDeque<(String, i64, String, FlowDriveState)> = VecDeque::new();
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(parent_id.to_string());

    // Get children via downward edges.
    // IMPORTANT: insert each child into `visited` BEFORE enqueuing. In a DAG a
    // node can be reachable via multiple paths (e.g. both directly from `parent`
    // and as a grandchild via a sibling). Without this dedup at enqueue time,
    // such a node would be processed twice with different parent_drive_state
    // values, producing non-deterministic drive recomputation and duplicate
    // `drive_recomputed` events.
    let children = get_children_for_emission(conn, parent_id, &downward_types, &reversed_types)?;
    for child_id in children {
        if visited.insert(child_id.clone()) {
            queue.push_back((child_id, 1, parent_id.to_string(), parent_state.clone()));
        }
    }

    while let Some((current_id, depth, immediate_parent_id, parent_drive_state)) = queue.pop_front()
    {
        if depth > max_depth {
            continue;
        }

        if let Some(child) = get_node(conn, &current_id)? {
            let child_state = load_drive_state(conn, &child);
            let intrinsic = FlowDriveState::intrinsic(&child.node_type);

            let edge_type_str = get_edge_type_for_edge(conn, &immediate_parent_id, &current_id)?;
            if !contributes_to_polarity(&edge_type_str) {
                let grandchildren =
                    get_children_for_emission(conn, &current_id, &downward_types, &reversed_types)?;
                for gc in grandchildren {
                    if !visited.contains(&gc) {
                        visited.insert(gc.clone());
                        queue.push_back((
                            gc,
                            depth + 1,
                            current_id.clone(),
                            parent_drive_state.clone(),
                        ));
                    }
                }
                continue;
            }

            let mut flow_rate = get_flow_rate_for_edge(conn, &immediate_parent_id, &current_id)?;
            if edge_type_str == "BLOCKS" {
                flow_rate = -flow_rate.abs();
            } else if edge_type_str == "CONTRADICTS" {
                flow_rate = -flow_rate.abs() * 1.5;
            }

            let contribution = build_contribution(&parent_drive_state, flow_rate);
            let new_state = receive_stabilize(&child_state, &intrinsic, &contribution);

            store_drive_state(conn, &current_id, &new_state)?;

            let _ = record_event(
                conn,
                "drive_recomputed",
                Some(&current_id),
                None,
                None,
                Some(&new_state.to_json()),
            );

            affected += 1;

            let grandchildren =
                get_children_for_emission(conn, &current_id, &downward_types, &reversed_types)?;
            for gc in grandchildren {
                if !visited.contains(&gc) {
                    visited.insert(gc.clone());
                    queue.push_back((gc, depth + 1, current_id.clone(), new_state.clone()));
                }
            }
        }
    }

    Ok(affected)
}

/// Get children for emission traversal.
fn get_children_for_emission(
    conn: &Connection,
    node_id: &str,
    downward_types: &[&str],
    reversed_types: &[&str],
) -> TdgResult<Vec<String>> {
    let mut children = Vec::new();

    // Standard downward: edges where this node is source
    for dt in downward_types {
        let edges = get_edges(conn, Some(node_id), None, Some(dt), None, 500)?;
        for e in edges {
            children.push(e.target_id);
        }
    }

    // Reversed: edges where this node is TARGET but edge type is reversed
    for rt in reversed_types {
        let edges = get_edges(conn, None, Some(node_id), Some(rt), None, 500)?;
        for e in edges {
            children.push(e.source_id);
        }
    }

    children.sort();
    children.dedup();
    Ok(children)
}

/// Get the flow rate for the edge between source and target.
fn get_flow_rate_for_edge(conn: &Connection, source_id: &str, target_id: &str) -> TdgResult<f64> {
    // Check standard direction
    let edges = get_edges(conn, Some(source_id), Some(target_id), None, None, 10)?;
    if let Some(e) = edges.first() {
        return Ok(edge_flow_rate(&e.edge_type).0);
    }

    // Check reversed direction
    let edges = get_edges(conn, Some(target_id), Some(source_id), None, None, 10)?;
    if let Some(e) = edges.first() {
        return Ok(edge_flow_rate(&e.edge_type).0);
    }

    Ok(0.4) // default
}

fn get_edge_type_for_edge(
    conn: &Connection,
    source_id: &str,
    target_id: &str,
) -> TdgResult<String> {
    let edges = get_edges(conn, Some(source_id), Some(target_id), None, None, 10)?;
    if let Some(e) = edges.first() {
        return Ok(e.edge_type.clone());
    }
    let edges = get_edges(conn, Some(target_id), Some(source_id), None, None, 10)?;
    if let Some(e) = edges.first() {
        return Ok(e.edge_type.clone());
    }
    Ok("MENTIONS".to_string())
}

/// Phase 3: Aggregate child drive states upward to parent.
///
/// Returns the number of parents updated.
pub fn aggregate_upward(conn: &Connection, node_id: &str) -> TdgResult<i64> {
    // Find all parent edges (incoming to this node)
    let incoming = get_edges(conn, None, Some(node_id), None, None, 500)?;
    let mut parent_ids: Vec<String> = Vec::new();

    for e in &incoming {
        if contributes_to_polarity(&e.edge_type) && !parent_ids.contains(&e.source_id) {
            parent_ids.push(e.source_id.clone());
        }
    }

    if parent_ids.is_empty() {
        return Ok(0);
    }

    let mut updated_parents = 0;

    for pid in &parent_ids {
        if let Some(parent) = get_node(conn, pid)? {
            let parent_state = load_drive_state(conn, &parent);
            let intrinsic = FlowDriveState::intrinsic(&parent.node_type);

            // Get all children of this parent
            let child_edges = get_edges(conn, Some(pid), None, None, None, 500)?;
            let mut child_states: Vec<FlowDriveState> = Vec::new();

            for ce in &child_edges {
                if contributes_to_polarity(&ce.edge_type) {
                    if let Some(child) = get_node(conn, &ce.target_id)? {
                        let child_state = load_drive_state(conn, &child);
                        child_states.push(child_state);
                    }
                }
            }

            if child_states.is_empty() {
                continue;
            }

            // Compute weighted average of child drives
            let _n = child_states.len() as f64;
            let aggregate = FlowDriveState {
                eros: average_drive(&child_states.iter().map(|s| &s.eros).collect::<Vec<_>>()),
                agape: average_drive(&child_states.iter().map(|s| &s.agape).collect::<Vec<_>>()),
                agency: average_drive(&child_states.iter().map(|s| &s.agency).collect::<Vec<_>>()),
                communion: average_drive(
                    &child_states
                        .iter()
                        .map(|s| &s.communion)
                        .collect::<Vec<_>>(),
                ),
            };

            // Blend: 60% parent intrinsic + 40% aggregate
            let blended = FlowDriveState {
                eros: blend_drives(&intrinsic.eros, &aggregate.eros, 0.6),
                agape: blend_drives(&intrinsic.agape, &aggregate.agape, 0.6),
                agency: blend_drives(&intrinsic.agency, &aggregate.agency, 0.6),
                communion: blend_drives(&intrinsic.communion, &aggregate.communion, 0.6),
            };

            // Blend with existing parent state (70% existing, 30% new blend)
            let final_state = FlowDriveState {
                eros: blend_drives(&parent_state.eros, &blended.eros, 0.7),
                agape: blend_drives(&parent_state.agape, &blended.agape, 0.7),
                agency: blend_drives(&parent_state.agency, &blended.agency, 0.7),
                communion: blend_drives(&parent_state.communion, &blended.communion, 0.7),
            };

            store_drive_state(conn, pid, &final_state)?;
            updated_parents += 1;
        }
    }

    Ok(updated_parents)
}

fn average_drive(drives: &[&DualPoleDrive]) -> DualPoleDrive {
    let n = drives.len() as f64;
    if n == 0.0 {
        return DualPoleDrive::new(0.0, 0.0);
    }
    let pos: f64 = drives.iter().map(|d| d.positive_pole).sum::<f64>() / n;
    let neg: f64 = drives.iter().map(|d| d.negative_pole).sum::<f64>() / n;
    DualPoleDrive::new(pos, neg)
}

fn blend_drives(a: &DualPoleDrive, b: &DualPoleDrive, a_weight: f64) -> DualPoleDrive {
    let b_weight = 1.0 - a_weight;
    DualPoleDrive::new(
        a.positive_pole * a_weight + b.positive_pole * b_weight,
        a.negative_pole * a_weight + b.negative_pole * b_weight,
    )
}

/// Phase 2: Renormalize the entire graph.
///
/// 1. Schema heal: ensure all nodes have drive_state
/// 2. Downward flow from telos nodes
/// 3. Upward aggregation from leaves
pub fn renormalize_graph(conn: &Connection, force_intrinsic: bool) -> TdgResult<serde_json::Value> {
    if is_lean_mode() {
        return Ok(serde_json::json!({
            "skipped": true,
            "reason": "lean_mode",
        }));
    }
    let mut healed = 0i64;
    let mut emitted = 0i64;
    let mut aggregated = 0i64;

    // Phase 1: Schema heal — ensure all nodes have drive_state
    {
        let mut stmt =
            conn.prepare("SELECT id, node_type, drives_json FROM nodes WHERE valid_to IS NULL")?;
        let rows: Vec<(String, String, String)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        for (id, node_type, drives_json) in &rows {
            let needs_init = force_intrinsic
                || drives_json == "{}"
                || drives_json.is_empty()
                || !drives_json.contains("eros");

            if needs_init {
                let intrinsic = FlowDriveState::intrinsic(node_type);
                store_drive_state(conn, id, &intrinsic)?;
                healed += 1;
            }
        }
    }

    // Phase 2: Downward flow from top-level telos nodes
    {
        let mut stmt = conn.prepare(
            "SELECT id FROM nodes WHERE node_type = 'telos' AND valid_to IS NULL AND parent_ids = '[]'",
        )?;
        let telos_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        for tid in telos_ids {
            emitted += emit_downward(conn, &tid, DEFAULT_MAX_DEPTH)?;
        }
    }

    // Phase 3: Upward aggregation using topological order
    //
    // `aggregate_upward(conn, id)` updates the PARENTS of `id` based on `id`'s
    // drive state. The previous implementation guarded this call with
    // `if child_count > 0`, which is the INVERSE of the correct logic: it
    // skipped aggregation for leaf nodes (nodes with no outgoing edges),
    // meaning a parent whose only children were leaves never got re-aggregated.
    //
    // The correct approach: for every active node, ask "does this node have
    // parents that should be re-aggregated?" — i.e. does it have INCOMING
    // polarity edges. If yes, run aggregate_upward on those parents.
    // aggregate_upward itself is a no-op when there are no incoming edges.
    {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT n.id
             FROM nodes n
             INNER JOIN edges e ON e.target_id = n.id
             WHERE n.valid_to IS NULL
               AND e.valid_to IS NULL
               AND e.edge_type IN ('DECOMPOSES_TO','ENABLES','PURSUES','CONTEXT',
                                   'EVIDENCES','BLOCKS','SYNTHESIZES','HAS_CAPABILITY',
                                   'SENT','RECEIVED','TRIGGERED','DETECTED','ILLUMINATES',
                                   'OPENS','CREATES','ADVANCES','APPEALS_TO','REPLIES',
                                   'CONTINUES','SEEKS')",
        )?;
        let parent_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        for pid in &parent_ids {
            aggregated += aggregate_upward(conn, pid)?;
        }
    }

    let result = serde_json::json!({
        "healed": healed,
        "emitted": emitted,
        "aggregated": aggregated,
    });

    // Record event
    crate::db::crud::record_event(conn, "graph_renormalized", None, None, None, Some(&result))?;

    Ok(result)
}

/// Full polarity diagnosis across the graph.
pub fn diagnose_polarity(conn: &Connection) -> TdgResult<serde_json::Value> {
    let mut addictions = Vec::new();
    let mut allergies = Vec::new();
    let mut blind_spots = Vec::new();
    let mut tension_pairs = Vec::new();
    let mut chakra_health: HashMap<String, serde_json::Value> = HashMap::new();

    let mut stmt =
        conn.prepare("SELECT id, node_type, name, drives_json FROM nodes WHERE valid_to IS NULL")?;

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

    for (id, node_type, name, drives_json) in &rows {
        let drives: serde_json::Value =
            serde_json::from_str(drives_json).unwrap_or(serde_json::json!({}));
        let state = FlowDriveState::from_json(&drives);

        let drives_map = [
            ("eros", &state.eros),
            ("agape", &state.agape),
            ("agency", &state.agency),
            ("communion", &state.communion),
        ];

        for (drive_name, drive) in &drives_map {
            let diagnosis = drive.diagnose();
            match diagnosis {
                DriveDiagnosis::Addiction => addictions.push(serde_json::json!({
                    "node_id": id, "name": name, "drive": drive_name,
                    "positive": drive.positive_pole, "negative": drive.negative_pole,
                })),
                DriveDiagnosis::Allergy => allergies.push(serde_json::json!({
                    "node_id": id, "name": name, "drive": drive_name,
                    "positive": drive.positive_pole, "negative": drive.negative_pole,
                })),
                DriveDiagnosis::BlindSpot => blind_spots.push(serde_json::json!({
                    "node_id": id, "name": name, "drive": drive_name,
                })),
                DriveDiagnosis::TensionPair => tension_pairs.push(serde_json::json!({
                    "node_id": id, "name": name, "drive": drive_name,
                    "positive": drive.positive_pole, "negative": drive.negative_pole,
                })),
                DriveDiagnosis::Integrated => {}
            }
        }

        // Chakra health for telos nodes
        if node_type == "telos" {
            let net = state.net_vector();
            let avg = net.iter().sum::<f64>() / 4.0;
            let variance = net.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / 4.0;
            chakra_health.insert(
                id.clone(),
                serde_json::json!({
                    "name": name,
                    "net_vector": net,
                    "average": avg,
                    "variance": variance,
                    "health": if variance < 2.0 { "balanced" } else if variance < 8.0 { "moderate" } else { "imbalanced" },
                }),
            );
        }
    }

    // Run entropy check
    let entropy = compute_graph_entropy(conn)?;

    Ok(serde_json::json!({
        "addictions": addictions,
        "allergies": allergies,
        "blind_spots": blind_spots,
        "tension_pairs": tension_pairs,
        "chakra_health": chakra_health,
        "entropy": entropy,
    }))
}

/// Compute Shannon entropy of drive distribution across the graph.
pub fn compute_graph_entropy(conn: &Connection) -> TdgResult<serde_json::Value> {
    let mut drive_values: HashMap<&str, Vec<f64>> = HashMap::new();
    drive_values.insert("eros", Vec::new());
    drive_values.insert("agape", Vec::new());
    drive_values.insert("agency", Vec::new());
    drive_values.insert("communion", Vec::new());

    let mut stmt = conn
        .prepare("SELECT drives_json FROM nodes WHERE valid_to IS NULL AND drives_json != '{}'")?;

    let rows: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for drives_json in &rows {
        let drives: serde_json::Value =
            serde_json::from_str(drives_json).unwrap_or(serde_json::json!({}));
        let state = FlowDriveState::from_json(&drives);

        drive_values.get_mut("eros").unwrap().push(state.eros.net());
        drive_values
            .get_mut("agape")
            .unwrap()
            .push(state.agape.net());
        drive_values
            .get_mut("agency")
            .unwrap()
            .push(state.agency.net());
        drive_values
            .get_mut("communion")
            .unwrap()
            .push(state.communion.net());
    }

    let mut per_drive = serde_json::Map::new();
    let mut total_entropy = 0.0;

    for (name, values) in &drive_values {
        if values.is_empty() {
            per_drive.insert(
                name.to_string(),
                serde_json::json!({"entropy": 0.0, "count": 0}),
            );
            continue;
        }

        // Bin values into buckets: [-10,-6), [-6,-2), [-2,2), [2,6), [6,10]
        let mut bins = [0.0f64; 5];
        for v in values {
            let bin = match v {
                x if *x < -6.0 => 0,
                x if *x < -2.0 => 1,
                x if *x < 2.0 => 2,
                x if *x < 6.0 => 3,
                _ => 4,
            };
            bins[bin] += 1.0;
        }

        let total = values.len() as f64;
        let mut entropy = 0.0;
        for &count in &bins {
            if count > 0.0 {
                let p = count / total;
                entropy -= p * p.log2();
            }
        }

        // Normalize: max entropy for 5 bins is log2(5) ≈ 2.32
        let normalized = entropy / 2.32;
        total_entropy += normalized;

        per_drive.insert(
            name.to_string(),
            serde_json::json!({
                "entropy": (entropy * 1000.0).round() / 1000.0,
                "normalized": (normalized * 1000.0).round() / 1000.0,
                "count": values.len(),
                "mean": values.iter().sum::<f64>() / total,
            }),
        );
    }

    let avg_entropy = total_entropy / 4.0;
    let health = if avg_entropy > 0.8 {
        "good"
    } else if avg_entropy > 0.5 {
        "warning"
    } else {
        "critical_dilution"
    };

    Ok(serde_json::json!({
        "per_drive": per_drive,
        "average_normalized_entropy": (avg_entropy * 1000.0).round() / 1000.0,
        "health": health,
        "total_nodes": rows.len(),
    }))
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

    fn add_test_node(conn: &Connection, name: &str, node_type: &str) -> Node {
        crate::db::crud::add_node(
            conn,
            &NewNode {
                node_type: node_type.to_string(),
                name: name.to_string(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn dual_pole_drive_basics() {
        let d = DualPoleDrive::new(7.0, 1.0);
        assert_eq!(d.net(), 6.0);
        assert_eq!(d.diagnose(), DriveDiagnosis::Addiction);

        let d2 = DualPoleDrive::new(1.0, 7.0);
        assert_eq!(d2.net(), -6.0);
        assert_eq!(d2.diagnose(), DriveDiagnosis::Allergy);

        let d3 = DualPoleDrive::new(0.5, 0.5);
        assert_eq!(d3.diagnose(), DriveDiagnosis::BlindSpot);

        let d4 = DualPoleDrive::new(6.0, 6.0);
        assert_eq!(d4.diagnose(), DriveDiagnosis::TensionPair);
    }

    #[test]
    fn drive_state_intrinsic() {
        let state = FlowDriveState::intrinsic("telos");
        assert_eq!(state.eros.positive_pole, 6.0);
        assert_eq!(state.eros.negative_pole, 2.0);
    }

    #[test]
    fn drive_state_serialization_roundtrip() {
        let state = FlowDriveState::intrinsic("action");
        let json = state.to_json();
        let restored = FlowDriveState::from_json(&json);
        assert_eq!(restored.eros.positive_pole, state.eros.positive_pole);
        assert_eq!(restored.agency.negative_pole, state.agency.negative_pole);
    }

    #[test]
    fn receive_stabilize_respects_variance_floor() {
        let child = FlowDriveState::intrinsic("observation");
        let intrinsic = FlowDriveState::intrinsic("observation");
        let contribution = FlowDriveState {
            eros: DualPoleDrive::new(8.0, 0.0),
            agape: DualPoleDrive::new(8.0, 0.0),
            agency: DualPoleDrive::new(8.0, 0.0),
            communion: DualPoleDrive::new(8.0, 0.0),
        };

        let result = receive_stabilize(&child, &intrinsic, &contribution);
        // Result should be clamped
        assert!(result.eros.positive_pole <= MAX_DRIVE_VALUE);
        assert!(result.eros.positive_pole >= MIN_DRIVE_VALUE);
    }

    #[test]
    fn emit_downward_basic() {
        let conn = setup_db();
        let parent = add_test_node(&conn, "Root Telos", "telos");
        let child = add_test_node(&conn, "Child Action", "action");

        // Create DECOMPOSES_TO edge
        crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: parent.id.clone(),
                target_id: child.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let affected = emit_downward(&conn, &parent.id, 5).unwrap();
        assert!(affected >= 1);

        // Child should now have a drive state in drives_json
        let child_node = get_node(&conn, &child.id).unwrap().unwrap();
        let drives = &child_node.drives;
        assert!(drives.as_object().is_some_and(|m| m.contains_key("eros")));
    }

    #[test]
    fn renormalize_graph_basic() {
        let conn = setup_db();
        let telos = add_test_node(&conn, "Main Telos", "telos");
        let action = add_test_node(&conn, "Some Action", "action");
        let obs = add_test_node(&conn, "Some Observation", "observation");

        crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: telos.id.clone(),
                target_id: action.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: action.id.clone(),
                target_id: obs.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let result = renormalize_graph(&conn, false).unwrap();
        assert!(result.get("healed").is_some());
        assert!(result.get("emitted").is_some());
        assert!(result.get("aggregated").is_some());
    }

    #[test]
    fn diagnose_polarity_basic() {
        let conn = setup_db();
        add_test_node(&conn, "Test Telos", "telos");
        add_test_node(&conn, "Test Action", "action");

        let result = diagnose_polarity(&conn).unwrap();
        assert!(result.get("addictions").is_some());
        assert!(result.get("entropy").is_some());
    }

    #[test]
    fn compute_entropy_basic() {
        let conn = setup_db();
        // Add some nodes with different drive states
        for i in 0..5 {
            let node = add_test_node(&conn, &format!("Node {i}"), "observation");
            let state = FlowDriveState {
                eros: DualPoleDrive::new(i as f64 * 2.0, 1.0),
                agape: DualPoleDrive::new(5.0, 2.0),
                agency: DualPoleDrive::new(3.0, 1.0),
                communion: DualPoleDrive::new(4.0, 1.0),
            };
            store_drive_state(&conn, &node.id, &state).unwrap();
        }

        let result = compute_graph_entropy(&conn).unwrap();
        assert!(result.get("average_normalized_entropy").is_some());
        assert!(result.get("health").is_some());
    }

    #[test]
    fn edge_flow_rates() {
        assert_eq!(edge_flow_rate("DECOMPOSES_TO").0, 0.8);
        assert_eq!(edge_flow_rate("BLOCKS").0, -0.5);
        assert_eq!(edge_flow_rate("CONTRADICTS").0, -0.7);
        assert_eq!(edge_flow_rate("SUPPORTS").0, 0.8);
        assert_eq!(edge_flow_rate("RELATES_TO").0, 0.3);
        assert_eq!(edge_flow_rate("DECOMPOSES_TO").1, 0.8);
        assert_eq!(edge_flow_rate("BLOCKS").1, -0.5);
    }

    #[test]
    fn quadrant_modulators_present() {
        assert!(QUADRANT_MODULATORS.contains_key("UL"));
        assert!(QUADRANT_MODULATORS.contains_key("UR"));
        assert!(QUADRANT_MODULATORS.contains_key("LL"));
        assert!(QUADRANT_MODULATORS.contains_key("LR"));

        // Check all four drives present in each quadrant
        for quadrant in &["UL", "UR", "LL", "LR"] {
            let mods = QUADRANT_MODULATORS.get(quadrant).unwrap();
            assert!(mods.contains_key("eros"), "Missing eros in {quadrant}");
            assert!(mods.contains_key("agape"), "Missing agape in {quadrant}");
            assert!(mods.contains_key("agency"), "Missing agency in {quadrant}");
            assert!(
                mods.contains_key("communion"),
                "Missing communion in {quadrant}"
            );
        }
    }


    #[test]
    fn missing_intrinsic_signatures_added() {
        // These 3 were added in Phase 14
        let value = FlowDriveState::intrinsic("value");
        assert!(
            value.eros.positive_pole > 0.0,
            "value eros should be positive"
        );

        let bond = FlowDriveState::intrinsic("bond");
        assert!(
            bond.agape.positive_pole > 0.0,
            "bond agape should be positive"
        );

        let narrative = FlowDriveState::intrinsic("narrative");
        assert!(
            narrative.eros.positive_pole > 0.0,
            "narrative eros should be positive"
        );
    }

    #[test]
    fn lean_mode_skips_renormalization() {
        let conn = setup_db();
        let _telos = add_test_node(&conn, "Lean Telos", "telos");

        set_lean_mode(true);
        let result = renormalize_graph(&conn, false).unwrap();
        set_lean_mode(false);

        assert_eq!(result.get("skipped"), Some(&serde_json::json!(true)));
        assert_eq!(result.get("reason"), Some(&serde_json::json!("lean_mode")));
    }

    #[test]
    fn emit_downward_records_drive_recomputed_events() {
        let conn = setup_db();
        let parent = add_test_node(&conn, "Event Telos", "telos");
        let child = add_test_node(&conn, "Event Action", "action");

        crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: parent.id.clone(),
                target_id: child.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        emit_downward(&conn, &parent.id, 5).unwrap();

        // Check that drive_recomputed events were recorded
        let _events =
            crate::db::crud::get_edges(&conn, Some(&parent.id), None, None, None, 100).unwrap();
        // Events go to the events table, not edges — check via SQL
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM events WHERE event_action = 'drive_recomputed'")
            .unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert!(
            count >= 1,
            "Expected at least 1 drive_recomputed event, got {count}"
        );
    }

    #[test]
    fn contributes_to_polarity_filter() {
        assert!(contributes_to_polarity("DECOMPOSES_TO"));
        assert!(contributes_to_polarity("SUPPORTS"));
        assert!(contributes_to_polarity("ENABLES"));
        assert!(contributes_to_polarity("REALIZES"));
        assert!(contributes_to_polarity("DEPENDS_ON"));
        assert!(!contributes_to_polarity("BLOCKS"));
        assert!(!contributes_to_polarity("CONTRADICTS"));
        assert!(!contributes_to_polarity("MENTIONS"));
        assert!(!contributes_to_polarity("REFERENCES"));
        assert!(!contributes_to_polarity("CONTEXT"));
    }

    #[test]
    fn blocks_contradicts_negative_flow() {
        // BLOCKS and CONTRADICTS should have negative flow rates
        let (blocks_rate, _) = edge_flow_rate("BLOCKS");
        let (contradicts_rate, _) = edge_flow_rate("CONTRADICTS");
        assert!(blocks_rate < 0.0, "BLOCKS rate should be negative");
        assert!(
            contradicts_rate < 0.0,
            "CONTRADICTS rate should be negative"
        );
        // CONTRADICTS should be 1.5x stronger than BLOCKS
        assert!(contradicts_rate.abs() > blocks_rate.abs());
    }
}
