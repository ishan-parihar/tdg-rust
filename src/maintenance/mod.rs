//! # Maintenance Module
//!
//! Autonomous graph maintenance: health monitoring, structural repair,
//! metadata enrichment, and entropy compression.
//!
//! This is a feature-gated module — it compiles but is not required for core functionality.

pub mod archiver;
pub mod enricher;
pub mod janitor;
pub mod monitor;
pub mod orchestrator;

pub use archiver::{Archiver, ArchiverReport};
pub use enricher::{Enricher, EnricherReport};
pub use janitor::{Janitor, JanitorReport};
pub use monitor::{HealthMonitor, HealthReport};
pub use orchestrator::{SelfManager, SelfManagerReport};

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        crate::db::init_fts(&conn).unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    fn insert_test_nodes(conn: &Connection, count: usize) {
        for i in 0..count {
            conn.execute(
                "INSERT INTO nodes (id, node_type, name, lifecycle_state, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'active', datetime('now'), datetime('now'))",
                rusqlite::params![
                    format!("n{:04}", i),
                    if i % 2 == 0 { "observation" } else { "action" },
                    format!("Test Node {i}"),
                ],
            )
            .unwrap();
        }
    }

    #[test]
    fn health_monitor_runs() {
        let conn = setup_db();
        insert_test_nodes(&conn, 5);
        let monitor = HealthMonitor::new(&conn);
        let report = monitor.check().unwrap();
        assert!(report.health_score >= 0.0 && report.health_score <= 1.0);
    }

    #[test]
    fn janitor_runs_dry() {
        let conn = setup_db();
        insert_test_nodes(&conn, 3);
        let janitor = Janitor::new(&conn);
        let report = janitor.run(true).unwrap();
        assert!(report.dry_run);
    }

    #[test]
    fn enricher_runs_dry() {
        let conn = setup_db();
        insert_test_nodes(&conn, 3);
        let enricher = Enricher::new(&conn);
        let report = enricher.run(true).unwrap();
        assert!(report.dry_run);
    }

    #[test]
    fn archiver_runs_dry() {
        let conn = setup_db();
        insert_test_nodes(&conn, 3);
        let archiver = Archiver::new(&conn);
        let report = archiver.run(true).unwrap();
        assert!(report.dry_run);
    }

    #[test]
    fn self_manager_runs_dry() {
        let conn = setup_db();
        insert_test_nodes(&conn, 5);
        let manager = SelfManager::new(&conn);
        let report = manager.run(true).unwrap();
        assert!(report.dry_run);
        assert!(report.duration_seconds >= 0.0);
    }

    #[test]
    fn health_report_actions_trigger_no_fts() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn.execute(
            "INSERT INTO nodes (id, node_type, name, lifecycle_state, created_at, updated_at)
             VALUES ('n_no_fts', 'observation', 'No FTS Node', 'active', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        let monitor = HealthMonitor::new(&conn);
        let report = monitor.check().unwrap();
        assert!(
            report.fts5_coverage < 1.0,
            "Expected FTS5 coverage < 1.0 without FTS init, got {}",
            report.fts5_coverage
        );
    }
}
