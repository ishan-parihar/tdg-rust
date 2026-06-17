mod config;
mod db;
mod error;
mod flow;
mod hrr;
mod knowledge;
mod models;
mod mind;
mod validation;

use clap::{Parser, Subcommand};

use config::Config;
use db::{init_fts, init_schema, run_migrations, ConnectionPool};

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
        /// Port to listen on
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
                config.db_path.to_str().unwrap(),
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
                config.db_path.to_str().unwrap(),
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
            tracing::info!(
                "Backing up {} to {}",
                config.db_path.display(),
                output
            );
            let pool = ConnectionPool::new(
                config.db_path.to_str().unwrap(),
                5,
                30000,
            )?;
            pool.backup(std::path::Path::new(&output))?;
            tracing::info!("Backup completed");
            pool.close();
        }
        Commands::Stats => {
            let pool = ConnectionPool::new(
                config.db_path.to_str().unwrap(),
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
                    .prepare("SELECT node_type, COUNT(*) FROM nodes WHERE valid_to IS NULL GROUP BY node_type ORDER BY COUNT(*) DESC")
                    .unwrap();
                let rows = stmt.query_map([], |row| {
                    let t: String = row.get(0)?;
                    let c: i64 = row.get(1)?;
                    Ok((t, c))
                })?;
                println!("\nNodes by type:");
                for row in rows {
                    if let Ok((t, c)) = row {
                        println!("  {t}: {c}");
                    }
                }
                Ok(())
            })?;
            pool.close();
        }
        Commands::Serve { port } => {
            tracing::info!("Starting MCP server on port {port}");
            // Phase 11: Axum server
            tracing::warn!("MCP server not yet implemented (Phase 11)");
            println!("MCP server will be available in Phase 11. Current port: {port}");
        }
    }

    Ok(())
}
