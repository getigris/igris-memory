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
            data_dir.join("projects").join(&project_name).join("memory.db")
        } else {
            data_dir.join("memory.db")
        }
    }
    /// Resolve the database encryption key: CLI flag > env var > None
    pub fn resolve_db_key(&self) -> Option<String> {
        self.db_key.clone().or_else(|| std::env::var("IGRIS_DB_KEY").ok())
    }
}

#[cfg(test)]
#[path = "tests/cli_test.rs"]
mod tests;
