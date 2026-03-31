# Changelog

All notable changes to Igris Memory will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.2]: https://github.com/getigris/igris-memory/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/getigris/igris-memory/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/getigris/igris-memory/releases/tag/v0.1.0
