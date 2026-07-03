use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Igris Memory — persistent memory server for AI coding agents.
#[derive(Parser, Debug)]
#[command(name = "igmem", version, about)]
pub struct Cli {
    /// Override the data directory (default: ~/.igris or $IGRIS_DATA_DIR)
    #[arg(long = "data-dir", value_name = "PATH")]
    pub data_dir: Option<PathBuf>,

    /// Use a separate database per project instead of one global DB
    #[arg(long)]
    pub project_scoped: bool,

    /// Project name (used with --project-scoped; defaults to current directory name)
    #[arg(long)]
    pub project: Option<String>,

    /// Encryption key for the database (or set IGRIS_DB_KEY env var)
    #[arg(long = "db-key", value_name = "KEY")]
    pub db_key: Option<String>,

    /// Embedding provider for semantic search: none (default), hash (dev/test), or ollama.
    #[arg(long, value_name = "PROVIDER")]
    pub embedder: Option<String>,

    /// Embedding model name (used with --embedder ollama).
    #[arg(long = "embed-model", value_name = "MODEL")]
    pub embed_model: Option<String>,

    /// Ollama base URL (used with --embedder ollama).
    #[arg(long = "embed-url", value_name = "URL")]
    pub embed_url: Option<String>,

    /// Vector search backend: brute (default, exact) or vec (sqlite-vec ANN, opt-in).
    #[arg(long = "vector-index", value_name = "BACKEND")]
    pub vector_index: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start an HTTP REST API server instead of the default MCP stdio transport.
    Serve {
        /// Port to listen on
        #[arg(long, short, default_value = "7437")]
        port: u16,

        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },

    /// Launch the interactive terminal UI for browsing and managing memories.
    Tui,

    /// Sync memories to/from a directory (git-friendly chunked JSON).
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },

    /// Embed observations that don't yet have an embedding (requires --embedder).
    Embed {
        /// Backfill embeddings for all existing observations.
        #[arg(long)]
        backfill: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum SyncAction {
    /// Export all memories to a sync directory.
    Export {
        /// Directory to export to
        #[arg(long, short)]
        dir: std::path::PathBuf,
    },
    /// Import memories from a sync directory.
    Import {
        /// Directory to import from
        #[arg(long, short)]
        dir: std::path::PathBuf,
    },
}

impl Cli {
    /// Resolve the data directory: CLI flag > env var > default (~/.igris)
    pub fn resolve_data_dir(&self) -> PathBuf {
        if let Some(ref dir) = self.data_dir {
            return dir.clone();
        }
        if let Ok(dir) = std::env::var("IGRIS_DATA_DIR") {
            return PathBuf::from(dir);
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".igris")
    }

    /// Resolve the full database file path.
    /// Global mode: `{data_dir}/memory.db`
    /// Project-scoped: `{data_dir}/projects/{project}/memory.db`
    pub fn resolve_db_path(&self) -> PathBuf {
        let data_dir = self.resolve_data_dir();

        if self.project_scoped {
            let project_name = self.project.clone().unwrap_or_else(|| {
                std::env::current_dir()
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                    .unwrap_or_else(|| "default".to_string())
            });
            data_dir
                .join("projects")
                .join(&project_name)
                .join("memory.db")
        } else {
            data_dir.join("memory.db")
        }
    }
    /// Resolve the database encryption key: CLI flag > env var > None
    pub fn resolve_db_key(&self) -> Option<String> {
        self.db_key
            .clone()
            .or_else(|| std::env::var("IGRIS_DB_KEY").ok())
    }

    /// Resolve the configured embedder: CLI flag > env var > none.
    /// Returns None (keyword-only) unless an embedder is explicitly configured.
    pub fn build_embedder(&self) -> Option<std::sync::Arc<dyn crate::embed::Embedder>> {
        use crate::embed::{HashEmbedder, OllamaEmbedder};
        use std::sync::Arc;

        let kind = self
            .embedder
            .clone()
            .or_else(|| std::env::var("IGRIS_EMBEDDER").ok())
            .unwrap_or_else(|| "none".to_string());

        match kind.as_str() {
            "hash" => Some(Arc::new(HashEmbedder::new(256))),
            "ollama" => {
                let model = self
                    .embed_model
                    .clone()
                    .or_else(|| std::env::var("IGRIS_EMBED_MODEL").ok())
                    .unwrap_or_else(|| "nomic-embed-text".to_string());
                let url = self
                    .embed_url
                    .clone()
                    .or_else(|| std::env::var("IGRIS_EMBED_URL").ok())
                    .unwrap_or_else(|| "http://localhost:11434".to_string());
                Some(Arc::new(OllamaEmbedder::new(url, model)))
            }
            _ => None,
        }
    }

    /// Whether the sqlite-vec ANN backend is enabled (CLI > env > default false).
    pub fn vector_index_enabled(&self) -> bool {
        let v = self
            .vector_index
            .clone()
            .or_else(|| std::env::var("IGRIS_VECTOR_INDEX").ok())
            .unwrap_or_else(|| "brute".to_string());
        v == "vec"
    }
}

#[cfg(test)]
#[path = "tests/cli_test.rs"]
mod tests;
