mod db;
mod models;
mod schema;
mod server;
mod utils;

use crate::db::Database;
use crate::server::IgrisServer;
use rmcp::{ServiceExt, transport::stdio};
use std::path::PathBuf;
use tracing_subscriber::{self, EnvFilter};

fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("IGRIS_DATA_DIR") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".igris")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logging goes to stderr — stdout is reserved for MCP transport
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let dir = data_dir();
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
