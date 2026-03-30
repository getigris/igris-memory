/// Current schema version. Increment when adding migrations.
pub const SCHEMA_VERSION: u32 = 1;

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

/// Pragmas applied on every connection open.
pub const PRAGMAS: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA busy_timeout = 5000;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
"#;
