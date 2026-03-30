# Igris Memory

> Persistent memory server for AI coding agents. Single Rust binary. SQLite + FTS5. MCP protocol.

[![License: Elastic-2.0](https://img.shields.io/badge/License-Elastic--2.0-blue.svg)](LICENSE)

---

## What is Igris Memory?

Igris Memory is an MCP (Model Context Protocol) server that gives AI coding agents persistent memory across sessions. It stores observations — decisions, architecture notes, bugfixes, patterns, and learnings — in a local SQLite database with full-text search.

**Key features:**

- **Deduplication** — identical content within a 15-minute window is counted, not duplicated
- **Topic-key upsert** — evolving knowledge (e.g. `architecture/auth`) updates in place instead of creating new entries
- **Privacy stripping** — wrap sensitive values in `<private>...</private>` tags and they're automatically redacted before storage
- **Full-text search** — FTS5-powered search with relevance ranking and snippets
- **Session tracking** — group observations by work sessions for context continuity
- **Soft deletes** — data is never permanently removed, just excluded from queries

## Quick Start

### Build from source

```bash
git clone https://github.com/getigris/igris-memory.git
cd igris-memory
cargo build --release
```

The binary will be at `target/release/igris-memory` (~7 MB).

### Configure with Claude Code

Add to your MCP settings (`~/.claude/settings.json` or project `.claude/settings.json`):

```json
{
  "mcpServers": {
    "igris-memory": {
      "command": "/path/to/igris-memory"
    }
  }
}
```

### Run standalone

```bash
# Uses default data directory ~/.igris/
./target/release/igris-memory

# Or specify a custom data directory
IGRIS_DATA_DIR=/tmp/test-memory ./target/release/igris-memory
```

## MCP Tools

Igris Memory exposes 10 tools via the MCP protocol:

### Memory Operations

| Tool | Description |
|------|-------------|
| `igris_save` | Save an observation with automatic deduplication, privacy stripping, and topic-key upsert |
| `igris_search` | Full-text search across all memories with ranking and snippets |
| `igris_get` | Retrieve a single observation by ID (full untruncated content) |
| `igris_update` | Partial update — only provided fields are changed |
| `igris_delete` | Soft-delete an observation (data kept but excluded from queries) |
| `igris_context` | Load recent memories — call at session start for continuity |
| `igris_stats` | Aggregate statistics: totals, counts by type and project |

### Session Management

| Tool | Description |
|------|-------------|
| `igris_session_start` | Register the start of a working session |
| `igris_session_end` | Mark a session as completed with optional summary |
| `igris_session_summary` | Save a structured session summary — the most valuable memory for continuity |

## Usage Examples

### Save a decision

```json
{
  "tool": "igris_save",
  "arguments": {
    "title": "Auth middleware choice",
    "content": "Chose JWT with RS256 for API auth. Considered session cookies but needed stateless verification for microservices.",
    "type": "decision",
    "project": "igris-api",
    "topic_key": "architecture/auth"
  }
}
```

### Search for relevant memories

```json
{
  "tool": "igris_search",
  "arguments": {
    "query": "authentication JWT",
    "project": "igris-api",
    "limit": 10
  }
}
```

### Load context at session start

```json
{
  "tool": "igris_context",
  "arguments": {
    "project": "igris-api",
    "limit": 20
  }
}
```

### Save with sensitive data protection

```json
{
  "tool": "igris_save",
  "arguments": {
    "title": "Database connection config",
    "content": "PostgreSQL on port 5432, password is <private>super-secret-pw</private>",
    "type": "config",
    "project": "igris-api"
  }
}
```

The stored content will contain `[REDACTED]` instead of the password.

## Observation Types

| Type | Use for |
|------|---------|
| `decision` | Architecture or design decisions with rationale |
| `architecture` | System structure, component relationships |
| `bugfix` | Bug descriptions, root causes, and fixes |
| `pattern` | Recurring code patterns or conventions |
| `config` | Configuration details and environment setup |
| `discovery` | New findings about the codebase or tools |
| `learning` | General learnings and insights |
| `manual` | Default type for uncategorized observations |

## Architecture

```
AI Agent (Claude Code, Cursor, etc.)
    | stdio (JSON-RPC / MCP)
    v
Igris Memory (single Rust binary, ~7 MB)
    |
    v
SQLite + FTS5 (~/.igris/memory.db)
```

### Database Schema

- **sessions** — tracks coding work periods (id, project, directory, timestamps, summary)
- **observations** — core memory storage with deduplication and versioning
- **observations_fts** — FTS5 virtual table, kept in sync via triggers

### Key Logic

1. **Privacy stripping:** `<private>value</private>` is replaced with `[REDACTED]` before persisting
2. **Deduplication:** SHA-256 hash of normalized content; identical saves within 15 minutes increment a counter instead of creating duplicates
3. **Topic-key upsert:** observations with the same `topic_key` + project + scope are updated in-place (revision count incremented)
4. **Soft delete:** `deleted_at` is set instead of removing rows; data is always recoverable

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `IGRIS_DATA_DIR` | `~/.igris` | Directory where `memory.db` is stored |
| `RUST_LOG` | `info` | Log level (trace, debug, info, warn, error) |

## Development

### Prerequisites

- Rust 1.82+ (edition 2024)

### Build

```bash
cargo build          # debug build
cargo build --release  # optimized release build
```

### Test

```bash
cargo test
```

13 tests covering:
- Privacy tag stripping (single, multiple, none)
- Content hashing (determinism, uniqueness)
- Save and retrieve round-trip
- Topic-key upsert behavior
- Soft-delete mechanics
- FTS5 search functionality
- Context loading with limits
- Session lifecycle (start, end, summary)
- Statistics aggregation

### Project Structure

```
src/
  main.rs      — Entry point, data directory setup, MCP stdio transport
  server.rs    — MCP tool definitions and handlers (10 tools)
  db.rs        — Database operations (CRUD, search, sessions, dedup)
  schema.rs    — SQLite DDL, FTS5 triggers, indices, pragmas
  models.rs    — Data structures (Observation, Session, SearchResult, Stats)
  utils.rs     — Privacy stripping, SHA-256 hashing, UTC timestamps
```

## License

[Elastic License 2.0](LICENSE)
