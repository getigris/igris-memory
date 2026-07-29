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
├── schema.rs        # SQL schema v1+v2+v3+v4: tables, FTS5, triggers, indices, pragmas (v2 = entity graph, v3 = embeddings, v4 = vec0 index)
├── store.rs         # BrainStore trait — storage contract (entity/graph surface)
├── embed.rs         # Embedder trait, HashEmbedder, vector utils, register_sqlite_vec (statically-linked)
├── db/
│   ├── mod.rs           # Database struct (rusqlite Connection), init, schema apply
│   ├── observations.rs  # CRUD + topic-key upsert + SHA-256 dedup (15-min window)
│   ├── entities.rs      # BrainStore impl: entity upsert/get + alias resolution + graph trait methods
│   ├── graph.rs         # Inherent edge helpers used by entities.rs's BrainStore impl: row_to_edge, EDGE_COLS
│   ├── search.rs        # FTS5 queries, recent context, stats aggregation
│   ├── embeddings.rs    # embedding storage + brute-force vector_search + hybrid_search (RRF), VectorIndex seam
│   ├── vecindex.rs      # VectorIndex trait, brute-force default, vec0 ANN path with post-filter, fallback
│   ├── sessions.rs      # Session lifecycle
│   ├── timeline.rs      # Chronological before/after queries
│   ├── export.rs        # Full export/import with hash-based dedup
│   └── purge.rs         # Hard-delete soft-deleted entries + VACUUM
├── server/
│   ├── mod.rs       # IgrisServer with #[tool_router] — 27 MCP tools
│   ├── notify.rs    # Best-effort MCP logging/progress notifications (logging capability, dynamic level via logging/setLevel)
│   └── args.rs      # Tool parameter schemas (schemars JsonSchema)
├── http/
│   ├── mod.rs       # Axum server setup, AppState = Arc<Mutex<Database>>
│   └── routes.rs    # 16 REST endpoints
├── tui/
│   ├── mod.rs       # App state, Screen enum, refresh logic
│   ├── handler.rs   # Keyboard event handling (vim-style + arrows)
│   └── ui.rs        # ratatui rendering (tabs, table, detail, search, stats)
├── models/          # Observation, Session, SearchResult, Timeline, Stats, ExportData, Entity, Edge, EntityNeighbor, EntityBrief
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

### Vector Search (Fase 1c)

#### Overview

Vector search uses the `VectorIndex` seam to abstract the retrieval backend:

- **Default (brute-force)**: Exhaustive cosine similarity over all embeddings. Exact results, no approximation.
- **Optional (sqlite-vec ANN)**: Approximate nearest neighbors via statically-linked sqlite-vec with post-filtering to ensure exactness. Faster for large memory sets.
- **Fallback**: If vec0 index is unavailable or corrupted, automatically falls back to brute-force.

#### Configuration

Enable the optional sqlite-vec ANN backend via CLI flag or environment variable:

**CLI flags:**
```bash
igmem --vector-index vec          # Use sqlite-vec (ANN with post-filter)
igmem --vector-index brute        # Use brute-force (default, explicit)
```

**Environment variables:**
```bash
IGRIS_VECTOR_INDEX=vec            # Use sqlite-vec
IGRIS_VECTOR_INDEX=brute          # Use brute-force
```

#### Schema (v4)

Schema v4 extends v3 with:

- `vec_index_meta`: tracks ANN index state (method, dimension, timestamp, schema version)
- `embeddings_vec`: a lazily-created vec0 virtual table (statically-linked sqlite-vec) that maintains the ANN index over embeddings

When `--vector-index vec` is enabled, `igmem embed --rebuild-index` triggers index creation/rebuild. Embeddings and the vec index are **derived caches** (not exported via `igris_export`).

#### sqlite-vec Integration

sqlite-vec is statically linked via `register_sqlite_vec` (in `embed.rs`), making it part of the single-binary distribution. Encryption (SQLCipher) is compatible and preserves the vec0 virtual table.

#### Retrieval Design

`vector_search` (called by `hybrid_search`):
1. If `--vector-index vec` is enabled and vec0 exists: run ANN query with over-fetch (kNN + safety margin), post-filter with exact cosine recomputed from the durable embedding blob
2. If fallback needed (no vec0 or error): brute-force cosine similarity over all embeddings
3. Similarity is always exact cosine, independent of vec0's internal distance metric

### Embeddings & Semantic Search Configuration

Embeddings are stored per (observation, model) as f32 BLOBs in the `embeddings` table. The `hybrid_search` function fuses full-text (FTS5) and semantic (cosine similarity) results via Reciprocal Rank Fusion (RRF). The engine is LLM-free; the query embedding is supplied by the caller.

**Embeddings are a derived cache**: they are not exported via `igris_export` and are rebuilt as needed (via `igmem embed --backfill`).

#### Enable Embeddings via Ollama

To enable semantic search via an external embedder (e.g., Ollama), configure one of the following:

**CLI flags:**
```bash
igmem --embedder ollama --embed-url http://localhost:11434 --embed-model nomic-embed-text
```

**Environment variables:**
```bash
IGRIS_EMBEDDER=ollama
IGRIS_EMBED_URL=http://localhost:11434
IGRIS_EMBED_MODEL=nomic-embed-text
```

**Backfill existing memories:**
```bash
igmem embed --backfill
```

To enable the optional sqlite-vec ANN backend, add `--vector-index vec --rebuild-index`:
```bash
igmem --embedder ollama --vector-index vec embed --backfill --rebuild-index
```

Once configured, `igris_save` automatically embeds new observations and `igris_search` returns hybrid results (semantic + keyword via RRF).

**Note on HTTP serve:** The HTTP REST API (`igmem serve`) currently supports keyword-only search. Hybrid search and automatic embedding on save for the HTTP interface is a deferred follow-up (MCP server wiring is complete in Fase 1b; HTTP parity is planned for Fase 1c).

### Sync

The `igmem sync export` command writes a complete database export to a directory containing:

```
sync-dir/
├── manifest.json             # Metadata: version, export timestamp, machine ID, counts
├── sessions.json             # Array of all sessions
├── entities.json             # Array of all entities (Fase 0a)
├── entity_aliases.json       # Array of all entity aliases (Fase 0a)
├── edges.json                # Array of all edges (entity relationships) (Fase 0a)
├── mentions.json             # Array of all observation-entity mentions (Fase 0a)
└── observations/
    ├── chunk_0000.json       # Observations 0–99
    ├── chunk_0001.json       # Observations 100–199
    └── ...
```

Chunked observation files allow large exports to be split and re-imported incrementally. The `igmem sync import` command reads this structure and populates the database, deduplicating observations by content hash and entities by slug, then remapping IDs to preserve entity-graph relationships.

### MCP Logs & Progress Notifications

Every MCP tool call emits best-effort `notifications/message` (a start log and an end log with `duration_ms`, plus any existing warning paths duplicated at `Warning` level) and, for bulk operations (`igris_export`, `igris_import`, `igris_purge`) and embedder-backed calls (`igris_save`, `igris_search`, `igris_entity_merge`), `notifications/progress` — but only when the client's request carries a `progressToken`. Both are fire-and-forget (`tokio::spawn`, never awaited by the tool): a client that ignores them, or a non-rmcp client that never sends a `progressToken`, sees identical tool behavior, return values, and latency to before this feature.

The client can raise or lower verbosity live via the standard `logging/setLevel` request (default minimum level: `info`). Notification payloads never include full `title`/`content` bodies — only ids, counts, lengths, and (for search queries) a 120-char truncated preview — since `content`/`title` may still contain unredacted `<private>...</private>` text at call time (redaction happens on write).

`igris_export`/`igris_import` report progress across the 6 portable sections (observations, sessions, entities, aliases, edges, mentions) via `Database::export_all_with_progress`/`import_data_with_progress`; `igris_purge` reports its 2 phases (hard-delete, `VACUUM`) via `Database::purge_with_progress`. The plain `export_all`/`import_data`/`purge` methods are unchanged thin wrappers, so `sync.rs` and the HTTP REST API are unaffected. See `src/server/notify.rs`.

### Database

SQLite with WAL mode, `busy_timeout=5000`, `synchronous=NORMAL`. Optional SQLCipher encryption via `--db-key` or `IGRIS_DB_KEY` env var.

| Location | When |
|----------|------|
| `~/.igris/memory.db` | Default (global) |
| `~/.igris/projects/{name}/memory.db` | With `--project-scoped` |

### Valid Observation Types

`decision`, `architecture`, `bugfix`, `pattern`, `config`, `discovery`, `learning`, `plan`, `manual`

### Valid Entity Kinds

`person`, `company`, `project`, `concept`, `place`, `product`, `other`

Note: an entity's slug is derived from its name only, so it is unique per project+scope *regardless of kind* — a person and a company sharing the same name resolve to one entity, and `igris_entity_upsert` is idempotent by name accordingly.

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
