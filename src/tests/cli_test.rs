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
    if std::env::var("IGRIS_DATA_DIR").is_err() {
        let dir = cli.resolve_data_dir();
        assert!(dir.ends_with(".igris"));
    }
}

#[test]
fn cli_version_flag() {
    let result = Cli::try_parse_from(["igris-memory", "--version"]);
    assert!(result.is_err());
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

#[test]
fn cli_serve_default_port() {
    let cli = Cli::try_parse_from(["igris-memory", "serve"]).unwrap();
    match cli.command {
        Some(Command::Serve { port, host }) => {
            assert_eq!(port, 7437);
            assert_eq!(host, "127.0.0.1");
        }
        _ => panic!("expected Serve command"),
    }
}

#[test]
fn cli_serve_custom_port() {
    let cli = Cli::try_parse_from(["igris-memory", "serve", "--port", "8080"]).unwrap();
    match cli.command {
        Some(Command::Serve { port, .. }) => assert_eq!(port, 8080),
        _ => panic!("expected Serve command"),
    }
}

#[test]
fn cli_no_subcommand_is_mcp() {
    let cli = Cli::try_parse_from(["igris-memory"]).unwrap();
    assert!(cli.command.is_none(), "no subcommand = MCP mode");
}

#[test]
fn cli_serve_with_data_dir() {
    let cli = Cli::try_parse_from(["igris-memory", "--data-dir", "/tmp/db", "serve", "--port", "9000"]).unwrap();
    assert_eq!(cli.data_dir, Some(PathBuf::from("/tmp/db")));
    match cli.command {
        Some(Command::Serve { port, .. }) => assert_eq!(port, 9000),
        _ => panic!("expected Serve command"),
    }
}

// ─── Multi-project namespacing ──────────────────────────────────

#[test]
fn resolve_global_db_path() {
    let cli = Cli::try_parse_from(["igris-memory", "--data-dir", "/tmp/igris"]).unwrap();
    let path = cli.resolve_db_path();
    assert_eq!(path, PathBuf::from("/tmp/igris/memory.db"));
}

#[test]
fn resolve_scoped_with_project_name() {
    let cli = Cli::try_parse_from(["igris-memory", "--data-dir", "/tmp/igris", "--project-scoped", "--project", "foo"]).unwrap();
    let path = cli.resolve_db_path();
    assert_eq!(path, PathBuf::from("/tmp/igris/projects/foo/memory.db"));
}

#[test]
fn resolve_scoped_defaults_to_cwd() {
    let cli = Cli::try_parse_from(["igris-memory", "--data-dir", "/tmp/igris", "--project-scoped"]).unwrap();
    let path = cli.resolve_db_path();
    // Should use the current directory's basename
    let cwd_name = std::env::current_dir().unwrap();
    let expected_name = cwd_name.file_name().unwrap().to_str().unwrap();
    assert_eq!(path, PathBuf::from(format!("/tmp/igris/projects/{expected_name}/memory.db")));
}

#[test]
fn cli_parses_db_key() {
    let cli = Cli::try_parse_from(["igris-memory", "--db-key", "secret123"]).unwrap();
    assert_eq!(cli.resolve_db_key(), Some("secret123".to_string()));
}

#[test]
fn cli_db_key_none_by_default() {
    let cli = Cli::try_parse_from(["igris-memory"]).unwrap();
    // Only if env var is not set
    if std::env::var("IGRIS_DB_KEY").is_err() {
        assert_eq!(cli.resolve_db_key(), None);
    }
}

#[test]
fn project_flag_ignored_without_scoped() {
    let cli = Cli::try_parse_from(["igris-memory", "--data-dir", "/tmp/igris", "--project", "foo"]).unwrap();
    let path = cli.resolve_db_path();
    assert_eq!(path, PathBuf::from("/tmp/igris/memory.db"), "--project without --project-scoped should use global DB");
}
