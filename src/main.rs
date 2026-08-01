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

pub(crate) struct EmbedRunSummary {
    pub done: usize,
    pub total: usize,
}

/// Runs the embed backfill loop against every observation missing an
/// embedding for `embedder.model()`. Extracted from `main()` so it's
/// unit-testable without spawning the process.
pub(crate) fn run_embed_backfill(
    db: &Database,
    embedder: &dyn crate::embed::Embedder,
) -> anyhow::Result<EmbedRunSummary> {
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
    Ok(EmbedRunSummary { done, total })
}

/// Whether `igmem embed --rebuild-index` should actually rebuild the vec0
/// index: only when the flag was passed AND the vector index is enabled.
pub(crate) fn should_rebuild_index(rebuild_index: bool, vector_index_enabled: bool) -> bool {
    rebuild_index && vector_index_enabled
}

// `main()` is the process entrypoint (CLI dispatch, real DB, stdio/HTTP
// serving); exercising it requires spawning the compiled binary, a test
// tier this crate doesn't have — equivalent mutant.
#[cfg_attr(test, mutants::skip)]
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
            let summary = run_embed_backfill(&db, embedder.as_ref())?;
            println!(
                "Embedded {}/{} observations with model '{}'.",
                summary.done,
                summary.total,
                embedder.model()
            );

            if should_rebuild_index(rebuild_index, cli.vector_index_enabled()) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::{Embedder, HashEmbedder};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Wraps `HashEmbedder` but fails every 2nd call, so tests can assert an
    /// exact `done` strictly less than `total` — only the correct `+=` (not
    /// `-=`/`*=`) reproduces that count.
    struct FlakyEmbedder {
        inner: HashEmbedder,
        calls: AtomicUsize,
    }

    impl FlakyEmbedder {
        fn new() -> Self {
            Self {
                inner: HashEmbedder::new(16),
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl Embedder for FlakyEmbedder {
        fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
            let call_number = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if call_number.is_multiple_of(2) {
                return Err("simulated embed failure".to_string());
            }
            self.inner.embed(text)
        }

        fn dimensions(&self) -> usize {
            self.inner.dimensions()
        }

        fn model(&self) -> &str {
            self.inner.model()
        }
    }

    #[test]
    fn run_embed_backfill_counts_successes_and_skips_failures() {
        let db = Database::open_in_memory().unwrap();
        for i in 0..4 {
            db.save_observation(
                &format!("t{i}"),
                &format!("content {i}"),
                "manual",
                None,
                "project",
                None,
                None,
                None,
            )
            .unwrap();
        }

        let embedder = FlakyEmbedder::new();
        let summary = run_embed_backfill(&db, &embedder).unwrap();

        assert_eq!(summary.total, 4);
        assert_eq!(summary.done, 2);
        assert!(summary.done < summary.total);

        // failed embeds leave their observation pending; successes don't.
        let still_pending = db.observations_needing_embedding(embedder.model()).unwrap();
        assert_eq!(still_pending.len(), 2);
    }

    #[test]
    fn should_rebuild_index_truth_table() {
        assert!(should_rebuild_index(true, true));
        assert!(!should_rebuild_index(true, false));
        assert!(!should_rebuild_index(false, true));
        assert!(!should_rebuild_index(false, false));
    }
}
