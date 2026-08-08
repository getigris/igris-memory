# Changelog

All notable changes to Igris Memory will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-08-08

### Added
- Entity graph (Fase 0): `igris_entity_upsert/get/link/neighbors/search/list/delete/unlink/update/merge`, `igris_entity_timeline`, `igris_brief` with deterministic Compiled Truth, and entity mentions recorded by `igris_save`
- Hybrid retrieval (Fase 1): `Embedder` trait with `OllamaEmbedder` and `HashEmbedder`, `hybrid_search` fusing FTS5 and vectors via RRF, `--embedder` config, and `igmem embed --backfill`/`--rebuild-index`
- Optional sqlite-vec ANN backend (Fase 1c) with brute-force fallback (`--vector-index`)
- Code graph (Fase 2): tree-sitter based extractors for Rust, TypeScript/TSX, JavaScript, Python, Go, Java, C, C++, C#, Ruby, PHP, Swift, and Kotlin; indexer orchestration with background indexing on `igris_session_start`; and new MCP tools `igris_code_search`, `igris_code_neighbors`, `igris_code_path`, `igris_code_map`
- MCP log/progress notifications for entity, observation, and session tools; dynamic log level
- Entity graph round-tripped through export/import and sync
- Mutation-testing CI gate (`cargo-mutants --in-diff`) on PR diffs

### Changed
- `igris_stats` includes entity and edge counts
- Entity slugs are unique per project+scope regardless of kind (idempotent upsert by name)

### Fixed
- Entity merge wrapped in a transaction; atomic entity upsert
- Cascade mentions/edges on delete to unblock purge; deterministic neighbor order
- v1 → v2 schema migration preserves data

## [0.1.2] - 2026-03-31

### Security
- Pin third-party GitHub Actions to commit SHAs (ncipollo/release-action, dmnemec/copy_file_to_another_repo_action)
- Pin cross-rs/cross to tag v0.2.5 instead of git HEAD
- Add SHA-256 checksum verification to install.sh

## [0.1.1] - 2026-03-31

### Added
- SECURITY.md with vulnerability reporting policy
- CONTRIBUTING.md with collaboration guidelines
- DEVELOPMENT.md with architecture and developer documentation
- CODEOWNERS for mandatory review by @adiazblanco
- GitHub issue and PR templates
- Mermaid diagrams with animations in README

### Changed
- AGENTS.md restructured as pure AI agent instructions with Igris Memory MCP protocol
- README.md Development section now points to DEVELOPMENT.md
- Cargo.toml metadata: added authors, homepage, keywords, categories

## [0.1.0] - 2025-05-01

### Added
- Initial release
- MCP stdio server with 15 tools (save, search, get, update, delete, context, stats, timeline, suggest_topic_key, export, import, purge, session_start, session_end, session_summary)
- HTTP REST API with 16 endpoints via Axum
- TUI interactive browser via ratatui
- Sync export/import with chunked JSON and manifest
- SQLite with FTS5 full-text search
- Optional SQLCipher encryption (--db-key / IGRIS_DB_KEY)
- Topic-key upsert for evolving knowledge (same key updates in place)
- SHA-256 content dedup with 15-minute window
- Privacy redaction via `<private>` tags
- Soft deletes with igris_purge for permanent cleanup
- Session lifecycle management
- Project-scoped databases (--project-scoped)
- Cross-platform builds: Linux x64/ARM64, macOS ARM64, Windows x64
- Shell installer (dist/install.sh) with auto-detection
- Homebrew formula (getigris/tap/igris-memory)
- CI pipeline with fmt, clippy, and tests on Ubuntu/macOS/Windows
- Release pipeline with GitHub Releases and SHA-256 checksums
- Pre-commit hooks (.githooks/) for fmt, clippy, and tests

[0.2.0]: https://github.com/getigris/igris-memory/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/getigris/igris-memory/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/getigris/igris-memory/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/getigris/igris-memory/releases/tag/v0.1.0
