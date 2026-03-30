use clap::Parser;
use std::path::PathBuf;

/// Igris Memory — persistent memory server for AI coding agents.
#[derive(Parser, Debug)]
#[command(name = "igris-memory", version, about)]
pub struct Cli {
    /// Override the data directory (default: ~/.igris or $IGRIS_DATA_DIR)
    #[arg(long = "data-dir", value_name = "PATH")]
    pub data_dir: Option<PathBuf>,
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
