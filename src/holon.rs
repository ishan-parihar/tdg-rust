//! Holon — a first-class whole/part primitive wrapping `Node`.
//!
//! In TDG theory, a holon is a whole that is also a part. Every holon runs
//! the invariant dual-metabolic architecture (lesser cycle M·P·C·E + greater
//! cycle S·T·G·Ch) through a shared contact boundary.
//!
//! `Holon` is a zero-cost newtype over `Node` that adds compositional methods
//! for navigating the holarchy (parent/child relationships) and querying
//! holonic identity (scale, type, status). It does NOT duplicate the `Node`
//! data — it borrows it.
//!
//! ## Design
//!
//! `Holon` is intentionally lightweight: it holds a reference to a `Node` and
//! a database connection. All methods are computed on demand from the graph.
//! This keeps memory usage low (important for the 2GB VPS target) while
//! providing a clean API for holonic operations.
//!
//! ## Future Phases
//!
//! - Phase 2: lesser cycle state (M·P·C·E) will be added as a computed field
//! - Phase 3: attractor field A(H) = ⟨A_M, A_P, A_G, Γ⟩ will be computed
//! - Phase 4: greater cycle state (S·T·G·Ch) will be tracked
//! - Phase 6: type_class classification will be derived from the attractor field

use rusqlite::Connection;

use crate::db::crud;
use crate::error::TdgResult;
use crate::models::{Node, SynthesisStatus};

/// A holon — a whole that is also a part.
///
/// Wraps a `Node` with holonic navigation methods. Zero-cost: the wrapper
/// exists only at the type level; the data is a borrowed `Node`.
#[derive(Debug, Clone)]
pub struct Holon<'a> {
    pub node: &'a Node,
}

impl<'a> Holon<'a> {
    /// Create a `Holon` view over a borrowed `Node`.
    pub fn new(node: &'a Node) -> Self {
        Self { node }
    }

    /// The holon's unique identifier.
    pub fn id(&self) -> &str {
        &self.node.id
    }

    /// The holon's type (e.g. "observation", "telos", "skill").
    pub fn node_type(&self) -> &str {
        &self.node.node_type
    }

    /// The holon's display name.
    pub fn name(&self) -> &str {
        &self.node.name
    }

    // ─── Identity ───────────────────────────────────────────────────────────

    /// The epistemic status of this holon on the TDG ladder.
    pub fn synthesis_status(&self) -> Option<SynthesisStatus> {
        SynthesisStatus::from_str(&self.node.synthesis_status)
    }

    /// Whether this holon is canonical (human-validated).
    pub fn is_canonical(&self) -> bool {
        self.synthesis_status() == Some(SynthesisStatus::Canonical)
    }

    /// Whether this holon is an AI draft (not yet validated).
    pub fn is_ai_draft(&self) -> bool {
        self.synthesis_status() == Some(SynthesisStatus::AiDraft)
    }

    /// The organisational scale code (e.g. "S11" for Civilization).
    pub fn scale_code(&self) -> Option<&str> {
        self.node.scale_code.as_deref()
    }

    /// The human-readable scale name (e.g. "Civilization").
    pub fn scale_name(&self) -> Option<&'static str> {
        self.node.scale_code.as_deref().and_then(crate::scale_codes::scale_name)
    }

    /// The Tetra-Axes coordinates (UL, UR, LL, LR), each 1-19.
    pub fn tetra_coords(&self) -> (Option<i32>, Option<i32>, Option<i32>, Option<i32>) {
        (
            self.node.tetra_ul,
            self.node.tetra_ur,
            self.node.tetra_ll,
            self.node.tetra_lr,
        )
    }

    /// The octave identifier for cross-octave involution lineage.
    pub fn octave_id(&self) -> Option<&str> {
        self.node.octave_id.as_deref()
    }

    // ─── Compositional Algebra (whole/part) ────────────────────────────────

    /// The canonical parent (first in `parent_ids`).
    ///
    /// In TDG theory, every holon is a part of a larger whole. The canonical
    /// parent is the primary "whole" this holon belongs to.
    pub fn canonical_parent_id(&self) -> Option<&str> {
        self.node.parent_ids.first().map(|s| s.as_str())
    }

    /// All parent IDs (this holon may be part of multiple wholes).
    pub fn all_parent_ids(&self) -> &[String] {
        &self.node.parent_ids
    }

    /// Whether this holon is a part (has parents).
    pub fn is_part(&self) -> bool {
        !self.node.parent_ids.is_empty()
    }

    /// Load the canonical parent holon from the database, if it exists.
    pub fn canonical_parent(&self, conn: &Connection) -> TdgResult<Option<Node>> {
        match self.canonical_parent_id() {
            Some(pid) => crud::get_node(conn, pid),
            None => Ok(None),
        }
    }

    /// Load all parent holons from the database.
    pub fn all_parents(&self, conn: &Connection) -> TdgResult<Vec<Node>> {
        let mut parents = Vec::new();
        for pid in &self.node.parent_ids {
            if let Some(node) = crud::get_node(conn, pid)? {
                parents.push(node);
            }
        }
        Ok(parents)
    }

    /// Load the children of this holon (nodes that have this holon in their
    /// `parent_ids`). Uses DECOMPOSES_TO edges for structural children.
    pub fn children(&self, conn: &Connection) -> TdgResult<Vec<Node>> {
        // Query for nodes where this holon's ID appears in their parent_ids
        let mut stmt = conn.prepare(
            "SELECT id, node_type, name, description, properties_json, quadrants_json,
             drives_json, lifecycle_state, teleological_level, developmental_stage,
             confidence, source, parent_ids, agent_path, created_at, updated_at,
             valid_from, valid_to, helpful_count, retrieval_count, agent_id,
             synthesis_status, scale_code, tetra_ul, tetra_ur, tetra_ll, tetra_lr, octave_id
             FROM nodes
             WHERE valid_to IS NULL AND parent_ids LIKE ?1
             ORDER BY created_at DESC",
        )?;
        let pattern = format!("%\"{}\"%", self.node.id);
        let rows = stmt.query_map(rusqlite::params![pattern], crud::row_to_node)?;
        let mut children = Vec::new();
        for row in rows {
            if let Ok(node) = row {
                // Verify the ID is actually in parent_ids (not just a substring match)
                if node.parent_ids.contains(&self.node.id) {
                    children.push(node);
                }
            }
        }
        Ok(children)
    }

    /// Whether this holon is a whole (has children).
    ///
    /// Note: this requires a database query. Use `children(conn).is_empty()`
    /// if you already need the children.
    pub fn is_whole(&self, conn: &Connection) -> TdgResult<bool> {
        Ok(!self.children(conn)?.is_empty())
    }

    /// Load sub-holons connected via DECOMPOSES_TO edges (structural decomposition).
    pub fn sub_holons(&self, conn: &Connection) -> TdgResult<Vec<Node>> {
        let edges = crud::get_edges(conn, Some(&self.node.id), None, Some("DECOMPOSES_TO"), None, 100)?;
        let mut subs = Vec::new();
        for edge in edges {
            if let Some(node) = crud::get_node(conn, &edge.target_id)? {
                subs.push(node);
            }
        }
        Ok(subs)
    }

    // ─── Developmental State ────────────────────────────────────────────────

    /// The developmental stage (1-8), if assigned.
    pub fn developmental_stage(&self) -> Option<u8> {
        self.node.developmental_stage.map(|s| s as u8)
    }

    /// The teleological level (T0-T6), if assigned.
    pub fn teleological_level(&self) -> Option<&str> {
        self.node.teleological_level.as_deref()
    }

    /// The lifecycle state ("active", "archived", etc.).
    pub fn lifecycle_state(&self) -> &str {
        &self.node.lifecycle_state
    }

    /// Whether this holon is active (not archived/deleted).
    pub fn is_active(&self) -> bool {
        self.node.valid_to.is_none() && self.node.lifecycle_state == "active"
    }
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
    fn holon_basic_identity() {
        let conn = setup_db();
        let node = crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Test observation".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let holon = Holon::new(&node);
        assert_eq!(holon.node_type(), "observation");
        assert_eq!(holon.name(), "Test observation");
        assert!(holon.is_active());
        assert!(holon.is_ai_draft()); // default synthesis_status
        assert!(!holon.is_canonical());
        assert_eq!(holon.scale_code(), Some("S40")); // inferred from node_type
        assert_eq!(holon.scale_name(), Some("Individual"));
    }

    #[test]
    fn holon_synthesis_status() {
        let conn = setup_db();
        let node = crud::add_node(
            &conn,
            &NewNode {
                node_type: "hypothesis".to_string(),
                name: "Test hypothesis".to_string(),
                synthesis_status: Some("canonical-hypothesis".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let holon = Holon::new(&node);
        assert_eq!(
            holon.synthesis_status(),
            Some(SynthesisStatus::CanonicalHypothesis)
        );
        assert!(!holon.is_ai_draft());
        assert!(!holon.is_canonical());
    }

    #[test]
    fn holon_compositional_algebra() {
        let conn = setup_db();
        // Create a parent (telos)
        let parent = crud::add_node(
            &conn,
            &NewNode {
                node_type: "telos".to_string(),
                name: "Parent telos".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        // Create a child observation with the parent
        let child = crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Child observation".to_string(),
                parent_ids: Some(vec![parent.id.clone()]),
                ..Default::default()
            },
        )
        .unwrap();

        let child_holon = Holon::new(&child);
        assert!(child_holon.is_part());
        assert_eq!(child_holon.canonical_parent_id(), Some(parent.id.as_str()));

        let loaded_parent = child_holon.canonical_parent(&conn).unwrap();
        assert!(loaded_parent.is_some());
        assert_eq!(loaded_parent.unwrap().id, parent.id);

        // Parent should be a whole (has children)
        let parent_holon = Holon::new(&parent);
        assert!(parent_holon.is_whole(&conn).unwrap());
        let children = parent_holon.children(&conn).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, child.id);
    }

    #[test]
    fn holon_scale_code_inference() {
        let conn = setup_db();
        let project = crud::add_node(
            &conn,
            &NewNode {
                node_type: "project".to_string(),
                name: "Test project".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(Holon::new(&project).scale_code(), Some("S30"));

        let artifact = crud::add_node(
            &conn,
            &NewNode {
                node_type: "artifact".to_string(),
                name: "Test artifact".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(Holon::new(&artifact).scale_code(), Some("S50"));
    }

    #[test]
    fn holon_tetra_coords() {
        let conn = setup_db();
        let node = crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Test".to_string(),
                tetra_ul: Some(5),
                tetra_ur: Some(10),
                tetra_ll: Some(15),
                tetra_lr: Some(19),
                ..Default::default()
            },
        )
        .unwrap();

        let holon = Holon::new(&node);
        assert_eq!(holon.tetra_coords(), (Some(5), Some(10), Some(15), Some(19)));
    }

    #[test]
    fn holon_octave_id() {
        let conn = setup_db();
        let node = crud::add_node(
            &conn,
            &NewNode {
                node_type: "observation".to_string(),
                name: "Previous octave".to_string(),
                octave_id: Some("N-1".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let holon = Holon::new(&node);
        assert_eq!(holon.octave_id(), Some("N-1"));
    }
}
