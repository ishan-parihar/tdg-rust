//! ContextPack — single-call structured context aggregation for AI agents.
//!
//! Source: HoloOS `AGENTS.md` §"The ContextPack" and `_THEORY/02_Ontology/08.1`
//!
//! The ContextPack is the capstone of the agent API. It aggregates
//! intra/inter/extra-holonic context into a single structured object,
//! replacing 6+ CLI calls with 1. Every claim carries a `[status: {status}]`
//! tag so the agent knows the epistemic grade of what it's reading.
//!
//! ## Structure
//!
//! ```text
//! ContextPack
//! ├── identity (holon_id, scale_code, type_code, type_class, synthesis_status)
//! ├── intra (attractor_field, health, lesser_cycle, greater_cycle, drives)
//! ├── inter (bonds, bridges, top-5 resonances)
//! ├── extra (parent_chain, sub_holons, great_way)
//! ├── analogues (cross-domain type-homologues, max 10)
//! ├── provenance (last 5 events, evidence_count)
//! └── grounding (anchor_docs, epistemology_status)
//! ```
//!
//! ## Token-Budget Truncation
//!
//! Drop cheapest-to-lose first: analogues → provenance → resonances →
//! archetypal_loads detail. NEVER drop: synthesis_status, grounding, type_class.

use serde::{Deserialize, Serialize};

use crate::error::TdgResult;
use crate::holon::Holon;
use crate::metabolism::attractor::AttractorField;
use crate::metabolism::greater_cycle::GreaterCycleState;
use crate::metabolism::health::Health;
use crate::metabolism::lesser_cycle::LesserCycleState;
use crate::models::Node;

// ─── Types ───────────────────────────────────────────────────────────────────

/// The complete ContextPack — a structured projection of a holon's full
/// context for AI agent consumption.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextPack {
    // ─── Identity ───────────────────────────────────────────────────────────
    pub holon_id: String,
    pub node_type: String,
    pub name: String,
    pub scale_code: Option<String>,
    pub type_class: String,
    pub synthesis_status: String,
    /// Phase 13: Realm placement ("gross" | "subtle" | "causal").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realm_placement: Option<String>,
    /// Phase 13: Collectivity ("individual" | "collective" | "universal").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collectivity: Option<String>,

    // ─── Intra-holonic (interior state) ─────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intra: Option<IntraContext>,

    // ─── Inter-holonic (same-scale relationships) ───────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inter: Option<InterContext>,

    // ─── Extra-holonic (Great Way context) ──────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<ExtraContext>,

    // ─── Cross-domain analogues ─────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub analogues: Vec<Analogue>,

    // ─── Provenance ─────────────────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ProvenanceSummary>,

    // ─── Grounding (epistemic spine) ────────────────────────────────────────
    pub grounding: Grounding,
}

/// Intra-holonic context — the holon's interior state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntraContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attractor_field: Option<AttractorField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<Health>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lesser_cycle: Option<LesserCycleState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub greater_cycle: Option<GreaterCycleState>,
    pub drives: serde_json::Value,
    pub quadrants: serde_json::Value,
    pub developmental_stage: Option<i32>,
    pub teleological_level: Option<String>,
}

/// Inter-holonic context — same-scale relationships.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InterContext {
    /// Edges from this holon to others (bonds).
    pub bonds: Vec<BondSummary>,
    /// Top resonance partners (from resonance_graph).
    pub resonances: Vec<ResonanceSummary>,
}

/// A bond (edge) summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BondSummary {
    pub edge_type: String,
    pub target_id: String,
    pub target_name: String,
    pub target_type: String,
    pub weight: f64,
}

/// A resonance partner summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResonanceSummary {
    pub partner_id: String,
    pub resonance: f64,
    pub interpretation: String,
}

/// Extra-holonic context — Great Way / parent chain / sub-holons.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtraContext {
    /// Parent chain (canonical parent + ancestors).
    pub parent_chain: Vec<ParentSummary>,
    /// Sub-holons (children via DECOMPOSES_TO).
    pub sub_holons: Vec<SubHolonSummary>,
    /// Great Way trajectory (from greater cycle state).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub great_way: Option<GreatWaySummary>,
}

/// A parent in the chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParentSummary {
    pub holon_id: String,
    pub name: String,
    pub node_type: String,
    pub scale_code: Option<String>,
}

/// A sub-holon (child).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubHolonSummary {
    pub holon_id: String,
    pub name: String,
    pub node_type: String,
}

/// Great Way trajectory summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreatWaySummary {
    pub greater_phase: String,
    pub significator: f64,
    pub great_way: f64,
    pub transformation_pressure: f64,
    pub octave_count: u64,
}

/// A cross-domain analogue (same type_class, different scale).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Analogue {
    pub holon_id: String,
    pub name: String,
    pub node_type: String,
    pub scale_code: Option<String>,
    pub type_class: String,
    pub resonance: f64,
}

/// Provenance summary — last N events + evidence count.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvenanceSummary {
    pub recent_events: Vec<EventSummary>,
    pub evidence_count: i64,
    pub source: String,
}

/// An event in the provenance trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSummary {
    pub event_action: String,
    pub timestamp: String,
    pub agent_id: Option<String>,
}

/// Grounding — the epistemic spine. NEVER dropped during truncation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Grounding {
    /// The synthesis_status of this holon (always present).
    pub synthesis_status: String,
    /// Whether the holon's type_class is stable.
    pub is_stable: bool,
    /// Whether the holon is canonical (human-validated).
    pub is_canonical: bool,
    /// Epistemological status: "grounded" (canonical), "hypothesis-graded"
    /// (canonical-hypothesis), or "speculative" (ai-draft).
    pub epistemology_status: String,
}

// ─── Builder ─────────────────────────────────────────────────────────────────

/// Build a ContextPack for a holon.
///
/// # Arguments
/// * `conn` - SQLite connection
/// * `holon_id` - The holon to build context for
/// * `scope` - What to include: "intra", "inter", "extra", or "intra+inter+extra"
/// * `depth` - 0=identity, 1=+intra, 2=+inter+extra, 3=+analogues+provenance
/// * `token_budget` - Optional token budget (truncates cheapest-to-lose first)
pub fn build(
    conn: &rusqlite::Connection,
    holon_id: &str,
    scope: &str,
    depth: u8,
    token_budget: Option<usize>,
) -> TdgResult<ContextPack> {
    // Phase 13: Check cache first (5-min TTL)
    if let Some(cached) = check_cache(conn, holon_id, scope, depth, token_budget) {
        return Ok(cached);
    }

    // Load the node
    let node = crate::db::crud::get_node(conn, holon_id)?
        .ok_or_else(|| crate::error::TdgError::NotFound(holon_id.to_string()))?;

    let holon = Holon::new(&node);

    // Load attractor field (for type_class)
    let af = crate::metabolism::attractor::load(conn, holon_id)?;
    let type_class = af
        .as_ref()
        .map(|f| f.type_class.clone())
        .unwrap_or_else(|| "uncomputed".to_string());

    // Build identity (Phase 13: now includes realm_placement)
    let mut pack = ContextPack {
        holon_id: node.id.clone(),
        node_type: node.node_type.clone(),
        name: node.name.clone(),
        scale_code: node.scale_code.clone(),
        type_class: type_class.clone(),
        synthesis_status: node.synthesis_status.clone(),
        realm_placement: node.realm_placement.clone(),
        collectivity: node.collectivity.clone(),
        grounding: Grounding {
            synthesis_status: node.synthesis_status.clone(),
            is_stable: af.as_ref().map(|f| f.is_stable()).unwrap_or(false),
            is_canonical: holon.is_canonical(),
            epistemology_status: epistemology_status(&node.synthesis_status),
        },
        ..Default::default()
    };

    // Determine what to include based on scope and depth
    let _include_intra = scope.contains("intra") || scope == "all";
    let _include_inter = scope.contains("inter") || scope == "all";
    let _include_extra = scope.contains("extra") || scope == "all";

    // ─── Depth 1+: Intra ────────────────────────────────────────────────────
    if depth >= 1 {
        let health = crate::metabolism::health::load(conn, holon_id)?;
        let lesser = crate::metabolism::lesser_cycle::load_state(conn, holon_id)?;
        let greater = crate::metabolism::greater_cycle::load_state(conn, holon_id)?;

        pack.intra = Some(IntraContext {
            attractor_field: af.clone(),
            health,
            lesser_cycle: Some(lesser),
            greater_cycle: Some(greater),
            drives: node.drives.clone(),
            quadrants: node.quadrants.clone(),
            developmental_stage: node.developmental_stage,
            teleological_level: node.teleological_level.clone(),
        });
    }

    // ─── Depth 2+: Inter ────────────────────────────────────────────────────
    if depth >= 2 {
        let bonds = build_bonds(conn, holon_id)?;
        let resonances = build_resonances(conn, holon_id, 5)?;

        pack.inter = Some(InterContext { bonds, resonances });
    }

    // ─── Depth 2+: Extra ────────────────────────────────────────────────────
    if depth >= 2 {
        let parent_chain = build_parent_chain(conn, &node, 5)?;
        let sub_holons = build_sub_holons(conn, holon_id)?;
        let great_way = build_great_way(conn, holon_id)?;

        pack.extra = Some(ExtraContext {
            parent_chain,
            sub_holons,
            great_way,
        });
    }

    // ─── Depth 3+: Analogues + Provenance ───────────────────────────────────
    if depth >= 3 {
        pack.analogues = build_analogues(conn, holon_id, &type_class, 10)?;
        pack.provenance = Some(build_provenance(conn, holon_id)?);
    }

    // ─── Token-budget truncation ────────────────────────────────────────────
    if let Some(budget) = token_budget {
        apply_token_budget(&mut pack, budget);
    }

    // Phase 13: Save to cache
    save_cache(conn, holon_id, scope, depth, token_budget, &pack);

    Ok(pack)
}

/// Determine the epistemology status from synthesis_status.
fn epistemology_status(synthesis_status: &str) -> String {
    match synthesis_status {
        "canonical" => "grounded".to_string(),
        "canonical-hypothesis" => "hypothesis-graded".to_string(),
        "superseded" => "superseded".to_string(),
        _ => "speculative".to_string(), // ai-draft
    }
}

/// Build bond summaries (edges from this holon).
fn build_bonds(conn: &rusqlite::Connection, holon_id: &str) -> TdgResult<Vec<BondSummary>> {
    let edges = crate::db::crud::get_edges(conn, Some(holon_id), None, None, None, 20)?;

    let mut bonds = Vec::new();
    for edge in edges {
        if let Some(target) = crate::db::crud::get_node(conn, &edge.target_id)? {
            bonds.push(BondSummary {
                edge_type: edge.edge_type,
                target_id: target.id,
                target_name: target.name,
                target_type: target.node_type,
                weight: edge.weight,
            });
        }
    }
    Ok(bonds)
}

/// Build resonance summaries from the resonance_graph table.
fn build_resonances(
    conn: &rusqlite::Connection,
    holon_id: &str,
    limit: i64,
) -> TdgResult<Vec<ResonanceSummary>> {
    let mut stmt = conn.prepare(
        "SELECT partner_id, resonance_score
         FROM resonance_graph
         WHERE holon_id = ?1
         ORDER BY resonance_score DESC
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(rusqlite::params![holon_id, limit], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    })?;

    let mut resonances = Vec::new();
    for row in rows {
        if let Ok((partner_id, score)) = row {
            resonances.push(ResonanceSummary {
                partner_id,
                resonance: score,
                interpretation: crate::metabolism::health::interpret_resonance(score).to_string(),
            });
        }
    }
    Ok(resonances)
}

/// Build the parent chain (canonical parent + ancestors).
fn build_parent_chain(
    conn: &rusqlite::Connection,
    node: &Node,
    max_depth: usize,
) -> TdgResult<Vec<ParentSummary>> {
    let mut chain = Vec::new();
    let mut current = node.parent_ids.first().cloned();

    for _ in 0..max_depth {
        match current {
            Some(pid) => {
                if let Some(parent) = crate::db::crud::get_node(conn, &pid)? {
                    chain.push(ParentSummary {
                        holon_id: parent.id.clone(),
                        name: parent.name.clone(),
                        node_type: parent.node_type.clone(),
                        scale_code: parent.scale_code.clone(),
                    });
                    current = parent.parent_ids.first().cloned();
                } else {
                    break;
                }
            }
            None => break,
        }
    }
    Ok(chain)
}

/// Build sub-holon summaries (children via DECOMPOSES_TO).
fn build_sub_holons(
    conn: &rusqlite::Connection,
    holon_id: &str,
) -> TdgResult<Vec<SubHolonSummary>> {
    let edges =
        crate::db::crud::get_edges(conn, Some(holon_id), None, Some("DECOMPOSES_TO"), None, 20)?;

    let mut subs = Vec::new();
    for edge in edges {
        if let Some(child) = crate::db::crud::get_node(conn, &edge.target_id)? {
            subs.push(SubHolonSummary {
                holon_id: child.id,
                name: child.name,
                node_type: child.node_type,
            });
        }
    }
    Ok(subs)
}

/// Build the Great Way summary from the greater cycle state.
fn build_great_way(
    conn: &rusqlite::Connection,
    holon_id: &str,
) -> TdgResult<Option<GreatWaySummary>> {
    let greater = crate::metabolism::greater_cycle::load_state(conn, holon_id)?;

    // Only return if the holon has been touched (not default forming state)
    if greater.octave_count > 0 || greater.transformation_pressure > 0.0 {
        Ok(Some(GreatWaySummary {
            greater_phase: greater.phase.as_str().to_string(),
            significator: greater.significator,
            great_way: greater.great_way,
            transformation_pressure: greater.transformation_pressure,
            octave_count: greater.octave_count,
        }))
    } else {
        Ok(None)
    }
}

/// Build cross-domain analogues (same type_class, different holon).
fn build_analogues(
    conn: &rusqlite::Connection,
    holon_id: &str,
    type_class: &str,
    limit: i64,
) -> TdgResult<Vec<Analogue>> {
    if type_class.is_empty() || type_class == "uncomputed" || type_class == "dormant" {
        return Ok(Vec::new());
    }

    // Find holons with the same type_class (from their attractor_field_json)
    let mut stmt = conn.prepare(
        "SELECT id, name, node_type, scale_code, attractor_field_json
         FROM nodes
         WHERE id != ?1
           AND valid_to IS NULL
           AND attractor_field_json IS NOT NULL
           AND attractor_field_json LIKE ?2
         ORDER BY updated_at DESC
         LIMIT ?3",
    )?;

    let pattern = format!("%\"type_class\":\"{}\"%", type_class);
    let rows = stmt.query_map(rusqlite::params![holon_id, pattern, limit], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;

    let mut analogues = Vec::new();
    for row in rows {
        if let Ok((id, name, node_type, scale_code, af_json)) = row {
            // Parse the attractor field to get the type_class (verify match)
            if let Some(af) = AttractorField::from_json(&af_json) {
                if af.type_class == type_class {
                    // Compute resonance between this holon and the analogue
                    let self_af = crate::metabolism::attractor::load(conn, holon_id)?;
                    let resonance = if let Some(self_af) = self_af {
                        crate::metabolism::health::resonance(&self_af, &af)
                    } else {
                        0.0
                    };

                    analogues.push(Analogue {
                        holon_id: id,
                        name,
                        node_type,
                        scale_code,
                        type_class: af.type_class,
                        resonance,
                    });
                }
            }
        }
    }
    Ok(analogues)
}

/// Build provenance summary (last 5 events + evidence count).
fn build_provenance(conn: &rusqlite::Connection, holon_id: &str) -> TdgResult<ProvenanceSummary> {
    // Get recent events
    let mut stmt = conn.prepare(
        "SELECT event_action, timestamp, agent_id
         FROM events
         WHERE node_id = ?1
         ORDER BY timestamp DESC
         LIMIT 5",
    )?;

    let rows = stmt.query_map(rusqlite::params![holon_id], |row| {
        Ok(EventSummary {
            event_action: row.get(0)?,
            timestamp: row.get(1)?,
            agent_id: row.get(2)?,
        })
    })?;

    let mut events = Vec::new();
    for row in rows {
        if let Ok(event) = row {
            events.push(event);
        }
    }

    // Get evidence count (EVIDENCES edges pointing to this holon)
    let evidence_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edges WHERE target_id = ?1 AND edge_type = 'EVIDENCES' AND valid_to IS NULL",
        rusqlite::params![holon_id],
        |row| row.get(0),
    )?;

    // Get source
    let source: String = conn.query_row(
        "SELECT source FROM nodes WHERE id = ?1",
        rusqlite::params![holon_id],
        |row| row.get(0),
    )?;

    Ok(ProvenanceSummary {
        recent_events: events,
        evidence_count,
        source,
    })
}

/// Apply token-budget truncation.
///
/// Drop cheapest-to-lose first:
/// 1. analogues (beyond top 3)
/// 2. provenance (beyond top 3 events)
/// 3. resonances (beyond top 3)
/// 4. analogues (all)
/// 5. provenance (all)
/// 6. resonances (all)
///
/// NEVER drop: synthesis_status, grounding, type_class.
fn apply_token_budget(pack: &mut ContextPack, _budget: usize) {
    // For now, we use a simple heuristic: if the pack is large, trim.
    // A real implementation would count tokens with a tokenizer.
    // Since we're on a lean VPS, we just cap the sizes.

    // Cap analogues at 5
    if pack.analogues.len() > 5 {
        pack.analogues.truncate(5);
    }

    // Cap provenance events at 3
    if let Some(ref mut prov) = pack.provenance {
        if prov.recent_events.len() > 3 {
            prov.recent_events.truncate(3);
        }
    }

    // Cap resonances at 3
    if let Some(ref mut inter) = pack.inter {
        if inter.resonances.len() > 3 {
            inter.resonances.truncate(3);
        }
    }
}

impl ContextPack {
    /// Render the ContextPack as a markdown prompt block with [status: {status}] tags.
    ///
    /// This is what the agent sees in its context window.
    pub fn to_prompt_block(&self) -> String {
        let mut md = String::new();

        // ─── Identity ────────────────────────────────────────────────────────
        md.push_str(&format!(
            "## Holon: {} [status: {}]\n\n",
            self.name, self.synthesis_status
        ));
        md.push_str(&format!("- **ID**: {}\n", self.holon_id));
        md.push_str(&format!("- **Type**: {}\n", self.node_type));
        if let Some(scale) = &self.scale_code {
            md.push_str(&format!("- **Scale**: {}\n", scale));
        }
        if let Some(realm) = &self.realm_placement {
            md.push_str(&format!("- **Realm**: {}\n", realm));
        }
        if let Some(coll) = &self.collectivity {
            md.push_str(&format!("- **Collectivity**: {}\n", coll));
        }
        md.push_str(&format!(
            "- **Type class**: {} [status: hypothesis-graded]\n",
            self.type_class
        ));
        md.push_str(&format!(
            "- **Epistemology**: {}\n",
            self.grounding.epistemology_status
        ));
        md.push('\n');

        // ─── Intra ───────────────────────────────────────────────────────────
        if let Some(intra) = &self.intra {
            md.push_str("### Intra-holonic State\n\n");

            if let Some(health) = &intra.health {
                md.push_str(&format!(
                    "- **Health**: G_z={:.1}, P_z={:.1}, state={} [status: hypothesis-graded]\n",
                    health.g_z,
                    health.p_z,
                    health.state.as_str()
                ));
            }

            if let Some(lesser) = &intra.lesser_cycle {
                md.push_str(&format!(
                    "- **Lesser cycle**: phase={}, experience={:.2}, pressure={:.2}\n",
                    lesser.phase, lesser.experience_accumulated, lesser.transformation_pressure
                ));
                if let Some(shadow) = &lesser.matrix.shadow {
                    md.push_str(&format!("- **Matrix shadow**: {}\n", shadow.as_str()));
                }
            }

            if let Some(greater) = &intra.greater_cycle {
                md.push_str(&format!(
                    "- **Greater cycle**: phase={}, octave={}, pressure={:.2}\n",
                    greater.phase, greater.octave_count, greater.transformation_pressure
                ));
            }

            if let Some(af) = &intra.attractor_field {
                md.push_str(&format!(
                    "- **Attractor**: π={:?}, noble={}\n",
                    af.pi,
                    af.is_noble()
                ));
            }

            md.push('\n');
        }

        // ─── Inter ───────────────────────────────────────────────────────────
        if let Some(inter) = &self.inter {
            if !inter.bonds.is_empty() {
                md.push_str("### Bonds\n\n");
                for bond in inter.bonds.iter().take(5) {
                    md.push_str(&format!(
                        "- {} → {} ({})\n",
                        bond.edge_type, bond.target_name, bond.target_type
                    ));
                }
                md.push('\n');
            }

            if !inter.resonances.is_empty() {
                md.push_str("### Resonance Partners [status: hypothesis-graded]\n\n");
                for res in inter.resonances.iter().take(3) {
                    md.push_str(&format!(
                        "- {} (R={:.3}, {})\n",
                        res.partner_id, res.resonance, res.interpretation
                    ));
                }
                md.push('\n');
            }
        }

        // ─── Extra ───────────────────────────────────────────────────────────
        if let Some(extra) = &self.extra {
            if !extra.parent_chain.is_empty() {
                md.push_str("### Parent Chain\n\n");
                for parent in &extra.parent_chain {
                    md.push_str(&format!("- {} ({})\n", parent.name, parent.node_type));
                }
                md.push('\n');
            }

            if !extra.sub_holons.is_empty() {
                md.push_str("### Sub-holons\n\n");
                for sub in extra.sub_holons.iter().take(5) {
                    md.push_str(&format!("- {} ({})\n", sub.name, sub.node_type));
                }
                md.push('\n');
            }

            if let Some(gw) = &extra.great_way {
                md.push_str("### Great Way Trajectory\n\n");
                md.push_str(&format!("- Phase: {}\n", gw.greater_phase));
                md.push_str(&format!("- Octave: {}\n", gw.octave_count));
                md.push_str(&format!("- Pressure: {:.2}\n", gw.transformation_pressure));
                md.push('\n');
            }
        }

        // ─── Analogues ───────────────────────────────────────────────────────
        if !self.analogues.is_empty() {
            md.push_str("### Cross-domain Analogues [status: hypothesis-graded]\n\n");
            for analogue in self.analogues.iter().take(3) {
                md.push_str(&format!(
                    "- {} ({}) — type_class={}, R={:.3}\n",
                    analogue.name, analogue.node_type, analogue.type_class, analogue.resonance
                ));
            }
            md.push('\n');
        }

        // ─── Provenance ──────────────────────────────────────────────────────
        if let Some(prov) = &self.provenance {
            md.push_str("### Provenance\n\n");
            md.push_str(&format!("- Source: {}\n", prov.source));
            md.push_str(&format!("- Evidence count: {}\n", prov.evidence_count));
            if !prov.recent_events.is_empty() {
                md.push_str("- Recent events:\n");
                for event in prov.recent_events.iter().take(3) {
                    md.push_str(&format!(
                        "  - {} ({})\n",
                        event.event_action, event.timestamp
                    ));
                }
            }
            md.push('\n');
        }

        md
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

// ─── Phase 13: ContextPack Caching ───────────────────────────────────────────

const CACHE_TTL_SECS: i64 = 300; // 5 minutes

/// Check the context_cache table for a fresh entry.
/// Returns Some(ContextPack) if cached and fresh, None otherwise.
fn check_cache(
    conn: &rusqlite::Connection,
    holon_id: &str,
    scope: &str,
    depth: u8,
    token_budget: Option<usize>,
) -> Option<ContextPack> {
    let cache_key = format!(
        "{}:{}:{}:{}",
        holon_id,
        scope,
        depth,
        token_budget.unwrap_or(0)
    );

    let now_secs = chrono::Utc::now().timestamp();
    let cutoff = now_secs - CACHE_TTL_SECS;

    let json: Option<String> = conn
        .query_row(
            "SELECT context_json FROM context_cache
             WHERE cache_key = ?1 AND computed_at_secs > ?2",
            rusqlite::params![cache_key, cutoff],
            |row| row.get(0),
        )
        .ok();

    json.and_then(|s| serde_json::from_str(&s).ok())
}

/// Save a ContextPack to the cache.
fn save_cache(
    conn: &rusqlite::Connection,
    holon_id: &str,
    scope: &str,
    depth: u8,
    token_budget: Option<usize>,
    pack: &ContextPack,
) {
    let cache_key = format!(
        "{}:{}:{}:{}",
        holon_id,
        scope,
        depth,
        token_budget.unwrap_or(0)
    );

    let now_secs = chrono::Utc::now().timestamp();
    let json = serde_json::to_string(pack).unwrap_or_default();

    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS context_cache (
            cache_key TEXT PRIMARY KEY,
            holon_id TEXT NOT NULL,
            context_json TEXT NOT NULL,
            computed_at_secs INTEGER NOT NULL
        )",
        [],
    );

    let _ = conn.execute(
        "INSERT OR REPLACE INTO context_cache (cache_key, holon_id, context_json, computed_at_secs)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![cache_key, holon_id, json, now_secs],
    );
}

/// Invalidate cache entries for a holon (called on writes).
pub fn invalidate_cache(conn: &rusqlite::Connection, holon_id: &str) {
    let _ = conn.execute(
        "DELETE FROM context_cache WHERE holon_id = ?1",
        rusqlite::params![holon_id],
    );
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
    fn build_context_pack_identity_only() {
        let conn = setup_db();
        let node = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Test observation".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let pack = build(&conn, &node.id, "intra", 0, None).unwrap();

        assert_eq!(pack.holon_id, node.id);
        assert_eq!(pack.node_type, "observation");
        assert_eq!(pack.name, "Test observation");
        assert_eq!(pack.synthesis_status, "ai-draft");
        assert_eq!(pack.grounding.epistemology_status, "speculative");
        assert!(pack.intra.is_none()); // depth 0 = identity only
    }

    #[test]
    fn build_context_pack_with_intra() {
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

        let pack = build(&conn, &node.id, "intra", 1, None).unwrap();

        assert!(pack.intra.is_some());
        let intra = pack.intra.unwrap();
        assert!(intra.lesser_cycle.is_some());
        assert!(intra.greater_cycle.is_some());
    }

    #[test]
    fn build_context_pack_full_depth() {
        let conn = setup_db();

        // Create a parent and child
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

        // Connect parent to child via DECOMPOSES_TO
        let _ = crate::db::crud::add_edge(
            &conn,
            &crate::models::NewEdge {
                source_id: parent.id.clone(),
                target_id: child.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        );

        let pack = build(&conn, &child.id, "intra+inter+extra", 3, None).unwrap();

        // Should have all sections
        assert!(pack.intra.is_some());
        assert!(pack.inter.is_some());
        assert!(pack.extra.is_some());

        // Parent chain should include the parent
        let extra = pack.extra.unwrap();
        assert_eq!(extra.parent_chain.len(), 1);
        assert_eq!(extra.parent_chain[0].name, "Parent");

        // Sub-holons of the parent should include the child
        let parent_pack = build(&conn, &parent.id, "extra", 3, None).unwrap();
        let parent_extra = parent_pack.extra.unwrap();
        assert!(!parent_extra.sub_holons.is_empty());
    }

    #[test]
    fn prompt_block_includes_status_tags() {
        let conn = setup_db();
        let node = crate::db::crud::add_node(
            &conn,
            &NewNode {
                node_type: "hypothesis".to_string(),
                name: "Test hypothesis".to_string(),
                synthesis_status: Some("canonical-hypothesis".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let pack = build(&conn, &node.id, "intra", 1, None).unwrap();
        let md = pack.to_prompt_block();

        assert!(md.contains("[status: canonical-hypothesis]"));
        assert!(md.contains("## Holon: Test hypothesis"));
    }

    #[test]
    fn epistemology_status_mapping() {
        assert_eq!(epistemology_status("canonical"), "grounded");
        assert_eq!(
            epistemology_status("canonical-hypothesis"),
            "hypothesis-graded"
        );
        assert_eq!(epistemology_status("ai-draft"), "speculative");
        assert_eq!(epistemology_status("superseded"), "superseded");
    }

    #[test]
    fn token_budget_truncates_analogues() {
        let mut pack = ContextPack {
            holon_id: "test".to_string(),
            node_type: "observation".to_string(),
            name: "Test".to_string(),
            scale_code: Some("S40".to_string()),
            type_class: "sharer".to_string(),
            synthesis_status: "ai-draft".to_string(),
            analogues: (0..10)
                .map(|i| Analogue {
                    holon_id: format!("h{}", i),
                    name: format!("Analogue {}", i),
                    node_type: "observation".to_string(),
                    scale_code: Some("S40".to_string()),
                    type_class: "sharer".to_string(),
                    resonance: 0.5,
                })
                .collect(),
            grounding: Grounding {
                synthesis_status: "ai-draft".to_string(),
                is_stable: true,
                is_canonical: false,
                epistemology_status: "speculative".to_string(),
            },
            ..Default::default()
        };

        apply_token_budget(&mut pack, 1000);
        assert!(pack.analogues.len() <= 5);
    }

    #[test]
    fn json_serialization() {
        let pack = ContextPack {
            holon_id: "test".to_string(),
            node_type: "observation".to_string(),
            name: "Test".to_string(),
            scale_code: Some("S40".to_string()),
            type_class: "sharer".to_string(),
            synthesis_status: "ai-draft".to_string(),
            grounding: Grounding {
                synthesis_status: "ai-draft".to_string(),
                is_stable: true,
                is_canonical: false,
                epistemology_status: "speculative".to_string(),
            },
            ..Default::default()
        };

        let json = pack.to_json();
        assert!(json.contains("\"holon_id\":\"test\""));
        assert!(json.contains("\"synthesis_status\":\"ai-draft\""));
    }
}
