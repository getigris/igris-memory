/// Current schema version. Increment when adding migrations.
pub const SCHEMA_VERSION: u32 = 5;

/// Initial database schema — tables, FTS5, triggers, and indices.
pub const SCHEMA_V1: &str = r#"
-- Sessions: tracks coding work periods
CREATE TABLE IF NOT EXISTS sessions (
    id         TEXT PRIMARY KEY,
    project    TEXT NOT NULL,
    directory  TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at   TEXT,
    summary    TEXT
);

-- Observations: the core memory unit
CREATE TABLE IF NOT EXISTS observations (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      TEXT,
    type            TEXT NOT NULL DEFAULT 'manual',
    title           TEXT NOT NULL,
    content         TEXT NOT NULL,
    project         TEXT,
    scope           TEXT NOT NULL DEFAULT 'project',
    topic_key       TEXT,
    normalized_hash TEXT,
    revision_count  INTEGER NOT NULL DEFAULT 1,
    duplicate_count INTEGER NOT NULL DEFAULT 1,
    tags            TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at      TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

-- Full-text search index (content-synced via triggers)
CREATE VIRTUAL TABLE IF NOT EXISTS observations_fts USING fts5(
    title, content, type, project, topic_key,
    content='observations',
    content_rowid='id'
);

-- Keep FTS5 in sync with observations table
CREATE TRIGGER IF NOT EXISTS obs_fts_insert AFTER INSERT ON observations BEGIN
    INSERT INTO observations_fts(rowid, title, content, type, project, topic_key)
    VALUES (new.id, new.title, new.content, new.type, new.project, new.topic_key);
END;

CREATE TRIGGER IF NOT EXISTS obs_fts_delete AFTER DELETE ON observations BEGIN
    INSERT INTO observations_fts(observations_fts, rowid, title, content, type, project, topic_key)
    VALUES ('delete', old.id, old.title, old.content, old.type, old.project, old.topic_key);
END;

CREATE TRIGGER IF NOT EXISTS obs_fts_update AFTER UPDATE ON observations BEGIN
    INSERT INTO observations_fts(observations_fts, rowid, title, content, type, project, topic_key)
    VALUES ('delete', old.id, old.title, old.content, old.type, old.project, old.topic_key);
    INSERT INTO observations_fts(rowid, title, content, type, project, topic_key)
    VALUES (new.id, new.title, new.content, new.type, new.project, new.topic_key);
END;

-- Indices for common query patterns
CREATE INDEX IF NOT EXISTS idx_obs_project    ON observations(project);
CREATE INDEX IF NOT EXISTS idx_obs_type       ON observations(type);
CREATE INDEX IF NOT EXISTS idx_obs_scope      ON observations(scope);
CREATE INDEX IF NOT EXISTS idx_obs_created    ON observations(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_obs_deleted    ON observations(deleted_at);
CREATE INDEX IF NOT EXISTS idx_obs_topic      ON observations(topic_key, project, scope, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_obs_dedupe     ON observations(normalized_hash, project, scope, type);
CREATE INDEX IF NOT EXISTS idx_sessions_proj  ON sessions(project);
"#;

/// Schema v2 — entity graph layer (additive; observations/sessions untouched).
pub const SCHEMA_V2: &str = r#"
-- Entities: first-class nodes (person, company, project, concept, ...)
CREATE TABLE IF NOT EXISTS entities (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    kind           TEXT NOT NULL,
    canonical_name TEXT NOT NULL,
    slug           TEXT NOT NULL,
    tier           INTEGER NOT NULL DEFAULT 3,
    salience       REAL NOT NULL DEFAULT 0.0,
    compiled_truth TEXT,
    compiled_at    TEXT,
    project        TEXT,
    scope          TEXT NOT NULL DEFAULT 'project',
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at     TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_entity_slug
    ON entities(slug, IFNULL(project, ''), scope) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_entity_kind    ON entities(kind);
CREATE INDEX IF NOT EXISTS idx_entity_deleted ON entities(deleted_at);

-- Aliases: normalized strings that resolve to an entity (deterministic wiring)
CREATE TABLE IF NOT EXISTS entity_aliases (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_id        INTEGER NOT NULL,
    alias_normalized TEXT NOT NULL,
    source           TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_alias_unique
    ON entity_aliases(alias_normalized, entity_id);
CREATE INDEX IF NOT EXISTS idx_alias_norm ON entity_aliases(alias_normalized);

-- Edges: typed relations between entities (wired in Fase 0b)
CREATE TABLE IF NOT EXISTS edges (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    src_entity_id  INTEGER NOT NULL,
    dst_entity_id  INTEGER NOT NULL,
    edge_type      TEXT NOT NULL,
    evidence_count INTEGER NOT NULL DEFAULT 1,
    confidence     REAL NOT NULL DEFAULT 1.0,
    first_seen     TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen      TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at     TEXT,
    FOREIGN KEY (src_entity_id) REFERENCES entities(id) ON DELETE CASCADE,
    FOREIGN KEY (dst_entity_id) REFERENCES entities(id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_edge_unique
    ON edges(src_entity_id, dst_entity_id, edge_type);

-- Mentions: bridge observation -> entity (wired in Fase 0b)
CREATE TABLE IF NOT EXISTS mentions (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    observation_id INTEGER NOT NULL,
    entity_id      INTEGER NOT NULL,
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (observation_id) REFERENCES observations(id) ON DELETE CASCADE,
    FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_mention_unique
    ON mentions(observation_id, entity_id);
CREATE INDEX IF NOT EXISTS idx_mention_entity ON mentions(entity_id);
"#;

/// Schema v3 — vector embeddings for hybrid retrieval (additive).
pub const SCHEMA_V3: &str = r#"
CREATE TABLE IF NOT EXISTS embeddings (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    object_type TEXT NOT NULL,
    object_id   INTEGER NOT NULL,
    model       TEXT NOT NULL,
    dim         INTEGER NOT NULL,
    vector      BLOB NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_embeddings_obj
    ON embeddings(object_type, object_id, model);
CREATE INDEX IF NOT EXISTS idx_embeddings_model ON embeddings(model);
"#;

/// Schema v4 — vector-index metadata (the vec0 table itself is created lazily).
pub const SCHEMA_V4: &str = r#"
CREATE TABLE IF NOT EXISTS vec_index_meta (
    id    INTEGER PRIMARY KEY CHECK (id = 1),
    dim   INTEGER NOT NULL,
    model TEXT NOT NULL
);
"#;

/// Schema v5 — code graph: files/symbols/edges extracted from source via
/// tree-sitter. A derived cache (rebuildable from source), not covered by
/// igris_export/igris_import.
pub const SCHEMA_V5: &str = r#"
CREATE TABLE IF NOT EXISTS code_files (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    project       TEXT NOT NULL,
    root_path     TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    language      TEXT NOT NULL,
    content_hash  TEXT NOT NULL,
    indexed_at    TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at    TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_code_file_path
    ON code_files(project, root_path, relative_path);
CREATE INDEX IF NOT EXISTS idx_code_file_deleted ON code_files(deleted_at);

CREATE TABLE IF NOT EXISTS code_symbols (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id        INTEGER NOT NULL,
    kind           TEXT NOT NULL,
    name           TEXT NOT NULL,
    qualified_name TEXT NOT NULL,
    start_line     INTEGER NOT NULL,
    end_line       INTEGER NOT NULL,
    indexed_at     TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at     TEXT,
    FOREIGN KEY (file_id) REFERENCES code_files(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_code_symbol_file ON code_symbols(file_id);
CREATE INDEX IF NOT EXISTS idx_code_symbol_name ON code_symbols(name);
CREATE INDEX IF NOT EXISTS idx_code_symbol_qualified ON code_symbols(qualified_name);
CREATE INDEX IF NOT EXISTS idx_code_symbol_deleted ON code_symbols(deleted_at);

CREATE TABLE IF NOT EXISTS code_edges (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    src_id            INTEGER NOT NULL,
    src_type          TEXT NOT NULL,
    dst_id            INTEGER,
    dst_type          TEXT,
    dst_name          TEXT NOT NULL,
    relation          TEXT NOT NULL,
    resolution        TEXT NOT NULL,
    external_boundary INTEGER NOT NULL DEFAULT 0,
    evidence_count    INTEGER NOT NULL DEFAULT 1,
    first_seen        TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen         TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_code_edge_src ON code_edges(src_type, src_id);
CREATE INDEX IF NOT EXISTS idx_code_edge_dst ON code_edges(dst_type, dst_id);
"#;

/// Pragmas applied on every connection open.
pub const PRAGMAS: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA busy_timeout = 5000;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
"#;
