# Igris Memory

> Persistent memory for AI agents. One binary. Works across Claude, ChatGPT, Cursor, and any MCP-compatible tool.

[![License: Elastic-2.0](https://img.shields.io/badge/License-Elastic--2.0-blue.svg)](LICENSE)

---

## Why?

Every AI conversation starts from zero. Igris Memory fixes that by giving your AI assistant a persistent, searchable memory that works across sessions and providers.

- **No more repeating yourself** — decisions, patterns, and context survive between conversations
- **Provider-agnostic** — same memory for Claude Code, ChatGPT, Cursor, or any MCP client
- **Plans that clean up** — save execution plans, track progress, delete when done
- **Privacy-first** — wrap secrets in `<private>` tags, auto-redacted before storage

## Install

**Shell script** (Linux/macOS — auto-detects architecture):
```bash
curl -fsSL https://raw.githubusercontent.com/getigris/igris-memory/main/dist/install.sh | sh
```

**Homebrew** (macOS/Linux):
```bash
brew install getigris/tap/igris-memory
```

**From source**:
```bash
cargo install --path .
```

**Windows**: download `igris-memory-x86_64-pc-windows-msvc.zip` from [GitHub Releases](https://github.com/getigris/igris-memory/releases), extract `igmem.exe`, and add to your PATH.

The binary is called **`igmem`**.

### Configure with Claude Code

Add to `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "igris-memory": {
      "command": "igmem"
    }
  }
}
```

### Configure with Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "igris-memory": {
      "command": "/usr/local/bin/igmem"
    }
  }
}
```

## How It Works

```mermaid
graph TB
    subgraph S1["🟣 Session 1 — Claude Code"]
        A1["igris_context\nLoad what we did before"]
        A2["igris_save\ndecision: Use PostgreSQL"]
        A3["igris_session_summary\nChose PG, set up schema"]
    end

    subgraph S2["🔵 Session 2 — ChatGPT"]
        B1["igris_context\nLoad recent memories"]
        B2["igris_search\nWhat DB did we pick?"]
    end

    subgraph S3["🟢 Session 3 — Cursor"]
        C1["igris_context\nLoad everything"]
        C2["igris_search\nFind architecture decisions"]
    end

    DB[("🗄️ ~/.igris/memory.db\nSQLite + FTS5")]

    A1 <-->|read| DB
    A2 -->|write| DB
    A3 -->|write| DB
    B1 <-->|read| DB
    B2 <-->|search| DB
    C1 <-->|read| DB
    C2 <-->|search| DB

    style S1 fill:#7c3aed22,stroke:#7c3aed,stroke-width:2px,color:#7c3aed
    style S2 fill:#2563eb22,stroke:#2563eb,stroke-width:2px,color:#2563eb
    style S3 fill:#16a34a22,stroke:#16a34a,stroke-width:2px,color:#16a34a
    style DB fill:#f59e0b22,stroke:#f59e0b,stroke-width:3px,color:#f59e0b

    style A1 fill:#7c3aed33,stroke:#7c3aed,color:#fff
    style A2 fill:#7c3aed33,stroke:#7c3aed,color:#fff
    style A3 fill:#7c3aed33,stroke:#7c3aed,color:#fff
    style B1 fill:#2563eb33,stroke:#2563eb,color:#fff
    style B2 fill:#2563eb33,stroke:#2563eb,color:#fff
    style C1 fill:#16a34a33,stroke:#16a34a,color:#fff
    style C2 fill:#16a34a33,stroke:#16a34a,color:#fff

    linkStyle 0,1,2 stroke:#7c3aed,stroke-width:2px
    linkStyle 3,4 stroke:#2563eb,stroke-width:2px
    linkStyle 5,6 stroke:#16a34a,stroke-width:2px
```

## Session Lifecycle

```mermaid
graph LR
    START["🚀 START\nigris_session_start\nigris_context"] --> DURING["⚡ DURING\nigris_save · igris_search\nSave decisions, bugs, patterns"]
    DURING --> END_S["🏁 END\nigris_session_summary\nigris_session_end"]

    style START fill:#16a34a33,stroke:#16a34a,color:#fff,stroke-width:2px
    style DURING fill:#2563eb33,stroke:#2563eb,color:#fff,stroke-width:2px
    style END_S fill:#7c3aed33,stroke:#7c3aed,color:#fff,stroke-width:2px

    linkStyle 0 stroke:#2563eb,stroke-width:2px
    linkStyle 1 stroke:#7c3aed,stroke-width:2px
```

## MCP Tools (15)

### Memory Operations

| Tool | Description |
|------|-------------|
| `igris_save` | Save a memory. Called proactively when decisions are made, bugs are fixed, patterns emerge, or plans are created |
| `igris_search` | Search memories by keyword or natural language. Returns ranked results with snippets |
| `igris_get` | Get full content of a memory by ID |
| `igris_update` | Update specific fields of an existing memory |
| `igris_delete` | Soft-delete a memory (use for completed plans, outdated info) |
| `igris_context` | Load recent memories. Called at the START of every conversation |
| `igris_stats` | Memory store statistics by type and project |
| `igris_timeline` | Chronological context around a specific memory |
| `igris_suggest_topic_key` | Generate consistent keys for evolving knowledge |

### Data Operations

| Tool | Description |
|------|-------------|
| `igris_export` | Export all memories as JSON for backup |
| `igris_import` | Import memories with automatic deduplication |
| `igris_purge` | Permanently remove old soft-deleted memories |

### Session Management

| Tool | Description |
|------|-------------|
| `igris_session_start` | Register a new working session |
| `igris_session_end` | Mark session complete with summary |
| `igris_session_summary` | Save structured summary — most important memory for continuity |

## Memory Types

| Type | When to use | Example |
|------|------------|---------|
| `decision` | User makes a choice | "Use PostgreSQL over MySQL" |
| `architecture` | System design is created or changed | "Auth middleware uses JWT with RS256" |
| `bugfix` | A bug is found and fixed | "Fix null pointer in login handler" |
| `pattern` | A reusable pattern emerges | "Error handling: always wrap in Result<T, AppError>" |
| `config` | Configuration is set up or changed | "Redis cluster with 3 nodes on port 6379" |
| `discovery` | Something unexpected is learned | "SQLite FTS5 doesn't support prefix queries by default" |
| `learning` | A concept is explained or understood | "Rust lifetimes ensure references are valid" |
| `plan` | An execution plan is created | "1. Add axum 2. Create routes 3. Add tests" |
| `manual` | User explicitly asks to remember | "Remember: deploy to staging before prod" |

## Plans

Plans are a special memory type designed for execution tracking:

```mermaid
graph LR
    A["📝 Create plan\nigris_save\ntype: plan\ntopic_key: plan/feature"] --> B["🔄 Update progress\nigris_save\nsame topic_key\nrevision_count++"]
    B --> C["✅ Complete\nigris_delete\nsoft-delete"]
    C --> D["🧹 Clean up\nigris_purge\npermanent removal"]

    style A fill:#2563eb33,stroke:#2563eb,color:#fff,stroke-width:2px
    style B fill:#f59e0b33,stroke:#f59e0b,color:#fff,stroke-width:2px
    style C fill:#16a34a33,stroke:#16a34a,color:#fff,stroke-width:2px
    style D fill:#6b728033,stroke:#6b7280,color:#fff,stroke-width:2px

    linkStyle 0 stroke:#f59e0b,stroke-width:2px
    linkStyle 1 stroke:#16a34a,stroke-width:2px
    linkStyle 2 stroke:#6b7280,stroke-width:2px,stroke-dasharray:5
```

## Topic Keys

Topic keys group evolving knowledge. Saving with the same `topic_key` updates the existing memory instead of creating a duplicate:

```mermaid
graph LR
    V1["v1 · JWT tokens\narchitecture/auth\nrevision: 1"] -->|"igris_save\nsame topic_key"| V2["v2 · OAuth2 + PKCE\narchitecture/auth\nrevision: 2"]
    V2 -->|"igris_save\nsame topic_key"| V3["v3 · OAuth2 + PKCE + MFA\narchitecture/auth\nrevision: 3"]

    style V1 fill:#6b728033,stroke:#6b7280,color:#fff,stroke-width:1px,stroke-dasharray:5
    style V2 fill:#2563eb33,stroke:#2563eb,color:#fff,stroke-width:1px,stroke-dasharray:5
    style V3 fill:#16a34a33,stroke:#16a34a,color:#fff,stroke-width:2px

    linkStyle 0 stroke:#2563eb,stroke-width:2px
    linkStyle 1 stroke:#16a34a,stroke-width:2px
```

Use `igris_suggest_topic_key` to generate consistent keys automatically.

## Privacy

Wrap sensitive values in `<private>` tags — auto-redacted before storage:

```mermaid
graph LR
    IN["📥 Input\nAPI key is sk-abc123"] -->|"&lt;private&gt; tags\nauto-redact"| OUT["🔒 Stored\nAPI key is [REDACTED]"]

    style IN fill:#ef444433,stroke:#ef4444,color:#fff,stroke-width:2px
    style OUT fill:#16a34a33,stroke:#16a34a,color:#fff,stroke-width:2px

    linkStyle 0 stroke:#f59e0b,stroke-width:2px
```

## Running Modes

```bash
# MCP server (default) — for Claude Code, Cursor, etc.
igmem

# HTTP REST API — for any HTTP client
igmem serve --port 7437

# Terminal UI — interactive browser
igmem tui

# Sync — export/import for backup or multi-machine
igmem sync export --dir ./my-sync
igmem sync import --dir ./my-sync
```

## Options

```bash
# Custom data directory
igmem --data-dir /path/to/data

# Per-project isolated database
igmem --project-scoped --project my-app

# Encrypted database (SQLCipher)
igmem --db-key "my-secret-key"
# Or: IGRIS_DB_KEY=my-secret-key igmem

# Custom log level
IGRIS_LOG=debug igmem serve --port 7437
```

## Architecture

```mermaid
graph LR
    BIN["⚡ igmem\n~9 MB single binary"]

    MCP["🔌 MCP stdio\nClaude · Cursor · ChatGPT"]
    HTTP["🌐 HTTP REST API\nserve --port 7437"]
    TUI["🖥️ TUI\nInteractive browser"]
    SYNC["🔄 Sync\nexport / import"]

    BIN --> MCP
    BIN --> HTTP
    BIN --> TUI
    BIN --> SYNC

    subgraph storage["💾 Storage Layer"]
        DB1[("🌍 Global\n~/.igris/memory.db")]
        DB2[("📁 Per-project\n~/.igris/projects/{name}/memory.db")]
    end

    MCP --> storage
    HTTP --> storage
    TUI --> storage
    SYNC --> storage

    style BIN fill:#7c3aed,stroke:#7c3aed,color:#fff,stroke-width:2px
    style MCP fill:#2563eb33,stroke:#2563eb,color:#fff,stroke-width:2px
    style HTTP fill:#16a34a33,stroke:#16a34a,color:#fff,stroke-width:2px
    style TUI fill:#f59e0b33,stroke:#f59e0b,color:#fff,stroke-width:2px
    style SYNC fill:#ec489933,stroke:#ec4899,color:#fff,stroke-width:2px
    style storage fill:#f59e0b11,stroke:#f59e0b,stroke-width:2px,color:#f59e0b
    style DB1 fill:#f59e0b33,stroke:#f59e0b,color:#fff
    style DB2 fill:#f59e0b33,stroke:#f59e0b,color:#fff

    linkStyle 0 stroke:#2563eb,stroke-width:2px
    linkStyle 1 stroke:#16a34a,stroke-width:2px
    linkStyle 2 stroke:#f59e0b,stroke-width:2px
    linkStyle 3 stroke:#ec4899,stroke-width:2px
    linkStyle 4,5,6,7 stroke:#f59e0b,stroke-width:2px,stroke-dasharray:5
```

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for full architecture, module map, design patterns, cross-compilation, and release process.

```bash
rustup install stable                  # Rust 1.94+
git config core.hooksPath .githooks    # Activate pre-commit hooks
cargo build --release                  # Build
cargo test                             # Test
cargo clippy -- -D warnings            # Lint
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## License

[Elastic License 2.0](LICENSE)
