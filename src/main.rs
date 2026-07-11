// ponytail: dead_code warnings enabled — let the compiler surface real dead code

use clap::{Parser, Subcommand};

use tdg_rust::config::Config;
use tdg_rust::db::{init_fts, init_schema, run_migrations, ConnectionPool};
use tdg_rust::mcp::server;
use tdg_rust::scripts;

#[derive(Parser)]
#[command(
    name = "tdg-rust",
    about = "Teleological Developmental Graph - Rust Implementation",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the MCP server
    Serve {
        /// Port to listen on (default=3000 for stdio, other ports for HTTP)
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
    /// Run database migrations
    Migrate,
    /// Backup the database
    Backup {
        /// Backup file path
        #[arg(short, long)]
        output: String,
    },
    /// Show database statistics
    Stats,
    /// Initialize the database schema
    Init,
    // ── Phase 12: Scripts & Utilities ─────────────────────────────────
    /// Audit graph integration (orphan detection, health scores, archival)
    Audit,
    /// Check constraint vitality and ghost nodes
    Check,
    /// Unify persistence across data sources
    Unify,
    /// Reconcile constraints (dedup, link repair)
    ReconcileConstraints,
    /// Sync skills directory to graph
    SyncSkills {
        /// Skills directory path
        #[arg(short, long)]
        dir: Option<String>,
    },
    /// Auto-capture observation from description
    AutoCapture {
        /// Observation description
        #[arg(short, long)]
        description: String,
        /// Quadrant (LR, UL, LL, UR)
        #[arg(short, long, default_value = "LR")]
        quadrant: String,
        /// Trust level (0.0-1.0)
        #[arg(short = 't', long, default_value = "0.5")]
        trust: f64,
        /// Comma-separated entities
        #[arg(short, long)]
        entities: Option<String>,
    },
    /// Create a node from CLI
    Create {
        /// Node type
        #[arg(short, long)]
        node_type: String,
        /// Node name
        #[arg(short, long)]
        name: String,
        /// Description
        #[arg(short, long)]
        description: Option<String>,
    },
    /// Maintenance check (orphan + stale node detection)
    MaintenanceCheck,
    /// Repair orphan nodes (link or archive)
    RepairOrphans,
    /// Embed nodes using ONNX model
    Embed {
        /// Rebuild all embeddings from scratch
        #[arg(long)]
        rebuild: bool,
    },
}

fn main() -> anyhow::Result<()> {
    // Load .env if present
    let _ = dotenvy::dotenv();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let config = Config::from_env();

    // Ensure directories exist
    config.ensure_dirs()?;

    match cli.command {
        Commands::Init => {
            tracing::info!("Initializing database at {}", config.db_path.display());
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            pool.with_connection(|conn| {
                init_schema(conn)?;
                init_fts(conn)?;
                run_migrations(conn)?;
                tracing::info!("Schema initialized successfully");
                Ok(())
            })?;
            pool.close();
        }
        Commands::Migrate => {
            tracing::info!("Running migrations on {}", config.db_path.display());
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            pool.with_connection(|conn| {
                init_schema(conn)?;
                init_fts(conn)?;
                run_migrations(conn)?;
                tracing::info!("Migrations completed successfully");
                Ok(())
            })?;
            pool.close();
        }
        Commands::Backup { output } => {
            tracing::info!("Backing up {} to {}", config.db_path.display(), output);
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            pool.backup(std::path::Path::new(&output))?;
            tracing::info!("Backup completed");
            pool.close();
        }
        Commands::Stats => {
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            pool.with_connection(|conn| {
                let node_count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL", [], |r| r.get(0))
                    .unwrap_or(0);
                let edge_count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM edges WHERE valid_to IS NULL", [], |r| r.get(0))
                    .unwrap_or(0);
                let event_count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
                    .unwrap_or(0);

                println!("TDG Database Statistics");
                println!("=======================");
                println!("Nodes:   {node_count}");
                println!("Edges:   {edge_count}");
                println!("Events:  {event_count}");

                // Count by type
                let mut stmt = conn
                    .prepare("SELECT node_type, COUNT(*) FROM nodes WHERE valid_to IS NULL GROUP BY node_type ORDER BY COUNT(*) DESC")?;
                let rows = stmt.query_map([], |row| {
                    let t: String = row.get(0)?;
                    let c: i64 = row.get(1)?;
                    Ok((t, c))
                })?;
                println!("\nNodes by type:");
                for (t, c) in rows.flatten() {
                    println!("  {t}: {c}");
                }
                Ok(())
            })?;
            pool.close();
        }
        // ── Phase 12: Scripts & Utilities ─────────────────────────────
        Commands::Audit => {
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            let result = pool.with_connection(scripts::audit)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            pool.close();
        }
        Commands::Check => {
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            let result = pool.with_connection(scripts::check)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            pool.close();
        }
        Commands::Unify => {
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            let result = pool.with_connection(scripts::unify)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            pool.close();
        }
        Commands::ReconcileConstraints => {
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            let result = pool.with_connection(scripts::reconcile_constraints)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            pool.close();
        }
        Commands::SyncSkills { dir } => {
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            let skills_dir =
                dir.unwrap_or_else(|| config.skills_dir.to_str().unwrap_or("./skills").to_string());
            let result = pool.with_connection(|conn| scripts::sync_skills(conn, &skills_dir))?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            pool.close();
        }
        Commands::AutoCapture {
            description,
            quadrant,
            trust,
            entities,
        } => {
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            let result = pool.with_connection(|conn| {
                scripts::auto_capture(conn, &description, &quadrant, trust, entities.as_deref())
            })?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            pool.close();
        }
        Commands::Create {
            node_type,
            name,
            description,
        } => {
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            let result = pool.with_connection(|conn| {
                scripts::create_node(conn, &node_type, &name, description.as_deref())
            })?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            pool.close();
        }
        Commands::MaintenanceCheck => {
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            let result = pool.with_connection(scripts::maintenance_check)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            pool.close();
        }
        Commands::RepairOrphans => {
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            let result = pool.with_connection(scripts::repair_orphans)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            pool.close();
        }
        #[cfg(feature = "onnx")]
        Commands::Embed { rebuild } => {
            use tdg_rust::mind::embedding::{self, EmbeddingConfig};

            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;

            // Apply schema migrations (adds dimension column if missing)
            pool.with_connection(|conn| {
                tdg_rust::db::run_migrations(conn)?;
                Ok(())
            })?;

            // Download model files if needed
            embedding::ensure_model_files(&config)?;

            // Initialize embedding engine
            let emb_config = EmbeddingConfig::from_app_config(&config);
            embedding::init(emb_config)?;

            let target_dim = config.embedding.effective_dimension();
            println!("Embedding nodes with {}-dim vectors...", target_dim);

            pool.with_connection(|conn| {
                // Get all nodes without embeddings or with --rebuild
                let nodes: Vec<(String, String, String)> = if rebuild {
                    let mut stmt = conn.prepare(
                        "SELECT id, name, COALESCE(description, '') FROM nodes WHERE valid_to IS NULL"
                    )?;
                    let rows = stmt.query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    })?;
                    rows.filter_map(|r| r.ok()).collect()
                } else {
                    let mut stmt = conn.prepare(
                        "SELECT n.id, n.name, COALESCE(n.description, '') FROM nodes n
                         LEFT JOIN embeddings e ON n.id = e.node_id
                         WHERE n.valid_to IS NULL AND e.node_id IS NULL"
                    )?;
                    let rows = stmt.query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    })?;
                    rows.filter_map(|r| r.ok()).collect()
                };

                let total = nodes.len();
                println!("Found {} nodes to embed", total);

                // Process in batches of 100
                let batch_size = 100;
                for (i, chunk) in nodes.chunks(batch_size).enumerate() {
                    let start = i * batch_size;
                    let end = std::cmp::min(start + batch_size, total);
                    print!("  Batch {}/{}...", start + 1, end);

                    for (node_id, name, description) in chunk {
                        let text = embedding::build_embedding_text(
                            conn,
                            node_id,
                            name,
                            description,
                            3,
                        );
                        let result = embedding::embed(&text)?;
                        tdg_rust::db::crud::upsert_embedding(
                            conn,
                            node_id,
                            &result.vector,
                            config.embedding.model_dir_name(),
                            target_dim as i64,
                        )?;
                    }

                    println!(" done");
                }

                println!("Embedding complete: {} nodes processed", total);
                Ok(())
            })?;

            pool.close();
        }
        #[cfg(not(feature = "onnx"))]
        Commands::Embed { .. } => {
            eprintln!("Error: Embed command requires the 'onnx' feature. Rebuild with:");
            eprintln!("  cargo build --features onnx");
            std::process::exit(1);
        }
        Commands::Serve { port } => {
            // Pre-flight check: verify libonnxruntime.so.1 is loadable.
            // The dynamic linker kills the process with exit 127 before any
            // Rust code runs if the library is missing. This check runs
            // early enough to print a helpful error message.
            #[cfg(feature = "onnx")]
            {
                use std::path::PathBuf;
                let lib_dir = std::env::var("LD_LIBRARY_PATH")
                    .unwrap_or_default()
                    .split(':')
                    .next()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| config.tdg_dir.join("lib"));
                let lib_path = lib_dir.join("libonnxruntime.so.1");
                if !lib_path.exists() {
                    eprintln!(
                        "ERROR: libonnxruntime.so.1 not found at {}",
                        lib_path.display()
                    );
                    eprintln!("       Required for ONNX embedding support.");
                    eprintln!("       Fix: run install.sh or set LD_LIBRARY_PATH to include the directory containing libonnxruntime.so.1");
                    std::process::exit(1);
                }
            }

            tracing::info!("Starting MCP server on port {port}");
            let pool = ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                5,
                30000,
            )?;
            pool.with_connection(|conn| {
                init_schema(conn)?;
                init_fts(conn)?;
                run_migrations(conn)?;
                Ok(())
            })?;

            // Spawn a background maintenance scheduler.
            //
            // Previously, NO automatic maintenance ever ran — the agent had to
            // manually call tdg_maintenance / tdg_enrich / tdg_self_manage.
            // This meant stale nodes accumulated forever, embeddings were never
            // backfilled, FTS5 index drifted, health_checks stayed empty, and
            // trust scores never decayed. The scheduler now runs:
            //   - Every 6 hours: full SelfManager cycle (janitor + enricher + archiver + monitor)
            //   - Every 5 minutes: internal health check (records to health_checks table)
            let rt = tokio::runtime::Runtime::new()?;
            // ConnectionPool is not Clone, so we create a second pool pointing
            // at the same DB file for the background scheduler. Both pools
            // share the same SQLite database (WAL mode allows concurrent readers
            // and a single writer across processes/connections).
            let maintenance_pool = std::sync::Arc::new(ConnectionPool::new(
                config
                    .db_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                4, // 2 for maintenance + 2 for metabolism workers
                30000,
            )?);
            // Configurable intervals via env vars (with sensible defaults).
            // TDG_MAINTENANCE_INTERVAL_SECS: how often to run full SelfManager
            //   (janitor + enricher + archiver + telearchy). Default: 21600 (6h)
            // TDG_HEALTH_CHECK_INTERVAL_SECS: how often to record internal
            //   health checks. Default: 300 (5m)
            let self_manager_interval_secs = std::env::var("TDG_MAINTENANCE_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(6 * 60 * 60);
            let health_check_interval_secs = std::env::var("TDG_HEALTH_CHECK_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(5 * 60);
            // Phase 4: Greater-cycle sweep interval (default 10 min).
            let greater_cycle_interval_secs = std::env::var("TDG_GREATER_CYCLE_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(10 * 60);

            // Phase 12: Graph-level mind integration interval (default 15 min).
            let mind_integration_interval_secs =
                std::env::var("TDG_MIND_INTEGRATION_INTERVAL_SECS")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(15 * 60);

            // Phase 15: Resonance graph full rebuild interval (default 4 hours).
            let resonance_rebuild_interval_secs =
                std::env::var("TDG_RESONANCE_REBUILD_INTERVAL_SECS")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(4 * 60 * 60);

            // Phase 16: Synaptic decay (LTD) interval (default 1 hour).
            let synaptic_decay_interval_secs = std::env::var("TDG_SYNAPTIC_DECAY_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(60 * 60);

            // Phase 17: Synaptogenesis interval (default 30 min).
            // Grows new RESONATES_WITH edges for high-resonance holon pairs.
            let synaptogenesis_interval_secs = std::env::var("TDG_SYNAPTOGENESIS_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(30 * 60);

            // Phase 18: Memory replay interval (default 6 hours).
            // Re-activates recent memories to strengthen their edges (sleep replay).
            let memory_replay_interval_secs = std::env::var("TDG_MEMORY_REPLAY_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(6 * 60 * 60);

            // Clone the pool Arc for the metabolism worker (before moving into maintenance spawn)
            let metabolism_pool = maintenance_pool.clone();

            let maintenance_handle = rt.spawn(async move {
                let self_manager_interval = std::time::Duration::from_secs(self_manager_interval_secs);
                let health_check_interval = std::time::Duration::from_secs(health_check_interval_secs);
                let greater_cycle_interval = std::time::Duration::from_secs(greater_cycle_interval_secs);
                let mind_integration_interval = std::time::Duration::from_secs(mind_integration_interval_secs);
                let mut self_manager_ticker =
                    tokio::time::interval(self_manager_interval);
                let mut health_check_ticker =
                    tokio::time::interval(health_check_interval);
                let mut greater_cycle_ticker =
                    tokio::time::interval(greater_cycle_interval);
                let mut mind_integration_ticker =
                    tokio::time::interval(mind_integration_interval);
                let mut resonance_rebuild_ticker =
                    tokio::time::interval(std::time::Duration::from_secs(resonance_rebuild_interval_secs));
                let mut synaptic_decay_ticker =
                    tokio::time::interval(std::time::Duration::from_secs(synaptic_decay_interval_secs));
                let mut synaptogenesis_ticker =
                    tokio::time::interval(std::time::Duration::from_secs(synaptogenesis_interval_secs));
                let mut memory_replay_ticker =
                    tokio::time::interval(std::time::Duration::from_secs(memory_replay_interval_secs));
                // Skip the first immediate tick (we don't want to run maintenance
                // during startup — let the server stabilize first).
                self_manager_ticker.tick().await;
                health_check_ticker.tick().await;
                greater_cycle_ticker.tick().await;
                mind_integration_ticker.tick().await;
                synaptic_decay_ticker.tick().await;
                synaptogenesis_ticker.tick().await;
                memory_replay_ticker.tick().await;
                resonance_rebuild_ticker.tick().await;
                loop {
                    tokio::select! {
                        _ = mind_integration_ticker.tick() => {
                            // Phase 12: Graph-level mind integration — the closed loop.
                            // Diagnoses graph-level patterns and injects catalyst
                            // to force integration. This is what turns TDG from a
                            // dashboard into a mind.
                            let pool = maintenance_pool.clone();
                            tokio::task::spawn_blocking(move || {
                                if let Ok(conn) = pool.get_connection() {
                                    match tdg_rust::mind::graph_mind::run_integration(&conn) {
                                        Ok(report) => {
                                            if !report.diagnoses.is_empty() {
                                                tracing::info!(
                                                    "Graph mind integration: {} diagnoses, {} injections (mean_g_z={:.1}, mean_p_z={:.1})",
                                                    report.diagnoses.len(),
                                                    report.injections_enqueued,
                                                    report.mean_g_z,
                                                    report.mean_p_z
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!("Graph mind integration failed: {}", e);
                                        }
                                    }
                                    pool.release_connection(conn);
                                }
                            }).await.ok();
                        }
                        _ = greater_cycle_ticker.tick() => {
                            // Phase 4: Greater-cycle sweep.
                            // Enqueue GreaterTick jobs for holons that have
                            // accumulated transformation pressure in their lesser
                            // cycle state. This is the Tier 3 scheduled integration
                            // that fires the discontinuous/ratcheting greater cycle.
                            let pool = maintenance_pool.clone();
                            tokio::task::spawn_blocking(move || {
                                if let Ok(conn) = pool.get_connection() {
                                    // Find holons with transformation_pressure > 0
                                    // (they have lesser cycle state that needs to feed upward)
                                    if let Ok(mut stmt) = conn.prepare(
                                        "SELECT id FROM nodes
                                         WHERE valid_to IS NULL
                                           AND lesser_cycle_json IS NOT NULL
                                           AND lesser_cycle_json LIKE '%transformation_pressure%'
                                         ORDER BY updated_at DESC
                                         LIMIT 200",
                                    ) {
                                        let holon_ids: Vec<String> = stmt
                                            .query_map([], |row| row.get(0))
                                            .ok()
                                            .map(|iter| iter.filter_map(|r| r.ok()).collect())
                                            .unwrap_or_default();

                                        let mut enqueued = 0;
                                        for holon_id in &holon_ids {
                                            let _ = tdg_rust::metabolism::worker::enqueue_job(
                                                &conn,
                                                holon_id,
                                                tdg_rust::metabolism::worker::JobType::GreaterTick,
                                                serde_json::json!({"trigger": "tier3_sweep"}),
                                                tdg_rust::metabolism::worker::PRIORITY_LOW,
                                            );
                                            enqueued += 1;
                                        }

                                        if enqueued > 0 {
                                            tracing::info!(
                                                "Greater-cycle sweep: enqueued {} GreaterTick jobs",
                                                enqueued
                                            );
                                        }
                                    }
                                    pool.release_connection(conn);
                                }
                            }).await.ok();
                        }
                        _ = self_manager_ticker.tick() => {
                            tracing::info!("Scheduled maintenance: running SelfManager cycle");
                            let pool = maintenance_pool.clone();
                            tokio::task::spawn_blocking(move || {
                                if let Ok(conn) = pool.get_connection() {
                                    let manager = tdg_rust::maintenance::SelfManager::new(&conn);
                                    match manager.run(false) {
                                        Ok(report) => {
                                            tracing::info!(
                                                "Scheduled maintenance completed: health_delta={:.3}, failed={}",
                                                report.health_delta,
                                                report.failed.len()
                                            );
                                        }
                                        Err(e) => {
                                            tracing::warn!("Scheduled maintenance failed: {}", e);
                                        }
                                    }
                                    pool.release_connection(conn);
                                }
                            }).await.ok();
                        }
                        _ = health_check_ticker.tick() => {
                            // Record a lightweight internal health check so the
                            // health_checks table doesn't stay empty.
                            let pool = maintenance_pool.clone();
                            tokio::task::spawn_blocking(move || {
                                if let Ok(conn) = pool.get_connection() {
                                    let start = std::time::Instant::now();
                                    let success = conn.execute_batch("SELECT 1;").is_ok();
                                    let latency = start.elapsed().as_millis() as f64;
                                    let _ = tdg_rust::db::crud::record_health_check(
                                        &conn,
                                        "tdg_internal",
                                        latency,
                                        success,
                                        if success { None } else { Some("SELECT 1 health probe failed".to_string()) }.as_deref(),
                                    );
                                    pool.release_connection(conn);
                                }
                            }).await.ok();
                        }
                        _ = resonance_rebuild_ticker.tick() => {
                            // Phase 15: Resonance graph full rebuild.
                            // Corrects incremental drift by recomputing all
                            // resonance entries from scratch.
                            let pool = maintenance_pool.clone();
                            tokio::task::spawn_blocking(move || {
                                if let Ok(conn) = pool.get_connection() {
                                    // Clear and rebuild resonance_graph for all stable holons
                                    let _ = conn.execute("DELETE FROM resonance_graph", []);

                                    // Load all holons with attractor fields
                                    let holons: Vec<(String, String)> = {
                                        let stmt_result = conn.prepare(
                                            "SELECT id, attractor_field_json FROM nodes
                                             WHERE valid_to IS NULL
                                               AND attractor_field_json IS NOT NULL
                                               AND attractor_field_json != ''",
                                        );

                                        match stmt_result {
                                            Ok(mut stmt) => {
                                                let collected: Vec<(String, String)> = stmt
                                                    .query_map([], |row| {
                                                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                                                    })
                                                    .ok()
                                                    .map(|iter| iter.filter_map(|r| r.ok()).collect())
                                                    .unwrap_or_default();
                                                collected
                                            }
                                            Err(_) => {
                                                // Can't release conn here (still borrowed by stmt_result).
                                                // Just return — conn will be dropped (acceptable for background task).
                                                return;
                                            }
                                        }
                                    };

                                    // Now conn is no longer borrowed — we can use it for inserts
                                    let mut rebuilt = 0;
                                    for (holon_id, af_json) in &holons {
                                        if let Some(af1) = tdg_rust::metabolism::attractor::AttractorField::from_json(af_json) {
                                            if !af1.is_stable() { continue; }
                                            for (partner_id, partner_json) in &holons {
                                                if holon_id == partner_id { continue; }
                                                if let Some(af2) = tdg_rust::metabolism::attractor::AttractorField::from_json(partner_json) {
                                                    if !af2.is_stable() { continue; }
                                                    let rc = tdg_rust::metabolism::health::resonance_with_components(&af1, &af2);
                                                    if rc.resonance > 0.0 {
                                                        let now = chrono::Utc::now().to_rfc3339();
                                                        let _ = conn.execute(
                                                            "INSERT OR REPLACE INTO resonance_graph
                                                                (holon_id, partner_id, resonance_score,
                                                                 complementarity, gamma_compat, great_way_intersect, computed_at)
                                                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                                                            rusqlite::params![holon_id, partner_id, rc.resonance, rc.complementarity, rc.gamma_compat, rc.great_way_intersect, now],
                                                        );
                                                    }
                                                }
                                            }
                                            rebuilt += 1;
                                        }
                                    }

                                    tracing::info!(
                                        "Resonance graph rebuilt: {} holons processed",
                                        rebuilt
                                    );
                                    pool.release_connection(conn);
                                }
                            }).await.ok();
                        }
                        _ = synaptic_decay_ticker.tick() => {
                            // Phase 16: Synaptic decay (LTD) — "use it or lose it".
                            // Decays co_activation_count for edges that haven't fired recently.
                            // Soft-deletes edges with zero co-activation AND low weight.
                            let pool = maintenance_pool.clone();
                            tokio::task::spawn_blocking(move || {
                                if let Ok(conn) = pool.get_connection() {
                                    // LTD: decay co_activation_count by 50% for edges
                                    // not co-activated in the last 7 days
                                    let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
                                    let cutoff_str = cutoff.to_rfc3339();
                                    let decayed = conn.execute(
                                        "UPDATE edges SET co_activation_count = co_activation_count / 2
                                         WHERE valid_to IS NULL
                                           AND co_activation_count > 0
                                           AND (last_co_activation IS NULL OR last_co_activation < ?1)",
                                        rusqlite::params![cutoff_str],
                                    ).unwrap_or(0);

                                    // Pruning: soft-delete edges with zero co-activation
                                    // AND weight < 0.3 (truly unused weak edges)
                                    let now = chrono::Utc::now().to_rfc3339();
                                    let pruned = conn.execute(
                                        "UPDATE edges SET valid_to = ?1
                                         WHERE valid_to IS NULL
                                           AND co_activation_count = 0
                                           AND weight < 0.3
                                           AND created_at < ?2",
                                        rusqlite::params![now, cutoff_str],
                                    ).unwrap_or(0);

                                    if decayed > 0 || pruned > 0 {
                                        tracing::info!(
                                            "Synaptic decay (LTD): {} edges decayed, {} pruned",
                                            decayed, pruned
                                        );
                                    }
                                    pool.release_connection(conn);
                                }
                            }).await.ok();
                        }
                        _ = synaptogenesis_ticker.tick() => {
                            // Phase 17: Synaptogenesis — grow new edges from resonance.
                            let pool = maintenance_pool.clone();
                            tokio::task::spawn_blocking(move || {
                                if let Ok(conn) = pool.get_connection() {
                                    let pairs: Vec<(String, String, f64)> = {
                                        let stmt_result = conn.prepare(
                                            "SELECT rg.holon_id, rg.partner_id, rg.resonance_score
                                             FROM resonance_graph rg
                                             WHERE rg.resonance_score > 0.7
                                             AND NOT EXISTS (
                                                 SELECT 1 FROM edges e
                                                 WHERE ((e.source_id = rg.holon_id AND e.target_id = rg.partner_id)
                                                    OR (e.source_id = rg.partner_id AND e.target_id = rg.holon_id))
                                                   AND e.valid_to IS NULL
                                             )
                                             LIMIT 20",
                                        );
                                        match stmt_result {
                                            Ok(mut stmt) => {
                                                stmt.query_map([], |row| {
                                                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                                                })
                                                .ok()
                                                .map(|iter| iter.filter_map(|r| r.ok()).collect())
                                                .unwrap_or_default()
                                            }
                                            Err(_) => { return; }
                                        }
                                    };

                                    let mut created = 0;
                                    for (holon_id, partner_id, score) in &pairs {
                                        let _ = conn.execute(
                                            "INSERT INTO edges (id, source_id, target_id, edge_type, weight, properties_json, created_at, updated_at, agent_id)
                                             VALUES (?1, ?2, ?3, 'RESONATES_WITH', 0.3, ?4, ?5, ?5, 'synaptogenesis')",
                                            rusqlite::params![
                                                format!("e{}", uuid::Uuid::new_v4().simple()),
                                                holon_id, partner_id,
                                                serde_json::json!({"resonance_score": score, "auto_created": true}).to_string(),
                                                chrono::Utc::now().to_rfc3339(),
                                            ],
                                        );
                                        created += 1;
                                    }

                                    if created > 0 {
                                        tracing::info!(
                                            "Synaptogenesis: {} new RESONATES_WITH edges created",
                                            created
                                        );
                                    }
                                    pool.release_connection(conn);
                                }
                            }).await.ok();
                        }
                        _ = memory_replay_ticker.tick() => {
                            // Phase 18: Memory replay (sleep consolidation).
                            let pool = maintenance_pool.clone();
                            tokio::task::spawn_blocking(move || {
                                if let Ok(conn) = pool.get_connection() {
                                    let node_ids: Vec<String> = {
                                        let cutoff = chrono::Utc::now() - chrono::Duration::hours(24);
                                        let stmt_result = conn.prepare(
                                            "SELECT DISTINCT node_id FROM events
                                             WHERE node_id IS NOT NULL
                                               AND timestamp > ?1
                                             LIMIT 50",
                                        );
                                        match stmt_result {
                                            Ok(mut stmt) => {
                                                stmt.query_map(rusqlite::params![cutoff.to_rfc3339()], |row| row.get(0))
                                                    .ok()
                                                    .map(|iter| iter.filter_map(|r| r.ok()).collect())
                                                    .unwrap_or_default()
                                            }
                                            Err(_) => { return; }
                                        }
                                    };

                                    let mut replayed = 0;
                                    for node_id in &node_ids {
                                        let _ = tdg_rust::metabolism::worker::enqueue_job(
                                            &conn, node_id,
                                            tdg_rust::metabolism::worker::JobType::CatalystInjection,
                                            serde_json::json!({"catalyst_amount": 0.3, "source": "memory_replay"}),
                                            tdg_rust::metabolism::worker::PRIORITY_LOW,
                                        );
                                        replayed += 1;
                                    }

                                    // Value-based forgetting
                                    let forget_cutoff = chrono::Utc::now() - chrono::Duration::days(30);
                                    let now = chrono::Utc::now().to_rfc3339();
                                    let forgotten = conn.execute(
                                        "UPDATE nodes SET lifecycle_state = 'archived', valid_to = ?1
                                         WHERE valid_to IS NULL
                                           AND retrieval_count = 0
                                           AND confidence < 0.3
                                           AND created_at < ?2
                                           AND node_type NOT IN ('telos', 'skill', 'capability')",
                                        rusqlite::params![now, forget_cutoff.to_rfc3339()],
                                    ).unwrap_or(0);

                                    if replayed > 0 || forgotten > 0 {
                                        tracing::info!(
                                            "Memory replay: {} nodes re-activated, {} forgotten",
                                            replayed, forgotten
                                        );
                                    }
                                    pool.release_connection(conn);
                                }
                            }).await.ok();
                        }
                    }
                }
            });
            tracing::info!(
                "Background maintenance scheduler started (self_manager={}s, health_check={}s)",
                self_manager_interval_secs,
                health_check_interval_secs
            );

            // Phase 2: Spawn the metabolism worker pool.
            //
            // The metabolism worker processes Tier 2 async jobs from the
            // pending_metabolism table — lesser cycle ticks, catalyst
            // injections, and (in Phase 3) attractor field recomputations.
            //
            // Default: 1 worker (lean VPS profile). Configurable via
            // TDG_METABOLISM_WORKERS env var. Each worker holds its own
            // SQLite connection from the maintenance pool.
            let metabolism_workers = std::env::var("TDG_METABOLISM_WORKERS")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            let metabolism_handle = rt.spawn(async move {
                let worker = tdg_rust::metabolism::MetabolismWorker::new(
                    metabolism_pool,
                    metabolism_workers,
                );
                worker.run().await;
            });
            tracing::info!(
                "Metabolism worker pool started ({} worker{})",
                metabolism_workers,
                if metabolism_workers == 1 { "" } else { "s" }
            );

            // Use stdio transport by default (for AI integration)
            // If port is non-default (not 3000), use HTTP transport
            if port != 3000 {
                rt.block_on(server::serve_http(pool, port))?;
            } else {
                rt.block_on(server::serve_stdio(pool))?;
            }
            // When the server shuts down, cancel the background tasks.
            maintenance_handle.abort();
            metabolism_handle.abort();
        }
    }

    Ok(())
}
