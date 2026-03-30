use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Igris Memory — persistent memory server for AI coding agents.
#[derive(Parser, Debug)]
#[command(name = "igris-memory", version, about)]
pub struct Cli {
    /// Override the data directory (default: ~/.igris or $IGRIS_DATA_DIR)
    #[arg(long = "data-dir", value_name = "PATH")]
    pub data_dir: Option<PathBuf>,

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
}

#[cfg(test)]
#[path = "tests/cli_test.rs"]
mod tests;
