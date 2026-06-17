//! Score reconciliation: 5-layer provenance-aware scoring.
//!
//! Ported from Python `tdg_score_reconciler.py` (395 lines).

use std::collections::HashMap;

use rusqlite::Connection;

use crate::error::TdgResult;

/// Source layers for score provenance, with confidence weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceLayer {
    /// Event trajectory analysis (confidence: 0.60)
    EventTrajectory,
    /// Entity span extraction (confidence: 0.50)
    EntitySpan,
    /// Structural distribution analysis (confidence: 0.45)
    StructuralDistribution,
    /// Anomaly inversion detection (confidence: 0.70)
    AnomalyInversion,
    /// Manual override (confidence: 0.95)
    Override,
}

impl SourceLayer {
    /// Get the confidence weight for this layer.
    pub fn confidence(&self) -> f64 {
        match self {
            Self::EventTrajectory => 0.60,
            Self::EntitySpan => 0.50,
            Self::StructuralDistribution => 0.45,
            Self::AnomalyInversion => 0.70,
            Self::Override => 0.95,
        }
    }

    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "event_trajectory" => Some(Self::EventTrajectory),
            "entity_span" => Some(Self::EntitySpan),
            "structural_distribution" => Some(Self::StructuralDistribution),
            "anomaly_inversion" => Some(Self::AnomalyInversion),
            "override" => Some(Self::Override),
            _ => None,
        }
    }

    /// String representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EventTrajectory => "event_trajectory",
            Self::EntitySpan => "entity_span",
            Self::StructuralDistribution => "structural_distribution",
            Self::AnomalyInversion => "anomaly_inversion",
            Self::Override => "override",
        }
    }
}

/// A score with provenance information.
#[derive(Debug, Clone)]
pub struct ProvenancedScore {
    pub value: f64,
    pub source_layer: SourceLayer,
    pub confidence: f64,
    pub frozen: bool,
    pub fallback_reason: Option<String>,
    pub computed_at: String,
    pub drive_name: String,
    pub node_id: String,
}

/// Threshold for detecting frozen reconciliation (same score N cycles).
pub const FROZEN_CYCLE_THRESHOLD: usize = 5;

/// Reconciliation engine that merges scores from multiple source layers.
pub struct ScoreReconciliationEngine {
    /// History of scores per (node_id, drive_name) for freeze detection.
    score_history: HashMap<(String, String), Vec<f64>>,
}

impl ScoreReconciliationEngine {
    /// Create a new reconciliation engine.
    pub fn new() -> Self {
        Self {
            score_history: HashMap::new(),
        }
    }

    /// Reconcile scores from multiple layers into a single provenanced score.
    ///
    /// Weighted average by confidence, with override layer taking precedence.
    pub fn reconcile_score(
        &mut self,
        node_id: &str,
        drive_name: &str,
        layer_scores: &[(SourceLayer, f64)],
    ) -> ProvenancedScore {
        let now = chrono::Utc::now().to_rfc3339();

        // Check for override
        for (layer, value) in layer_scores {
            if *layer == SourceLayer::Override {
                return ProvenancedScore {
                    value: *value,
                    source_layer: SourceLayer::Override,
                    confidence: SourceLayer::Override.confidence(),
                    frozen: false,
                    fallback_reason: None,
                    computed_at: now,
                    drive_name: drive_name.to_string(),
                    node_id: node_id.to_string(),
                };
            }
        }

        // Weighted average
        let mut total_weight = 0.0;
        let mut weighted_sum = 0.0;
        let mut best_layer = SourceLayer::StructuralDistribution;
        let mut best_confidence = 0.0;

        for (layer, value) in layer_scores {
            let weight = layer.confidence();
            total_weight += weight;
            weighted_sum += value * weight;

            if weight > best_confidence {
                best_confidence = weight;
                best_layer = *layer;
            }
        }

        let final_value = if total_weight > 0.0 {
            weighted_sum / total_weight
        } else {
            0.0
        };

        // Track history for freeze detection
        let key = (node_id.to_string(), drive_name.to_string());
        let history = self.score_history.entry(key).or_default();
        history.push(final_value);
        if history.len() > FROZEN_CYCLE_THRESHOLD * 2 {
            history.remove(0);
        }

        // Check for frozen reconciliation
        let frozen = self.detect_frozen(node_id, drive_name);

        ProvenancedScore {
            value: final_value,
            source_layer: best_layer,
            confidence: best_confidence,
            frozen,
            fallback_reason: if layer_scores.is_empty() {
                Some("No source layers provided".into())
            } else {
                None
            },
            computed_at: now,
            drive_name: drive_name.to_string(),
            node_id: node_id.to_string(),
        }
    }

    /// Detect if scores have been frozen (unchanged for N cycles).
    pub fn detect_frozen(&self, node_id: &str, drive_name: &str) -> bool {
        let key = (node_id.to_string(), drive_name.to_string());
        let history = match self.score_history.get(&key) {
            Some(h) => h,
            None => return false,
        };

        if history.len() < FROZEN_CYCLE_THRESHOLD {
            return false;
        }

        // Check last N scores are identical
        let recent = &history[history.len() - FROZEN_CYCLE_THRESHOLD..];
        let first = recent[0];
        recent.iter().all(|&v| (v - first).abs() < f64::EPSILON)
    }

    /// Reconcile scores for all nodes of a given drive.
    pub fn reconcile_all_nodes(
        &mut self,
        conn: &Connection,
        drive_name: &str,
    ) -> TdgResult<Vec<ProvenancedScore>> {
        // Query nodes that have this drive
        let nodes: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT id, drives_json FROM nodes
                 WHERE lifecycle_state = 'active'"
            )?;

            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let drives_json: String = row.get(1)?;
                Ok((id, drives_json))
            })?;

            rows.filter_map(|r| r.ok())
                .filter(|(_, drives_json)| {
                    drives_json.contains(drive_name)
                })
                .collect()
        };

        let mut scores = Vec::new();

        for (node_id, _) in nodes {
            // For now, use structural distribution as default layer
            let score = self.reconcile_score(
                &node_id,
                drive_name,
                &[(SourceLayer::StructuralDistribution, 0.5)],
            );
            scores.push(score);
        }

        Ok(scores)
    }

    /// Get a summary of reconciliation state.
    pub fn get_summary(&self) -> HashMap<String, usize> {
        let mut summary = HashMap::new();
        summary.insert("total_tracked".into(), self.score_history.len());

        let frozen_count = self
            .score_history
            .iter()
            .filter(|(k, _)| self.detect_frozen(&k.0, &k.1))
            .count();
        summary.insert("frozen".into(), frozen_count);

        summary
    }
}

impl Default for ScoreReconciliationEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconcile_single_layer() {
        let mut engine = ScoreReconciliationEngine::new();
        let score = engine.reconcile_score(
            "node1",
            "cognitive",
            &[(SourceLayer::EventTrajectory, 0.8)],
        );

        assert!((score.value - 0.8).abs() < f64::EPSILON);
        assert_eq!(score.source_layer, SourceLayer::EventTrajectory);
        assert!(!score.frozen);
    }

    #[test]
    fn test_reconcile_weighted_average() {
        let mut engine = ScoreReconciliationEngine::new();
        let score = engine.reconcile_score(
            "node1",
            "cognitive",
            &[
                (SourceLayer::EventTrajectory, 0.8),  // 0.60 weight
                (SourceLayer::EntitySpan, 0.4),       // 0.50 weight
            ],
        );

        // Weighted: (0.8*0.60 + 0.4*0.50) / (0.60+0.50) = (0.48+0.20)/1.10 = 0.618
        let expected = (0.8 * 0.60 + 0.4 * 0.50) / (0.60 + 0.50);
        assert!((score.value - expected).abs() < 0.001);
    }

    #[test]
    fn test_reconcile_override_takes_precedence() {
        let mut engine = ScoreReconciliationEngine::new();
        let score = engine.reconcile_score(
            "node1",
            "cognitive",
            &[
                (SourceLayer::EventTrajectory, 0.2),
                (SourceLayer::Override, 0.9),
            ],
        );

        assert!((score.value - 0.9).abs() < f64::EPSILON);
        assert_eq!(score.source_layer, SourceLayer::Override);
    }

    #[test]
    fn test_freeze_detection() {
        let mut engine = ScoreReconciliationEngine::new();

        // Feed same score 5 times
        for _ in 0..FROZEN_CYCLE_THRESHOLD {
            engine.reconcile_score(
                "node1",
                "cognitive",
                &[(SourceLayer::EventTrajectory, 0.5)],
            );
        }

        assert!(engine.detect_frozen("node1", "cognitive"));
    }

    #[test]
    fn test_freeze_not_triggered_with_variation() {
        let mut engine = ScoreReconciliationEngine::new();

        // Feed varying scores
        for i in 0..FROZEN_CYCLE_THRESHOLD {
            let value = 0.5 + (i as f64 * 0.1);
            engine.reconcile_score(
                "node1",
                "cognitive",
                &[(SourceLayer::EventTrajectory, value)],
            );
        }

        assert!(!engine.detect_frozen("node1", "cognitive"));
    }

    #[test]
    fn test_source_layer_confidence() {
        assert!((SourceLayer::Override.confidence() - 0.95).abs() < f64::EPSILON);
        assert!((SourceLayer::AnomalyInversion.confidence() - 0.70).abs() < f64::EPSILON);
        assert!((SourceLayer::EventTrajectory.confidence() - 0.60).abs() < f64::EPSILON);
        assert!((SourceLayer::EntitySpan.confidence() - 0.50).abs() < f64::EPSILON);
        assert!((SourceLayer::StructuralDistribution.confidence() - 0.45).abs() < f64::EPSILON);
    }

    #[test]
    fn test_summary() {
        let mut engine = ScoreReconciliationEngine::new();
        engine.reconcile_score("node1", "cognitive", &[(SourceLayer::EventTrajectory, 0.5)]);
        engine.reconcile_score("node2", "cognitive", &[(SourceLayer::EventTrajectory, 0.5)]);

        let summary = engine.get_summary();
        assert_eq!(summary.get("total_tracked"), Some(&2));
    }
}
