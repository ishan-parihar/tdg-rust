use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rmcp::ErrorData as McpError;
use serde_json::{json, Value};

use crate::db::pool::ConnectionPool;

#[derive(Debug, Clone)]
pub(crate) struct HealthCheckRecord {
    pub service: String,
    pub latency_ms: f64,
    pub success: bool,
    pub error_message: Option<String>,
    metadata: Option<Value>,
    pub timestamp: String,
}

pub(crate) struct HealthMonitor {
    checks: Mutex<Vec<HealthCheckRecord>>,
    breakers: Mutex<HashMap<String, crate::circuit_breaker::CircuitBreaker>>,
    pool: Arc<ConnectionPool>,
}

impl HealthMonitor {
    pub fn new(pool: Arc<ConnectionPool>) -> Self {
        Self {
            checks: Mutex::new(Vec::new()),
            breakers: Mutex::new(HashMap::new()),
            pool,
        }
    }

    /// Maximum in-memory health check records to prevent unbounded growth.
    const MAX_IN_MEMORY_CHECKS: usize = 1000;

    pub fn record_health_check(
        &self,
        service: &str,
        latency_ms: f64,
        success: bool,
        error_message: Option<String>,
        metadata: Option<Value>,
    ) -> Result<(), McpError> {
        {
            let mut checks = self
                .checks
                .lock()
                .map_err(|e| McpError::internal_error(format!("Lock poisoned: {}", e), None))?;
            checks.push(HealthCheckRecord {
                service: service.to_string(),
                latency_ms,
                success,
                error_message: error_message.clone(),
                metadata: metadata.clone(),
                timestamp: crate::db::crud::now_iso(),
            });
            // Prune oldest records if exceeding capacity (ring buffer behavior)
            if checks.len() > Self::MAX_IN_MEMORY_CHECKS {
                let drain_count = checks.len() - Self::MAX_IN_MEMORY_CHECKS;
                checks.drain(..drain_count);
            }
        }

        // Persist to DB — surface errors via tracing instead of silently swallowing
        // (the previous `let _ = …` masked real DB failures, leaving health_checks empty).
        match self.pool.get_connection() {
            Ok(conn) => {
                if let Err(e) = crate::db::crud::record_health_check(
                    &conn,
                    service,
                    latency_ms,
                    success,
                    error_message.as_deref(),
                ) {
                    tracing::warn!("Failed to persist health check for {}: {}", service, e);
                }
                self.pool.release_connection(conn);
            }
            Err(e) => {
                tracing::warn!("Failed to get DB connection for health check ({}): {}", service, e);
            }
        }

        if let Ok(mut breakers) = self.breakers.lock() {
            let cb = breakers
                .entry(service.to_string())
                .or_insert_with(crate::circuit_breaker::CircuitBreaker::new);
            if success {
                cb.record_success();
            } else {
                cb.record_failure();
            }
        }
        Ok(())
    }

    pub fn get_health_summary(&self) -> Result<Value, McpError> {
        if let Ok(conn) = self.pool.get_connection() {
            if let Ok(summary) = crate::db::crud::get_health_summary(&conn) {
                return Ok(summary);
            }
        }
        let checks = self
            .checks
            .lock()
            .map_err(|e| McpError::internal_error(format!("Lock poisoned: {}", e), None))?;
        let total = checks.len();
        if total == 0 {
            return Ok(json!({
                "total_checks": 0,
                "success_rate": 0.0,
                "avg_latency_ms": 0.0,
            }));
        }
        let successes = checks.iter().filter(|c| c.success).count();
        let total_latency: f64 = checks.iter().map(|c| c.latency_ms).sum();
        Ok(json!({
            "total_checks": total,
            "success_rate": successes as f64 / total as f64,
            "avg_latency_ms": total_latency / total as f64,
        }))
    }

    // ponytail: infrastructure for future health query tools
    #[allow(dead_code)]
    pub fn get_recent_health_checks(&self, service: Option<&str>, limit: i64) -> Value {
        if let Ok(conn) = self.pool.get_connection() {
            if let Ok(checks) = crate::db::crud::get_recent_health_checks(&conn, service, limit) {
                return json!({"checks": checks, "total": checks.len()});
            }
        }
        json!({"checks": [], "total": 0})
    }

    pub fn get_circuit_breaker_status(&self) -> Result<Value, McpError> {
        let breakers = self
            .breakers
            .lock()
            .map_err(|e| McpError::internal_error(format!("Lock poisoned: {}", e), None))?;
        let statuses: Vec<Value> = breakers
            .iter()
            .map(|(service, cb)| {
                json!({
                    "service": service,
                    "state": format!("{:?}", cb.state()),
                    "failure_count": cb.failure_count(),
                })
            })
            .collect();
        Ok(json!({"circuit_breakers": statuses}))
    }
}
