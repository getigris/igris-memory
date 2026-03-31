# AGENTS.md

Instructions for AI agents working in this repository. **All context persistence MUST go through the Igris Memory MCP server.**

For architecture, module map, and build commands see [DEVELOPMENT.md](DEVELOPMENT.md).
For contribution guidelines see [CONTRIBUTING.md](CONTRIBUTING.md).

## Build & Test (quick reference)

```bash
cargo build --release          # Build release binary (igmem)
cargo test                     # Run all tests
cargo clippy -- -D warnings    # Lint (warnings = errors)
cargo fmt --check              # Check formatting
```

Pre-commit hooks (`.githooks/`) run fmt, clippy, and tests. Activate: `git config core.hooksPath .githooks`

## Igris Memory MCP — Centralized Context Persistence

Igris Memory (`igris-memory`) is the single source of truth for cross-session context. Every agent MUST follow this protocol to ensure continuity across sessions and providers.

### Session Lifecycle (MANDATORY)

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

Structure the summary as:

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

### Context Validation Protocol

**Before any significant action**, the agent MUST validate context:

1. **Before writing code**: `igris_search` for related decisions, architecture notes, and patterns in the area you're modifying.
2. **Before making a decision**: `igris_search` for prior decisions on the same topic. If one exists, acknowledge it and explain if changing course.
3. **Before creating a plan**: `igris_search(type: "plan")` to check for active plans. Update existing plans via `topic_key` instead of creating duplicates.
4. **Before fixing a bug**: `igris_search(type: "bugfix")` to check if previously encountered or related to a known fix.

### MCP Tools Reference

#### Context & Discovery

| Tool | When to Use | Parameters |
|------|-------------|------------|
| `igris_context` | **Session start**. Load recent memories chronologically. | `project?`, `limit?` (default 20, max 50) |
| `igris_search` | Find specific past decisions, patterns, or context by keyword. | `query` (required), `project?`, `type?`, `limit?` |
| `igris_get` | Get full content of a memory by ID. | `id` (required) |
| `igris_timeline` | Understand the sequence of events around a memory. | `observation_id` (required), `before?`, `after?` |
| `igris_stats` | Memory store overview: totals by type/project. | _(none)_ |

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

### Topic Keys

Use `topic_key` to keep knowledge consolidated. Same key **updates in place** instead of creating duplicates.

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

**Plans**: Save as `type: "plan"` with `topic_key: "plan/feature-name"`. Update as it evolves. Delete with `igris_delete` when complete.

### Privacy

Wrap sensitive values in `<private>...</private>` — auto-redacted to `[REDACTED]` before storage.

### Rules

- **Never skip `igris_context` at session start.**
- **Save immediately, not later** — don't batch or defer.
- **Use `topic_key` for anything that evolves** — architecture, configs, active plans.
- **Delete completed plans** — they clutter context.
- **Be detailed in content** — future sessions only know what you wrote. Include the *why*.
- **Use `project: "igris-memory"`** for all observations related to this repository.
- **Scope**: `project` (default) for repo-specific, `personal` for user preferences.
