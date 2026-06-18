use tdg_rust::db::{init_fts, init_schema, run_migrations, ConnectionPool};

fn make_pool() -> ConnectionPool {
    let pool = ConnectionPool::new(":memory:", 5, 30000).expect("pool creation");
    pool.with_connection(|conn| {
        init_schema(conn)?;
        init_fts(conn)?;
        run_migrations(conn)?;
        Ok(())
    })
    .expect("schema init");
    pool
}

#[test]
fn trust_set_and_get() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::set_trust(conn, "agent-1", 0.8, Some("initial setup"))?;
        let score = tdg_rust::db::crud::get_trust(conn, "agent-1")?;
        assert!((score - 0.8).abs() < f64::EPSILON);
        Ok(())
    })
    .unwrap();
}

#[test]
fn trust_default_score() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let score = tdg_rust::db::crud::get_trust(conn, "unknown-agent")?;
        assert!((score - 0.5).abs() < f64::EPSILON);
        Ok(())
    })
    .unwrap();
}

#[test]
fn trust_adjust() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::set_trust(conn, "agent-2", 0.5, None)?;
        let new_score = tdg_rust::db::crud::adjust_trust(conn, "agent-2", 0.2, Some("good work"))?;
        assert!((new_score - 0.7).abs() < f64::EPSILON);
        let final_score = tdg_rust::db::crud::get_trust(conn, "agent-2")?;
        assert!((final_score - 0.7).abs() < f64::EPSILON);
        Ok(())
    })
    .unwrap();
}

#[test]
fn trust_clamped_to_range() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::set_trust(conn, "agent-3", 0.9, None)?;
        let new_score = tdg_rust::db::crud::adjust_trust(conn, "agent-3", 0.5, None)?;
        assert!((new_score - 1.0).abs() < f64::EPSILON);

        tdg_rust::db::crud::set_trust(conn, "agent-4", 0.1, None)?;
        let new_score = tdg_rust::db::crud::adjust_trust(conn, "agent-4", -0.5, None)?;
        assert!((new_score - 0.0).abs() < f64::EPSILON);
        Ok(())
    })
    .unwrap();
}

#[test]
fn trust_persists_across_connections() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::set_trust(conn, "agent-5", 0.75, Some("persistent test"))?;
        Ok(())
    })
    .unwrap();

    pool.with_connection(|conn| {
        let score = tdg_rust::db::crud::get_trust(conn, "agent-5")?;
        assert!((score - 0.75).abs() < f64::EPSILON);
        Ok(())
    })
    .unwrap();
}

#[test]
fn trust_overwrite() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        tdg_rust::db::crud::set_trust(conn, "agent-6", 0.3, None)?;
        tdg_rust::db::crud::set_trust(conn, "agent-6", 0.9, Some("updated"))?;
        let score = tdg_rust::db::crud::get_trust(conn, "agent-6")?;
        assert!((score - 0.9).abs() < f64::EPSILON);
        Ok(())
    })
    .unwrap();
}

#[test]
fn trust_table_exists_in_schema() {
    let pool = make_pool();
    pool.with_connection(|conn| {
        let exists: bool = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='trust_scores'",
                [],
                |row| row.get::<_, String>(0),
            )
            .map(|_| true)
            .unwrap_or(false);
        assert!(exists, "trust_scores table should exist after schema init");
        Ok(())
    })
    .unwrap();
}
