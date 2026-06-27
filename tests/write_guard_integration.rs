use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use rusqlite::Connection;
use tempfile::NamedTempFile;

use tdg_rust::db::crud::{add_edge, add_node, get_node};
use tdg_rust::db::write_guard::WriteGuard;
use tdg_rust::db::{init_fts, init_schema, run_migrations};
use tdg_rust::models::{NewEdge, NewNode};

fn setup_file_db() -> NamedTempFile {
    let tmp = NamedTempFile::new().unwrap();
    let conn = Connection::open(tmp.path()).unwrap();
    init_schema(&conn).unwrap();
    init_fts(&conn).unwrap();
    run_migrations(&conn).unwrap();
    drop(conn);
    tmp
}

#[test]
fn write_guard_prevents_concurrent_writes() {
    let tmp = setup_file_db();
    let db_path = tmp.path().to_path_buf();

    let guard1 = WriteGuard::acquire(&db_path, Duration::from_secs(5)).unwrap();
    let result = WriteGuard::acquire(&db_path, Duration::from_millis(50));
    assert!(
        result.is_err(),
        "second acquire should timeout while first holds guard"
    );
    drop(guard1);

    let guard2 = WriteGuard::acquire(&db_path, Duration::from_secs(5)).unwrap();
    drop(guard2);
}

#[test]
fn crud_acquire_guard_on_file_db() {
    let tmp = setup_file_db();
    let conn = Connection::open(tmp.path()).unwrap();

    let node = add_node(
        &conn,
        &NewNode {
            node_type: "observation".to_string(),
            name: "File DB test".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(node.id.starts_with('n'));

    let fetched = get_node(&conn, &node.id).unwrap().unwrap();
    assert_eq!(fetched.name, "File DB test");
}

#[test]
fn crud_skips_guard_on_in_memory_db() {
    let conn = Connection::open_in_memory().unwrap();
    init_schema(&conn).unwrap();
    init_fts(&conn).unwrap();
    run_migrations(&conn).unwrap();

    let node = add_node(
        &conn,
        &NewNode {
            node_type: "observation".to_string(),
            name: "In-memory test".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(node.id.starts_with('n'));
}

#[test]
fn write_guard_released_after_crud_op() {
    let tmp = setup_file_db();
    let db_path = tmp.path().to_path_buf();
    let conn = Connection::open(tmp.path()).unwrap();

    let _node = add_node(
        &conn,
        &NewNode {
            node_type: "action".to_string(),
            name: "Guard release test".to_string(),
            ..Default::default()
        },
    )
    .unwrap();

    let guard = WriteGuard::acquire(&db_path, Duration::from_secs(5)).unwrap();
    drop(guard);
}

#[test]
fn concurrent_writes_serialized() {
    let tmp = setup_file_db();
    let db_path = tmp.path().to_path_buf();

    let conn = Connection::open(tmp.path()).unwrap();
    init_schema(&conn).unwrap();
    init_fts(&conn).unwrap();
    run_migrations(&conn).unwrap();

    let parent = add_node(
        &conn,
        &NewNode {
            node_type: "telos".to_string(),
            name: "Parent".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    drop(conn);

    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();

    for i in 0..2 {
        let db = db_path.clone();
        let barrier = barrier.clone();
        let parent_id = parent.id.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            let conn = Connection::open(&db).unwrap();
            let node = add_node(
                &conn,
                &NewNode {
                    node_type: "action".to_string(),
                    name: format!("Concurrent node {i}"),
                    ..Default::default()
                },
            )
            .unwrap();

            add_edge(
                &conn,
                &NewEdge {
                    source_id: parent_id,
                    target_id: node.id,
                    edge_type: "DECOMPOSES_TO".to_string(),
                    ..Default::default()
                },
            )
            .unwrap();
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let conn = Connection::open(tmp.path()).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 3, "parent + 2 concurrent child nodes");
}
