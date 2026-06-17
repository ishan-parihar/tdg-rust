use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use tdg_rust::db::{init_fts, init_schema, run_migrations, ConnectionPool};
use tdg_rust::models::{NewEdge, NewNode, NodeQuery};
use tdg_rust::ops;
use tdg_rust::knowledge;
use tdg_rust::flow::FlowDriveState;
use tdg_rust::mind::pulse::PulseEngine;
use tdg_rust::mind::diagnostic::DiagnosticEngine;
use tdg_rust::hrr;

/// Create an in-memory pool with schema initialized.
fn make_pool() -> ConnectionPool {
    let pool = ConnectionPool::new(":memory:", 5, 30000).expect("pool creation");
    pool.with_connection(|conn| {
        init_schema(conn).unwrap();
        init_fts(conn).unwrap();
        run_migrations(conn).unwrap();
    })
    .unwrap();
    pool
}

/// Populate the pool with N nodes and edges for benchmarks.
fn populate(pool: &ConnectionPool, n: usize) {
    pool.with_connection(|conn| {
        let mut telos_ids = Vec::new();
        for i in 0..n {
            let node = tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: if i % 5 == 0 { "telos" } else if i % 3 == 0 { "observation" } else { "action" }.to_string(),
                    name: format!("Node {i}"),
                    description: Some(format!("Description for node {i} with searchable terms")),
                    ..Default::default()
                },
            )
            .unwrap();
            if node.node_type == "telos" {
                telos_ids.push(node.id);
            }
        }

        // Connect nodes in a chain
        let nodes: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT id FROM nodes WHERE valid_to IS NULL ORDER BY created_at ASC")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        };

        for i in 1..nodes.len() {
            let source = &nodes[i - 1];
            let target = &nodes[i];
            let edge_type = if i % 5 == 0 { "DECOMPOSES_TO" } else { "EVIDENCES" };
            let _ = tdg_rust::db::crud::add_edge(
                conn,
                &NewEdge {
                    source_id: source.clone(),
                    target_id: target.clone(),
                    edge_type: edge_type.to_string(),
                    ..Default::default()
                },
            );
        }
    })
    .unwrap();
}

// ─── CRUD Benchmarks ─────────────────────────────────────────────────────────

fn bench_add_node(c: &mut Criterion) {
    let mut group = c.benchmark_group("crud_add_node");
    for size in [10, 100] {
        group.bench_with_input(BenchmarkId::new("nodes", size), &size, |b, &size| {
            let pool = make_pool();
            b.iter(|| {
                pool.with_connection(|conn| {
                    for i in 0..size {
                        tdg_rust::db::crud::add_node(
                            conn,
                            &NewNode {
                                node_type: "observation".to_string(),
                                name: format!("Bench node {i}"),
                                description: Some(format!("Searchable benchmark node {i}")),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                    }
                })
                .unwrap();
            });
        });
    }
    group.finish();
}

fn bench_get_node(c: &mut Criterion) {
    let pool = make_pool();
    populate(&pool, 100);

    let node_ids: Vec<String> = pool
        .with_connection(|conn| {
            let mut stmt = conn
                .prepare("SELECT id FROM nodes WHERE valid_to IS NULL LIMIT 10")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        })
        .unwrap();

    c.bench_function("crud_get_node", |b| {
        let mut idx = 0;
        b.iter(|| {
            let id = &node_ids[idx % node_ids.len()];
            pool.with_connection(|conn| {
                tdg_rust::db::crud::get_node(conn, id).unwrap();
            })
            .unwrap();
            idx += 1;
        });
    });
}

fn bench_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("crud_search");
    let pool = make_pool();
    populate(&pool, 100);

    for query in ["node", "benchmark", "searchable"] {
        group.bench_with_input(BenchmarkId::from_parameter(query), query, |b, query| {
            b.iter(|| {
                pool.with_connection(|conn| {
                    tdg_rust::db::crud::search(conn, query, 10).unwrap();
                })
                .unwrap();
            });
        });
    }
    group.finish();
}

// ─── Pathfind Benchmark ──────────────────────────────────────────────────────

fn bench_pathfind(c: &mut Criterion) {
    let pool = make_pool();
    populate(&pool, 50);

    let (first, last): (String, String) = pool
        .with_connection(|conn| {
            let ids: Vec<String> = conn
                .prepare("SELECT id FROM nodes WHERE valid_to IS NULL ORDER BY created_at ASC LIMIT 2")
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            (ids[0].clone(), ids[ids.len() - 1].clone())
        })
        .unwrap();

    c.bench_function("crud_pathfind", |b| {
        b.iter(|| {
            pool.with_connection(|conn| {
                tdg_rust::db::crud::pathfind(conn, &first, &last, 5, 100).unwrap();
            })
            .unwrap();
        });
    });
}

// ─── Flow Benchmarks ─────────────────────────────────────────────────────────

fn bench_renormalize_graph(c: &mut Criterion) {
    let mut group = c.benchmark_group("flow_renormalize");
    for size in [20, 100] {
        group.bench_with_input(BenchmarkId::new("nodes", size), &size, |b, &size| {
            let pool = make_pool();
            populate(&pool, size);
            b.iter(|| {
                pool.with_connection(|conn| {
                    tdg_rust::flow::renormalize_graph(conn, false).unwrap();
                })
                .unwrap();
            });
        });
    }
    group.finish();
}

fn bench_aggregate_upward(c: &mut Criterion) {
    let pool = make_pool();
    populate(&pool, 100);

    let action_ids: Vec<String> = pool
        .with_connection(|conn| {
            conn.prepare("SELECT id FROM nodes WHERE valid_to IS NULL AND node_type = 'action' LIMIT 10")
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        })
        .unwrap();

    c.bench_function("flow_aggregate_upward", |b| {
        let mut idx = 0;
        b.iter(|| {
            let id = &action_ids[idx % action_ids.len()];
            pool.with_connection(|conn| {
                tdg_rust::flow::aggregate_upward(conn, id).unwrap();
            })
            .unwrap();
            idx += 1;
        });
    });
}

// ─── Knowledge Benchmarks ────────────────────────────────────────────────────

fn bench_detect_orphans(c: &mut Criterion) {
    let mut group = c.benchmark_group("knowledge_detect_orphans");
    for size in [50, 200] {
        group.bench_with_input(BenchmarkId::new("nodes", size), &size, |b, &size| {
            let pool = make_pool();
            populate(&pool, size);
            b.iter(|| {
                pool.with_connection(|conn| {
                    knowledge::detect_orphans(conn).unwrap();
                })
                .unwrap();
            });
        });
    }
    group.finish();
}

fn bench_generate_hygiene_report(c: &mut Criterion) {
    let pool = make_pool();
    populate(&pool, 200);

    c.bench_function("knowledge_hygiene_report", |b| {
        b.iter(|| {
            pool.with_connection(|conn| {
                knowledge::generate_hygiene_report(conn).unwrap();
            })
            .unwrap();
        });
    });
}

fn bench_classify_catalyst(c: &mut Criterion) {
    let pool = make_pool();
    let obs_id = pool
        .with_connection(|conn| {
            let node = tdg_rust::db::crud::add_node(
                conn,
                &NewNode {
                    node_type: "observation".to_string(),
                    name: "Signal: Performance drop".to_string(),
                    description: Some("Alert detected in system".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
            node.id
        })
        .unwrap();

    c.bench_function("knowledge_classify_catalyst", |b| {
        b.iter(|| {
            pool.with_connection(|conn| {
                knowledge::classify_catalyst(conn, &obs_id).unwrap();
            })
            .unwrap();
        });
    });
}

// ─── Mind Benchmarks ─────────────────────────────────────────────────────────

fn bench_pulse(c: &mut Criterion) {
    let pool = make_pool();
    populate(&pool, 100);

    c.bench_function("mind_pulse", |b| {
        b.iter(|| {
            pool.with_connection(|conn| {
                let engine = PulseEngine::new();
                let pulses = engine.pulse(conn, &[]).unwrap();
                let _ = engine.summarize(&pulses);
            })
            .unwrap();
        });
    });
}

fn bench_diagnostic(c: &mut Criterion) {
    let pool = make_pool();
    populate(&pool, 100);

    c.bench_function("mind_diagnostic", |b| {
        b.iter(|| {
            pool.with_connection(|conn| {
                let engine = DiagnosticEngine::new();
                let _ = engine.analyze(conn, &[], &[]).unwrap();
            })
            .unwrap();
        });
    });
}

// ─── HRR Benchmarks ──────────────────────────────────────────────────────────

fn bench_hrr_bind(c: &mut Criterion) {
    let a = hrr::random_key(hrr::HRR_DIM);
    let b = hrr::random_key(hrr::HRR_DIM);

    c.bench_function("hrr_bind", |b| {
        b.iter(|| {
            hrr::bind(&a, &b);
        });
    });
}

fn bench_hrr_bundle(c: &mut Criterion) {
    let mut group = c.benchmark_group("hrr_bundle");
    for count in [4, 16, 64] {
        group.bench_with_input(BenchmarkId::new("vectors", count), &count, |b, &count| {
            let vectors: Vec<_> = (0..count).map(|_| hrr::random_key(hrr::HRR_DIM)).collect();
            b.iter(|| {
                hrr::bundle(&vectors);
            });
        });
    }
    group.finish();
}

fn bench_hrr_probe(c: &mut Criterion) {
    let mut bank = hrr::HrrMemoryBank::new();
    for i in 0..100 {
        bank.store(
            format!("item_{i}"),
            hrr::random_key(hrr::HRR_DIM),
        );
    }
    let query = hrr::random_key(hrr::HRR_DIM);

    c.bench_function("hrr_probe_100_items", |b| {
        b.iter(|| {
            bank.probe(&query);
        });
    });
}

// ─── Ops Benchmarks ──────────────────────────────────────────────────────────

fn bench_reconcile(c: &mut Criterion) {
    let pool = make_pool();
    populate(&pool, 100);

    c.bench_function("ops_reconcile", |b| {
        b.iter(|| {
            pool.with_connection(|conn| {
                ops::reconcile(conn).unwrap();
            })
            .unwrap();
        });
    });
}

fn bench_micro_slice(c: &mut Criterion) {
    let pool = make_pool();
    populate(&pool, 100);

    c.bench_function("ops_micro_slice", |b| {
        b.iter(|| {
            pool.with_connection(|conn| {
                ops::micro_slice(conn).unwrap();
            })
            .unwrap();
        });
    });
}

// ─── Criterion Group ─────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_add_node,
    bench_get_node,
    bench_search,
    bench_pathfind,
    bench_renormalize_graph,
    bench_aggregate_upward,
    bench_detect_orphans,
    bench_generate_hygiene_report,
    bench_classify_catalyst,
    bench_pulse,
    bench_diagnostic,
    bench_hrr_bind,
    bench_hrr_bundle,
    bench_hrr_probe,
    bench_reconcile,
    bench_micro_slice,
);

criterion_main!(benches);
