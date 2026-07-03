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
                            5,
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
                    eprintln!("ERROR: libonnxruntime.so.1 not found at {}", lib_path.display());
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
            let maintenance_pool = std::sync::Arc::new(
                ConnectionPool::new(
                    config.db_path.to_str()
                        .ok_or_else(|| anyhow::anyhow!("Database path is not valid UTF-8"))?,
                    2,
                    30000,
                )?
            );
            let maintenance_handle = rt.spawn(async move {
                let self_manager_interval = std::time::Duration::from_secs(6 * 60 * 60);
                let health_check_interval = std::time::Duration::from_secs(5 * 60);
                let mut self_manager_ticker =
                    tokio::time::interval(self_manager_interval);
                let mut health_check_ticker =
                    tokio::time::interval(health_check_interval);
                // Skip the first immediate tick (we don't want to run maintenance
                // during startup — let the server stabilize first).
                self_manager_ticker.tick().await;
                health_check_ticker.tick().await;
                loop {
                    tokio::select! {
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
                    }
                }
            });
            tracing::info!("Background maintenance scheduler started (self_manager=6h, health_check=5m)");

            // Use stdio transport by default (for AI integration)
            // If port is non-default (not 3000), use HTTP transport
            if port != 3000 {
                rt.block_on(server::serve_http(pool, port))?;
            } else {
                rt.block_on(server::serve_stdio(pool))?;
            }
            // When the server shuts down, cancel the maintenance scheduler.
            maintenance_handle.abort();
        }
    }

    Ok(())
}
