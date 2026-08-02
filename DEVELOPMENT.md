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
cargo mutants --file <path>    # Mutation-test a single file (a few minutes)
```

Pre-commit hooks (`.githooks/`) run `fmt --check`, `clippy`, and `test` automatically before each commit.

### Mutation testing

CI runs a mutation-testing gate (`cargo mutants --in-diff diff.txt --in-place`, see `.github/workflows/ci.yml`) that fails a PR if it introduces code with no test able to detect a behavioral mutation (e.g. flipping `<` to `<=`, or `+=` to `-=`). Before pushing, check your own diff locally with the much faster scoped form:

```bash
cargo mutants --file src/your_changed_file.rs
```

`.cargo/mutants.toml` sets `exclude_globs` for files with nothing meaningful to mutate (e.g. `src/tui/ui.rs`'s rendering code, generated-shape extractor tables) — don't add entries there to work around a real gap in test coverage.

A non-zero exit code fails the CI step exactly the same way whether it's exit code 2 (a mutant survived) or exit code 3 (a mutation run timed out) — a hanging test is just as gate-blocking as an uncaught mutant, so a test that can hang under a plausible mutation (e.g. an `await` on a channel/response that never arrives if the code under test panics) needs its own bounded timeout, not just a correctness assertion.

For a mutation that is genuinely equivalent — no test could ever observe a behavioral difference for any real input — prefer restructuring the code to remove the ambiguous operator entirely (e.g. `a.min(b)`/`a.max(b)` instead of `if a < b {...} else {...}` when the values are guaranteed distinct) over skipping it. Only reach for `#[cfg_attr(test, mutants::skip)]` (never a bare `#[mutants::skip]` — `mutants` is a dev-only dependency, so a bare attribute breaks `cargo build --release`) when no such restructuring is possible, e.g. a process entrypoint like `main()`, or an idempotent migration-version gate where re-running a migration is a genuine no-op.

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
├── schema.rs        # SQL schema v1+v2+v3+v4+v5: tables, FTS5, triggers, indices, pragmas (v2 = entity graph, v3 = embeddings, v4 = vec0 index, v5 = code graph)
├── store.rs         # BrainStore trait — storage contract (entity/graph surface)
├── embed.rs         # Embedder trait, HashEmbedder, vector utils, register_sqlite_vec (statically-linked)
├── codegraph/
│   ├── mod.rs          # Re-exports: Language, LanguageExtractor(Registry), Extracted{Symbol,Edge}, ExtractionResult
│   ├── language.rs     # Language enum — one variant per Tier 1 language, extension → Language mapping
│   ├── walker.rs       # discover_files: gitignore-aware file discovery (ignore crate) + content hashing
│   ├── query_runner.rs # Shared run_query: parses source, runs a tree-sitter .scm query, folds captures into an ExtractionResult
│   ├── extractor.rs    # LanguageExtractor trait + LanguageExtractorRegistry (one extractor per Language)
│   ├── indexer.rs      # index_project: walk → diff by content hash → extract → persist → soft-delete missing files
│   ├── queries/*.scm   # Tree-sitter query per language (captures @name.function/@name.call/@name.import etc.)
│   └── extractors/*.rs # One LanguageExtractor impl per Tier 1 language (rust, javascript, typescript, python, go, java, c, cpp, csharp, ruby, php, swift, kotlin), each wiring its grammar + queries/*.scm into query_runner::run_query
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
│   ├── purge.rs         # Hard-delete soft-deleted entries + VACUUM
│   └── codegraph.rs     # Code graph persistence/query layer: file/symbol/edge upserts, name resolution, search, neighbors (BFS), shortest path, code_map
├── server/
│   ├── mod.rs       # IgrisServer with #[tool_router] — 31 MCP tools
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
│   └── codegraph.rs # CodeFile, CodeSymbol, CodeEdge, CodeNode (file|symbol), CodeNeighbor, CodeMap, IndexSummary
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

### Code Graph (Fase 2)

#### Overview

The code graph is a structural index of a project's source tree — files, symbols (functions/methods/classes/etc.), and the edges between them (imports and calls today — see the limits below) — extracted via [tree-sitter](https://tree-sitter.github.io/tree-sitter/) and persisted for fast structural queries (`igris_code_search`, `igris_code_neighbors`, `igris_code_path`, `igris_code_map`). It complements FTS5/vector search: those answer "what was said about X," the code graph answers "who calls/imports/depends on X."

13 Tier 1 languages are supported: Rust, JavaScript, TypeScript, TSX, Python, Go, Java, C, C++, C#, Ruby, PHP, Swift, Kotlin. Each has its own `LanguageExtractor` (`src/codegraph/extractors/*.rs`) that pairs a tree-sitter grammar with a `.scm` query file (`src/codegraph/queries/*.scm`) capturing `@name.function`, `@name.call`, and `@name.import` nodes; the shared `query_runner::run_query` (`src/codegraph/query_runner.rs`) does the actual parse-and-capture work so each extractor is just wiring, not a hand-rolled tree walk. A symbol's line range comes from the match's `@definition.*` wrapper capture (the whole definition body), not the `@name.*` identifier token — so a multi-line function gets a real `end_line > start_line`.

#### What the graph does and doesn't say today

The extractors deliberately stop short of full semantic analysis. Three limits are worth stating up front, because they change how results should be read:

- **`calls` edges are attributed to the containing file, not the calling symbol.** No current extractor sets `ExtractedEdge::src_qualified_name`, so every `calls` edge is sourced from the file node. `igris_code_neighbors` with `direction: "in"` therefore answers *"which files contain a call to this name"*, not *"which function calls this"*. Symbol-level call attribution is a Phase 2 follow-up.
- **`resolution` is only ever `"heuristic"` (calls) or `"static"` (imports).** The `"ambiguous"` value the design spec reserves is not produced by any current extractor.
- **Only `imports` and `calls` relations are produced.** `defines` and `references` are reserved for future extractors; filtering `igris_code_neighbors` by them matches nothing today.

#### Module Map

- `src/codegraph/language.rs` — `Language` enum, one variant per Tier 1 language, plus extension → `Language` mapping (files with an unrecognized extension are silently skipped, not treated as an error)
- `src/codegraph/walker.rs` — `discover_files`: gitignore-aware directory walk (via the `ignore` crate, the same one `ripgrep` uses) that returns every supported file with its content hash
- `src/codegraph/query_runner.rs` — `run_query`: parses source with a language's tree-sitter grammar, runs its `.scm` query, and folds captures into an `ExtractionResult`. Never panics on malformed input — a syntax error in one file degrades to fewer/incomplete captures for that file, not a crash of the whole index run
- `src/codegraph/extractor.rs` — `LanguageExtractor` trait (one impl per language) and `LanguageExtractorRegistry` (looks up the right extractor by `Language`)
- `src/codegraph/extractors/*.rs` — the 13 Tier 1 language extractors
- `src/codegraph/indexer.rs` — `index_project`: walks a root, reparses any file whose content hash changed since the last run (or that's new), persists symbols/edges, and soft-deletes any previously-indexed file no longer on disk
- `src/db/codegraph.rs` — persistence/query layer: file/symbol/edge upserts, cross-file name resolution (turning an edge's raw `dst_name` into a resolved `dst_id`/`dst_type` where possible), `search_code_nodes`, `code_neighbors` (BFS), `code_path` (shortest path, BFS capped at `max_hops`), and `code_map`
- `src/models/codegraph.rs` — `CodeFile`, `CodeSymbol`, `CodeEdge`, `CodeNode` (`File`/`Symbol`), `CodeNeighbor`, `CodeMap`, `IndexSummary`

#### MCP Tools

- **`igris_code_search`** — find code nodes (files/symbols) by name or path substring, optionally filtered by `kind`/`language`/`project`. Every symbol result carries its owning file's `relative_path` and `language` alongside `start_line`/`end_line`, so a hit is directly actionable (open that file at that line) without a second lookup. For structural questions ("who calls/imports/depends on X"), not free-text search inside file contents (use Grep/Glob for that) or knowledge about people/decisions/concepts (use `igris_entity_search` for that)
- **`igris_code_neighbors`** — connected code nodes (imports/calls) for a given node, expanded breadth-first for `hops` hops (default 1, minimum 1), with `resolution` confidence and `external_boundary` on each edge. Each edge is returned at most once and each node expanded at most once, so cycles terminate; results are ordered by hop distance. Remember the file-level attribution of `calls` edges described above — `direction: "in"` gives calling *files*, not calling functions. Absence of a `"static"` edge means the analyzer couldn't resolve a caller, not proof one doesn't exist — dynamic dispatch and reflection are blind spots
- **`igris_code_path`** — shortest path (BFS, capped at `max_hops`) between two code nodes, returning the edge chain connecting them, or a clean "not found" result (not an error) if no path exists within the hop cap
- **`igris_code_map`** — a one-call summary of a file: its symbols and strongest connections, meant for orienting in an unfamiliar part of the codebase before reading files directly

#### Schema (v5)

Schema v5 adds three tables, applied on top of v1–v4:

- `code_files`: one row per (project, root_path, relative_path), tracking `language` and a `content_hash` used to skip reparsing unchanged files on the next index run
- `code_symbols`: functions/methods/classes/etc. extracted from a `code_files` row, with `kind`, `name`, `qualified_name`, and line range. Reads always join `code_files`, so the `CodeSymbol` model also carries the owning file's `relative_path`/`language`
- `code_edges`: typed relations between two nodes. In practice `relation` is `calls` or `imports` — those are the only two any current extractor emits. `dst_id`/`dst_type` are `NULL` when the target couldn't be resolved within the index — the raw `dst_name` is kept so the fact isn't silently dropped. `resolution` records how confident the edge is (`"heuristic"` for calls, `"static"` for imports; exact same-scope resolution and the spec's `"ambiguous"` value are future work). `external_boundary` marks edges that leave the indexed project: today it is set for an `imports` edge whose target resolves to no in-project symbol (an unresolved import target is external by definition). Unresolved `calls` are *not* marked external — a call name may simply belong to a file not yet indexed in this run

Like embeddings and the vec0 index, the code graph is a **derived cache**: it's rebuilt from source on each index run and is not covered by `igris_export`/`igris_import`.

#### Background Indexing

`igris_session_start` triggers a background index of `directory` (if provided) immediately after registering the session — it doesn't block the tool's response. The indexing itself runs on a blocking task (`tokio::task::spawn_blocking`) since tree-sitter parsing and SQLite writes are both synchronous; a `notifications/message` log (`code_index`) reports `files_indexed`/`files_unchanged`/`files_skipped_unsupported`/`files_failed_parse`/`files_deleted` once it completes. A client that never calls the four `igris_code_*` tools, or that ignores the notification, sees no difference in `igris_session_start`'s own latency or return value.

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

Every MCP tool call emits a best-effort `notifications/message` end log with `duration_ms` (plus any existing warning paths duplicated at `Warning` level) — and, for all but two trivial tools (`igris_stats`, `igris_suggest_topic_key`, which are end-only by design), a matching start log. Bulk operations (`igris_export`, `igris_import`, `igris_purge`) and multi-step tools (`igris_entity_merge`) and embedder-backed calls (`igris_save`, `igris_search`) also emit `notifications/progress` — but only when the client's request carries a `progressToken`. Both are fire-and-forget (never awaited by the tool): a client that ignores them, or a non-rmcp client that never sends a `progressToken`, sees identical tool behavior, return values, and latency to before this feature.

The client can raise or lower verbosity live via the standard `logging/setLevel` request (default minimum level: `info`). Notification payloads never include full `title`/`content` bodies — only ids, counts, lengths, and (for search queries) a 120-char truncated preview — since `content`/`title` may still contain unredacted `<private>...</private>` text at call time (redaction happens on write).

Delivery order is guaranteed FIFO per server instance: every notification is enqueued onto a single channel drained by one long-lived task that awaits each send before moving to the next, so a client can rely on `progress` values for a given token increasing monotonically.

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
