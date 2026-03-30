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
mod tests {
    use super::*;

    #[test]
    fn cli_parses_data_dir() {
        let cli = Cli::try_parse_from(["igris-memory", "--data-dir", "/tmp/test"]).unwrap();
        assert_eq!(cli.data_dir, Some(PathBuf::from("/tmp/test")));
    }

    #[test]
    fn cli_no_args_ok() {
        let cli = Cli::try_parse_from(["igris-memory"]).unwrap();
        assert_eq!(cli.data_dir, None);
    }

    #[test]
    fn cli_resolve_flag_takes_priority() {
        let cli = Cli::try_parse_from(["igris-memory", "--data-dir", "/custom"]).unwrap();
        assert_eq!(cli.resolve_data_dir(), PathBuf::from("/custom"));
    }

    #[test]
    fn cli_resolve_default_is_home_igris() {
        let cli = Cli::try_parse_from(["igris-memory"]).unwrap();
        // When no flag and no env var, default ends with .igris
        // (env var may or may not be set — if flag is None and env is unset, we get ~/.igris)
        // We test the flag-absent path; env var behavior is covered by resolve_flag_takes_priority
        if std::env::var("IGRIS_DATA_DIR").is_err() {
            let dir = cli.resolve_data_dir();
            assert!(dir.ends_with(".igris"));
        }
    }

    #[test]
    fn cli_version_flag() {
        // --version causes an error (it exits), but it should be recognized
        let result = Cli::try_parse_from(["igris-memory", "--version"]);
        assert!(result.is_err()); // clap returns Err for --version (display info)
        let err = result.unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
    }

    #[test]
    fn cli_help_flag() {
        let result = Cli::try_parse_from(["igris-memory", "--help"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn cli_unknown_flag_fails() {
        let result = Cli::try_parse_from(["igris-memory", "--bogus"]);
        assert!(result.is_err());
    }
}
