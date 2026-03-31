# AGENTS.md

This file provides guidance to AI agents working with code in this repository. **All context persistence MUST go through the Igris Memory MCP server** — do not rely on file-based memory or conversation context alone.

## Igris Memory MCP — Centralized Context Persistence

Igris Memory (`igris-memory`) is the single source of truth for cross-session context. Every agent MUST follow this protocol to ensure continuity, traceability, and shared understanding across sessions and providers.

### Session Lifecycle (MANDATORY)

Every conversation follows this lifecycle:

#### 1. SESSION START — Load context before any action

```
igris_session_start(id: "<uuid>", project: "igris-memory", directory: "/path/to/repo")
igris_context(project: "igris-memory", limit: 20)
```

- **Always call `igris_context` first** to load recent memories before writing code, making decisions, or answering questions.
- Review returned observations to understand: prior decisions, active plans, known bugs, architecture state, and patterns.
- If you need specific context, use `igris_search` with keywords before proceeding.

#### 2. DURING SESSION — Save observations proactively

Save memories as important events happen. **Do NOT batch saves for the end** — save immediately when:

| Event | Type | Example |
|-------|------|---------|
| User makes a decision | `decision` | "Use SQLCipher for encryption instead of custom AES" |
| Architecture is designed or changed | `architecture` | "Added FTS5 triggers for sync" |
| A bug is found and fixed | `bugfix` | "Fix: topic_key upsert was creating duplicates" |
| A reusable pattern emerges | `pattern` | "Use Arc<Mutex<Database>> for thread-safe DB access" |
| Configuration is set up or changed | `config` | "Set WAL mode + busy_timeout=5000" |
| Something unexpected is discovered | `discovery` | "rusqlite doesn't support async natively" |
| A concept is explained or understood | `learning` | "FTS5 tokenizer behavior with CJK characters" |
| An execution plan is created | `plan` | "Plan: implement HTTP rate limiting" |
| User explicitly asks to remember | `manual` | Whatever the user specifies |

#### 3. SESSION END — Summarize before closing

```
igris_session_summary(project: "igris-memory", content: "<structured summary>")
igris_session_end(id: "<session-uuid>", summary: "<brief summary>")
```

The session summary is the **most critical memory** — the next session loads it first via `igris_context`. Structure it as:

```
## Accomplished
- <what was done>

## Key Decisions
- <decisions made and why>

## Next Steps
- <what remains to be done>

## Open Issues
- <blockers or unknowns>
```

### MCP Tools Reference

#### Context & Discovery

| Tool | When to Use | Parameters |
|------|-------------|------------|
| `igris_context` | **Session start**. Load recent memories chronologically. | `project?`, `limit?` (default 20, max 50) |
| `igris_search` | Find specific past decisions, patterns, or context by keyword. | `query` (required), `project?`, `type?`, `limit?` |
| `igris_get` | Get full content of a memory after finding its ID via search/context. | `id` (required) |
| `igris_timeline` | Understand the sequence of events around a specific memory. | `observation_id` (required), `before?`, `after?` |
| `igris_stats` | Get memory store overview: totals, breakdowns by type/project. | _(none)_ |

#### Saving & Updating

| Tool | When to Use | Parameters |
|------|-------------|------------|
| `igris_save` | Save a new observation. Use `topic_key` for evolving knowledge. | `title`, `content` (required), `type?`, `project?`, `scope?`, `tags?`, `topic_key?`, `session_id?` |
| `igris_update` | Correct specific fields of an existing memory. | `id` (required), `title?`, `content?`, `type?`, `tags?`, `topic_key?` |
| `igris_suggest_topic_key` | Generate a consistent topic_key before saving. | `type`, `title`, `content` (all required) |

#### Lifecycle & Cleanup

| Tool | When to Use | Parameters |
|------|-------------|------------|
| `igris_session_start` | Register a new working session. | `id`, `project` (required), `directory?` |
| `igris_session_end` | Mark session as completed. | `id` (required), `summary?` |
| `igris_session_summary` | Save structured summary before ending. | `content`, `project` (required) |
| `igris_delete` | Soft-delete completed plans, outdated info. | `id` (required) |
| `igris_purge` | Permanently remove soft-deleted entries. Irreversible. | `older_than_days` (required, 0 = all) |

#### Data Portability

| Tool | When to Use | Parameters |
|------|-------------|------------|
| `igris_export` | Backup all memories as JSON. | _(none)_ |
| `igris_import` | Restore from JSON export. Deduplicates by content hash. | `data` (required) |

### Context Validation Protocol

**Before any significant action**, the agent MUST validate context:

1. **Before writing code**: `igris_search` for related decisions, architecture notes, and patterns in the area you're modifying.
2. **Before making a decision**: `igris_search` for prior decisions on the same topic. If a previous decision exists, acknowledge it and explain if you're changing course.
3. **Before creating a plan**: `igris_search(type: "plan")` to check for active plans. Update existing plans via `topic_key` instead of creating duplicates.
4. **Before fixing a bug**: `igris_search(type: "bugfix")` to check if this bug was previously encountered or related to a known fix.

### Topic Keys — Evolving Knowledge

Use `topic_key` to keep knowledge consolidated. Saving with an existing `topic_key` **updates in place** (increments `revision_count`) instead of creating a duplicate.

Call `igris_suggest_topic_key` to generate consistent keys. Convention:

```
{type_family}/{slug}

Examples:
  architecture/auth-middleware
  decision/encryption-strategy
  plan/http-rate-limiting
  bugfix/topic-key-upsert
  config/ci-pipeline
  pattern/thread-safe-db-access
```

**Plans**: Save execution plans as `type: "plan"` with a `topic_key` like `plan/feature-name`. Update the plan as it evolves. When complete, delete it with `igris_delete`.

### Privacy

Wrap sensitive values in `<private>...</private>` tags — automatically redacted to `[REDACTED]` before storage:

```
Database password is <private>super-secret-123</private>
→ stored as: Database password is [REDACTED]
```

### Rules

- **Never skip `igris_context` at session start** — you lose continuity without it.
- **Save immediately, not later** — if you wait, you might forget or the session might end.
- **Use `topic_key` for anything that evolves** — architecture, configs, active plans.
- **Delete completed plans** — they clutter context if left active.
- **Be detailed in content** — future sessions only know what you wrote. Include the *why*, not just the *what*.
- **Use `project: "igris-memory"`** for all observations related to this repository.
- **Scope**: Use `project` (default) for repo-specific knowledge, `personal` for user preferences.

---

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

Pre-commit hooks (`.githooks/`) run fmt check, clippy, and tests automatically. Activate with:
```bash
git config core.hooksPath .githooks
```

## Architecture

Igris Memory is a single-binary (`igmem`) persistent memory server for AI coding agents, written in Rust (edition 2024). It stores "observations" (memories) in SQLite with FTS5 full-text search and optional SQLCipher encryption.

### Runtime Modes

The binary dispatches to one of four modes based on the CLI command:

1. **MCP stdio server** (default, no command) — uses `rmcp` crate with stdio transport
2. **HTTP REST API** (`igmem serve --port 7437`) — Axum with CORS + tracing middleware
3. **TUI** (`igmem tui`) — ratatui interactive browser
4. **Sync** (`igmem sync export/import --dir`) — chunked JSON export/import

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

- **Global**: `~/.igris/memory.db`
- **Project-scoped** (`--project-scoped`): `~/.igris/projects/{name}/memory.db`

### Valid Observation Types

`decision`, `architecture`, `bugfix`, `pattern`, `config`, `discovery`, `learning`, `plan`, `manual`

### Valid Scopes

`project`, `personal`

## Cross-Compilation

Uses `cross` for `aarch64-unknown-linux-gnu`. See `Cross.toml` for OpenSSL setup. Windows requires `build.rs` linking `crypt32` and `user32`.

## Release

Tag with `v*` to trigger `.github/workflows/release.yml` which builds for Linux x64/ARM64, macOS ARM64, Windows x64, creates GitHub Release with checksums, and updates the Homebrew formula at `getigris/homebrew-tap`.
