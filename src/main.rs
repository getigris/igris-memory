mod cli;
mod codegraph;
mod db;
mod embed;
mod errors;
mod http;
mod models;
mod schema;
mod server;
mod store;
mod sync;
mod topic;
mod tui;
mod utils;
mod validation;

use crate::cli::{Cli, Command, SyncAction};
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

    let db_path = cli.resolve_db_path();
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    tracing::info!("Igris Memory starting — db at {}", db_path.display());

    let db_key = cli.resolve_db_key();
    let db = Database::open_with(&db_path, db_key.as_deref(), cli.vector_index_enabled())?;
    let embedder = cli.build_embedder();

    match cli.command {
        Some(Command::Serve { port, host }) => {
            http::serve(db, &host, port).await?;
        }
        Some(Command::Tui) => {
            tui::run(db)?;
        }
        Some(Command::Sync { action }) => match action {
            SyncAction::Export { dir } => {
                let manifest = sync::export_to_dir(&db, &dir)?;
                println!(
                    "Exported {} observations, {} sessions to {}",
                    manifest.observation_count,
                    manifest.session_count,
                    dir.display()
                );
            }
            SyncAction::Import { dir } => {
                let result = sync::import_from_dir(&db, &dir)?;
                println!(
                    "Imported: {} observations ({} skipped), {} sessions ({} skipped)",
                    result.observations_imported,
                    result.observations_skipped,
                    result.sessions_imported,
                    result.sessions_skipped
                );
            }
        },
        Some(Command::Embed {
            backfill: _backfill,
            rebuild_index,
        }) => {
            let embedder = embedder.ok_or_else(|| {
                anyhow::anyhow!("no embedder configured — set --embedder ollama (or hash)")
            })?;
            let pending = db.observations_needing_embedding(embedder.model())?;
            let total = pending.len();
            let mut done = 0usize;
            for (id, content) in pending {
                match embedder.embed(&content) {
                    Ok(vec) => {
                        db.upsert_embedding("observation", id, embedder.model(), &vec)?;
                        done += 1;
                    }
                    Err(e) => tracing::warn!(observation = id, error = %e, "embed failed"),
                }
            }
            println!(
                "Embedded {done}/{total} observations with model '{}'.",
                embedder.model()
            );

            if rebuild_index && cli.vector_index_enabled() {
                let n = db.vec_index_rebuild(embedder.model())?;
                println!(
                    "Rebuilt vec index with {n} vectors for model '{}'.",
                    embedder.model()
                );
            }
        }
        None => {
            let server = IgrisServer::with_embedder(db, embedder);
            let service = server.serve(stdio()).await.inspect_err(|e| {
                tracing::error!("MCP serve error: {:?}", e);
            })?;
            service.waiting().await?;
        }
    }

    Ok(())
}
