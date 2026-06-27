#![allow(dead_code)] // Binary uses library public API — dead_code warnings are for library consumers

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
        Commands::Serve { port } => {
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
            let rt = tokio::runtime::Runtime::new()?;
            // Use stdio transport by default (for AI integration)
            // If port is non-default (not 3000), use HTTP transport
            if port != 3000 {
                rt.block_on(server::serve_http(pool, port))?;
            } else {
                rt.block_on(server::serve_stdio(pool))?;
            }
        }
    }

    Ok(())
}
