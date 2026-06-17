//! Metrics Engine — performance tracking and wisdom detection
//!
//! Port of `core/mind/metrics_engine.py`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};


/// Per-action performance metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionMetrics {
    pub cycles: i64,
    pub conversions: i64,
    pub quality_sum: f64,
    pub quality_count: i64,
    pub accuracy_sum: f64,
    pub accuracy_count: i64,
    pub total_revenue: f64,
    pub first_seen: String,
    pub last_seen: String,
}

impl Default for ActionMetrics {
    fn default() -> Self {
        Self {
            cycles: 0,
            conversions: 0,
            quality_sum: 0.0,
            quality_count: 0,
            accuracy_sum: 0.0,
            accuracy_count: 0,
            total_revenue: 0.0,
            first_seen: String::new(),
            last_seen: String::new(),
        }
    }
}

/// Wisdom node detected from pattern analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WisdomNode {
    pub category: String,
    pub message: String,
    pub severity: String,
    pub action_type: Option<String>,
    pub detected_at: String,
}

/// Full metrics state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsState {
    pub schema_version: String,
    pub total_cycles: i64,
    pub total_revenue: f64,
    pub total_conversions: i64,
    pub action_types: HashMap<String, ActionMetrics>,
    pub quadrant_history: Vec<String>,
    pub drive_history: Vec<String>,
    pub wisdom_nodes: Vec<WisdomNode>,
}

impl Default for MetricsState {
    fn default() -> Self {
        Self {
            schema_version: "2.0.0".to_string(),
            total_cycles: 0,
            total_revenue: 0.0,
            total_conversions: 0,
            action_types: HashMap::new(),
            quadrant_history: Vec::new(),
            drive_history: Vec::new(),
            wisdom_nodes: Vec::new(),
        }
    }
}

/// The Metrics Engine — manages performance tracking data.
pub struct MetricsEngine {
    state: MetricsState,
    max_history: usize,
}

impl MetricsEngine {
    pub fn new() -> Self {
        Self {
            state: MetricsState::default(),
            max_history: 1000,
        }
    }

    pub fn with_state(state: MetricsState) -> Self {
        Self {
            state,
            max_history: 1000,
        }
    }

    pub fn state(&self) -> &MetricsState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut MetricsState {
        &mut self.state
    }

    /// Record a single cycle with optional action type, quadrant, quality, and drive.
    pub fn record_cycle(
        &mut self,
        action_type: Option<&str>,
        quadrant: Option<&str>,
        drive: Option<&str>,
        quality: Option<f64>,
    ) {
        self.state.total_cycles += 1;
        let now = crate::db::crud::now_iso();

        // Update action metrics
        if let Some(at) = action_type {
            let metrics = self
                .state
                .action_types
                .entry(at.to_string())
                .or_insert_with(|| ActionMetrics {
                    first_seen: now.clone(),
                    ..Default::default()
                });
            metrics.cycles += 1;
            metrics.last_seen = now;
            if let Some(q) = quality {
                metrics.quality_sum += q;
                metrics.quality_count += 1;
            }
        }

        // Update quadrant history
        if let Some(q) = quadrant {
            self.state.quadrant_history.push(q.to_string());
            if self.state.quadrant_history.len() > self.max_history {
                self.state.quadrant_history.remove(0);
            }
        }

        // Update drive history
        if let Some(d) = drive {
            self.state.drive_history.push(d.to_string());
            if self.state.drive_history.len() > self.max_history {
                self.state.drive_history.remove(0);
            }
        }
    }

    /// Record a conversion.
    pub fn record_conversion(&mut self, action_type: &str, revenue: f64) {
        self.state.total_conversions += 1;
        self.state.total_revenue += revenue;
        if let Some(metrics) = self.state.action_types.get_mut(action_type) {
            metrics.conversions += 1;
            metrics.total_revenue += revenue;
        }
    }

    /// Detect wisdom patterns from accumulated metrics.
    pub fn detect_wisdom(&mut self) -> Vec<WisdomNode> {
        let mut wisdom = Vec::new();
        let now = crate::db::crud::now_iso();

        // High effort, zero return
        for (at, metrics) in &self.state.action_types {
            if metrics.cycles >= 10 && metrics.conversions == 0 {
                wisdom.push(WisdomNode {
                    category: "high_effort_zero_return".to_string(),
                    message: format!(
                        "Action '{}' has {} cycles with 0% conversion — consider deprioritizing",
                        at, metrics.cycles
                    ),
                    severity: "warning".to_string(),
                    action_type: Some(at.clone()),
                    detected_at: now.clone(),
                });
            }
        }

        // Low quality
        for (at, metrics) in &self.state.action_types {
            if metrics.quality_count >= 5 {
                let avg_quality = metrics.quality_sum / metrics.quality_count as f64;
                if avg_quality < 3.0 {
                    wisdom.push(WisdomNode {
                        category: "low_quality".to_string(),
                        message: format!(
                            "Action '{}' avg quality {:.1} < 3.0 threshold",
                            at, avg_quality
                        ),
                        severity: "warning".to_string(),
                        action_type: Some(at.clone()),
                        detected_at: now.clone(),
                    });
                }
            }
        }

        // Quadrant imbalance
        if self.state.quadrant_history.len() >= 50 {
            let recent: Vec<&str> = self.state.quadrant_history
                .iter()
                .rev()
                .take(50)
                .map(|s| s.as_str())
                .collect();
            let mut counts: HashMap<&str, i64> = HashMap::new();
            for q in &recent {
                *counts.entry(q).or_insert(0) += 1;
            }
            for (q, count) in &counts {
                if (*count as f64 / 50.0) > 0.4 {
                    wisdom.push(WisdomNode {
                        category: "quadrant_imbalance".to_string(),
                        message: format!(
                            "Quadrant '{}' at {:.0}% over last 50 cycles — rebalance",
                            q,
                            (*count as f64 / 50.0) * 100.0
                        ),
                        severity: "warning".to_string(),
                        action_type: None,
                        detected_at: now.clone(),
                    });
                }
            }
        }

        self.state.wisdom_nodes = wisdom.clone();
        wisdom
    }

    /// Get a compact summary for prompt injection.
    pub fn get_summary(&self) -> serde_json::Value {
        let action_summary: HashMap<String, serde_json::Value> = self
            .state
            .action_types
            .iter()
            .map(|(at, m)| {
                let avg_quality = if m.quality_count > 0 {
                    m.quality_sum / m.quality_count as f64
                } else {
                    0.0
                };
                (at.clone(), serde_json::json!({
                    "cycles": m.cycles,
                    "conversions": m.conversions,
                    "avg_quality": (avg_quality * 10.0).round() / 10.0,
                    "revenue": m.total_revenue,
                }))
            })
            .collect();

        // Quadrant balance (last 50)
        let recent_quads: Vec<&str> = self.state.quadrant_history
            .iter()
            .rev()
            .take(50)
            .map(|s| s.as_str())
            .collect();
        let total = recent_quads.len().max(1) as f64;
        let mut quad_balance: HashMap<String, f64> = HashMap::new();
        for q in &recent_quads {
            *quad_balance.entry(q.to_string()).or_insert(0.0) += 1.0;
        }
        for v in quad_balance.values_mut() {
            *v = (*v / total * 100.0 * 10.0).round() / 10.0;
        }

        serde_json::json!({
            "total_cycles": self.state.total_cycles,
            "total_revenue": self.state.total_revenue,
            "total_conversions": self.state.total_conversions,
            "action_types": action_summary,
            "quadrant_balance": quad_balance,
            "wisdom_count": self.state.wisdom_nodes.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_cycle_basic() {
        let mut engine = MetricsEngine::new();
        engine.record_cycle(Some("observe"), Some("UL"), Some("eros"), Some(7.0));
        engine.record_cycle(Some("observe"), Some("UR"), Some("eros"), Some(8.0));

        assert_eq!(engine.state().total_cycles, 2);
        let m = engine.state().action_types.get("observe").unwrap();
        assert_eq!(m.cycles, 2);
        assert!((m.quality_sum - 15.0).abs() < 0.01);
    }

    #[test]
    fn wisdom_detection_high_effort_zero_return() {
        let mut engine = MetricsEngine::new();
        for _ in 0..12 {
            engine.record_cycle(Some("cold_outreach"), None, None, Some(5.0));
        }
        let wisdom = engine.detect_wisdom();
        assert!(wisdom.iter().any(|w| w.category == "high_effort_zero_return"));
    }

    #[test]
    fn summary_output() {
        let mut engine = MetricsEngine::new();
        engine.record_cycle(Some("observe"), Some("UL"), Some("eros"), Some(7.0));
        let summary = engine.get_summary();
        assert_eq!(summary["total_cycles"], 1);
    }
}
