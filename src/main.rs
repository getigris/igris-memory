mod cli;
mod db;
mod errors;
mod models;
mod schema;
mod server;
mod utils;
mod validation;

use crate::cli::Cli;
use crate::db::Database;
use crate::server::IgrisServer;
use clap::Parser;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::{self, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Logging goes to stderr — stdout is reserved for MCP transport.
    // Priority: IGRIS_LOG > RUST_LOG > default (info)
    let env_filter = std::env::var("IGRIS_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| "info".to_string());

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&env_filter))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let dir = cli.resolve_data_dir();
    std::fs::create_dir_all(&dir)?;
    let db_path = dir.join("memory.db");

    tracing::info!("Igris Memory starting — db at {}", db_path.display());

    let db = Database::open(&db_path)?;
    let server = IgrisServer::new(db);

    let service = server
        .serve(stdio())
        .await
        .inspect_err(|e| {
            tracing::error!("MCP serve error: {:?}", e);
        })?;

    service.waiting().await?;
    Ok(())
}
