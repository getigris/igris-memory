# Development Guide

This document covers the internals of Igris Memory for contributors and developers.

## Prerequisites

- Rust 1.94+ (edition 2024)
- SQLite development headers (usually bundled via `rusqlite`)

## Setup

```bash
git clone https://github.com/getigris/igris-memory.git
cd igris-memory
git config core.hooksPath .githooks   # Activate pre-commit hooks
cargo build
cargo test
```

## Build & Test Commands

```bash
cargo build --release          # Build release binary (igmem)
cargo test                     # Run all tests
cargo test <test_name>         # Run a single test
cargo test --test db_test      # Run a specific test file
cargo clippy -- -D warnings    # Lint (warnings = errors)
cargo fmt --check              # Check formatting
cargo fmt                      # Auto-format
```

Pre-commit hooks (`.githooks/`) run `fmt --check`, `clippy`, and `test` automatically before each commit.

## Architecture

Igris Memory is a single-binary (`igmem`) persistent memory server for AI coding agents. It stores "observations" (memories) in SQLite with FTS5 full-text search and optional SQLCipher encryption.

### Runtime Modes

The binary dispatches to one of four modes based on the CLI command:

| Mode | Command | Transport | Crate |
|------|---------|-----------|-------|
| MCP stdio server | `igmem` (default) | stdin/stdout | `rmcp` |
| HTTP REST API | `igmem serve --port 7437` | TCP | `axum` |
| TUI browser | `igmem tui` | Terminal | `ratatui` |
| Sync | `igmem sync export/import --dir` | Filesystem | — |

### Module Map

```
src/
├── main.rs          # Entry: CLI parse → logging → DB init → mode dispatch
├── cli.rs           # clap derive structs (Cli, Command, ServeArgs, SyncArgs)
├── schema.rs        # SQL schema v1: tables, FTS5, triggers, indices, pragmas
├── db/
│   ├── mod.rs           # Database struct (rusqlite Connection), init, schema apply
│   ├── observations.rs  # CRUD + topic-key upsert + SHA-256 dedup (15-min window)
│   ├── search.rs        # FTS5 queries, recent context, stats aggregation
│   ├── sessions.rs      # Session lifecycle
│   ├── timeline.rs      # Chronological before/after queries
│   ├── export.rs        # Full export/import with hash-based dedup
│   └── purge.rs         # Hard-delete soft-deleted entries + VACUUM
├── server/
│   ├── mod.rs       # IgrisServer with #[tool_router] — 15 MCP tools
│   └── args.rs      # Tool parameter schemas (schemars JsonSchema)
├── http/
│   ├── mod.rs       # Axum server setup, AppState = Arc<Mutex<Database>>
│   └── routes.rs    # 16 REST endpoints
├── tui/
│   ├── mod.rs       # App state, Screen enum, refresh logic
│   ├── handler.rs   # Keyboard event handling (vim-style + arrows)
│   └── ui.rs        # ratatui rendering (tabs, table, detail, search, stats)
├── models/          # Observation, Session, SearchResult, Timeline, Stats, ExportData
├── errors.rs        # IgrisError with ErrorCode → HTTP status mapping
├── validation.rs    # Type/scope validation, non-empty checks
├── topic.rs         # suggest_topic_key: type → family, title → slug
├── utils.rs         # strip_private_tags, hash_content (SHA-256), now_utc
└── sync.rs          # Chunked file export/import with manifest
```

### Key Design Patterns

- **Thread safety**: `Arc<Mutex<Database>>` shared across MCP/HTTP handlers
- **Topic-key upsert**: same `topic_key` updates the existing observation in place (increments `revision_count`) rather than creating a duplicate
- **Content dedup**: SHA-256 of whitespace-normalized content; identical saves within 15 minutes increment `duplicate_count` instead of inserting
- **Privacy stripping**: `<private>...</private>` regex → `[REDACTED]` before storage on both title and content
- **Soft deletes**: `deleted_at` timestamp, all queries filter `WHERE deleted_at IS NULL`; `igris_purge` hard-deletes + VACUUMs
- **FTS5 sync**: INSERT/UPDATE/DELETE triggers keep `observations_fts` in sync with `observations`
- **Logging to stderr**: stdout is reserved for MCP stdio transport; all tracing goes to stderr

### Database

SQLite with WAL mode, `busy_timeout=5000`, `synchronous=NORMAL`. Optional SQLCipher encryption via `--db-key` or `IGRIS_DB_KEY` env var.

| Location | When |
|----------|------|
| `~/.igris/memory.db` | Default (global) |
| `~/.igris/projects/{name}/memory.db` | With `--project-scoped` |

### Valid Observation Types

`decision`, `architecture`, `bugfix`, `pattern`, `config`, `discovery`, `learning`, `plan`, `manual`

### Valid Scopes

`project`, `personal`

## Cross-Compilation

Uses `cross` for `aarch64-unknown-linux-gnu`. See `Cross.toml` for OpenSSL setup. Windows requires `build.rs` linking `crypt32` and `user32`.

```bash
# Linux ARM64
cross build --release --target aarch64-unknown-linux-gnu

# Linux x64
cargo build --release --target x86_64-unknown-linux-gnu

# macOS ARM64 (native on Apple Silicon)
cargo build --release

# Windows x64 (from Windows or cross)
cargo build --release --target x86_64-pc-windows-msvc
```

## Release

Tag with `v*` to trigger `.github/workflows/release.yml`:

```bash
git tag v0.2.0
git push origin v0.2.0
```

The workflow:
1. Builds for Linux x64/ARM64, macOS ARM64, Windows x64
2. Creates GitHub Release with SHA-256 checksums
3. Updates the Homebrew formula at `getigris/homebrew-tap`

## Project Structure

```
.
├── AGENTS.md          # AI agent instructions (CLAUDE.md symlinks here)
├── CONTRIBUTING.md    # Contribution guidelines
├── DEVELOPMENT.md     # This file
├── README.md          # User-facing documentation
├── LICENSE            # Elastic License 2.0
├── Cargo.toml         # Dependencies and metadata
├── Cross.toml         # Cross-compilation config
├── build.rs           # Windows linker flags
├── .githooks/         # Pre-commit hooks (fmt, clippy, test)
├── .github/
│   ├── CODEOWNERS     # @adiazblanco owns all files
│   └── workflows/     # CI/CD (release.yml)
├── src/               # Source code (see Module Map above)
├── tests/             # Integration tests
└── dist/              # Install scripts
```
