use crate::errors::IgrisError;
use crate::models::{Observation, SearchResult, Session, Stats};
use crate::schema::{PRAGMAS, SCHEMA_V1, SCHEMA_VERSION};
use crate::utils::{hash_content, now_utc, strip_private_tags};
use crate::validation;
use rusqlite::{params, Connection, Result as SqlResult};
use std::collections::HashMap;
use std::path::Path;

/// Result type for Database operations.
pub type DbResult<T> = Result<T, IgrisError>;

/// Deduplication window in minutes — saves with identical content
/// within this window are counted as duplicates instead of new entries.
const DEDUPE_WINDOW_MINUTES: i64 = 15;

/// Maximum observations returned by default queries.
const DEFAULT_LIMIT: i64 = 20;

#[derive(Debug)]
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (or create) the database at the given path and run migrations.
    pub fn open(path: &Path) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    /// Open an in-memory database (for tests).
    #[cfg(test)]
    pub fn open_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> SqlResult<()> {
        self.conn.execute_batch(PRAGMAS)?;
        let version: u32 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < SCHEMA_VERSION {
            self.conn.execute_batch(SCHEMA_V1)?;
            self.conn
                .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        }
        Ok(())
    }

    // ─── Observations (memories) ────────────────────────────────────

    /// Save a new observation with deduplication and topic-key upsert logic.
    #[allow(clippy::too_many_arguments)]
    pub fn save_observation(
        &self,
        title: &str,
        content: &str,
        obs_type: &str,
        project: Option<&str>,
        scope: &str,
        topic_key: Option<&str>,
        tags: Option<&[String]>,
        session_id: Option<&str>,
    ) -> DbResult<Observation> {
        validation::validate_save(title, content, obs_type, scope)?;
        let clean_content = strip_private_tags(content);
        let clean_title = strip_private_tags(title);
        let content_hash = hash_content(&clean_content);
        let tags_json = tags.map(|t| serde_json::to_string(t).unwrap_or_default());
        let now = now_utc();

        // 1) Topic-key upsert: if a topic_key is given, update existing entry
        if let Some(tk) = topic_key {
            let existing: Option<i64> = self
                .conn
                .query_row(
                    "SELECT id FROM observations
                     WHERE topic_key = ?1
                       AND IFNULL(project, '') = IFNULL(?2, '')
                       AND scope = ?3
                       AND deleted_at IS NULL
                     ORDER BY datetime(updated_at) DESC
                     LIMIT 1",
                    params![tk, project, scope],
                    |row| row.get(0),
                )
                .optional()?;

            if let Some(existing_id) = existing {
                self.conn.execute(
                    "UPDATE observations
                     SET title = ?1, content = ?2, type = ?3,
                         normalized_hash = ?4, tags = ?5,
                         revision_count = revision_count + 1,
                         updated_at = ?6
                     WHERE id = ?7",
                    params![
                        clean_title,
                        clean_content,
                        obs_type,
                        content_hash,
                        tags_json,
                        now,
                        existing_id
                    ],
                )?;
                return self.get_observation(existing_id);
            }
        }

        // 2) Deduplication: same hash within the time window → increment counter
        let dup_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM observations
                 WHERE normalized_hash = ?1
                   AND IFNULL(project, '') = IFNULL(?2, '')
                   AND scope = ?3
                   AND type = ?4
                   AND deleted_at IS NULL
                   AND datetime(created_at) >= datetime('now', ?5)
                 LIMIT 1",
                params![
                    content_hash,
                    project,
                    scope,
                    obs_type,
                    format!("-{DEDUPE_WINDOW_MINUTES} minutes")
                ],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(dup_id) = dup_id {
            self.conn.execute(
                "UPDATE observations
                 SET duplicate_count = duplicate_count + 1,
                     updated_at = ?1
                 WHERE id = ?2",
                params![now, dup_id],
            )?;
            return self.get_observation(dup_id);
        }

        // 3) Insert new observation
        self.conn.execute(
            "INSERT INTO observations
             (session_id, type, title, content, project, scope, topic_key,
              normalized_hash, tags, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                session_id,
                obs_type,
                clean_title,
                clean_content,
                project,
                scope,
                topic_key,
                content_hash,
                tags_json,
                now,
                now
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get_observation(id)
    }

    /// Get a single observation by ID.
    pub fn get_observation(&self, id: i64) -> DbResult<Observation> {
        Ok(self.conn.query_row(
            "SELECT id, session_id, type, title, content, project, scope,
                    topic_key, tags, revision_count, duplicate_count,
                    created_at, updated_at, deleted_at
             FROM observations WHERE id = ?1",
            params![id],
            |row| Ok(Self::row_to_observation(row)),
        )?)
    }

    /// Update an observation partially — only provided fields are changed.
    pub fn update_observation(
        &self,
        id: i64,
        title: Option<&str>,
        content: Option<&str>,
        obs_type: Option<&str>,
        tags: Option<&[String]>,
        topic_key: Option<&str>,
    ) -> DbResult<Observation> {
        validation::validate_update_has_fields(title, content, obs_type, tags, topic_key)?;
        if let Some(t) = obs_type {
            validation::validate_observation_type(t)?;
        }
        let now = now_utc();

        if let Some(t) = title {
            let clean = strip_private_tags(t);
            self.conn.execute(
                "UPDATE observations SET title = ?1, updated_at = ?2 WHERE id = ?3",
                params![clean, now, id],
            )?;
        }
        if let Some(c) = content {
            let clean = strip_private_tags(c);
            let hash = hash_content(&clean);
            self.conn.execute(
                "UPDATE observations SET content = ?1, normalized_hash = ?2, updated_at = ?3 WHERE id = ?4",
                params![clean, hash, now, id],
            )?;
        }
        if let Some(t) = obs_type {
            self.conn.execute(
                "UPDATE observations SET type = ?1, updated_at = ?2 WHERE id = ?3",
                params![t, now, id],
            )?;
        }
        if let Some(t) = tags {
            let json = serde_json::to_string(t).unwrap_or_default();
            self.conn.execute(
                "UPDATE observations SET tags = ?1, updated_at = ?2 WHERE id = ?3",
                params![json, now, id],
            )?;
        }
        if let Some(tk) = topic_key {
            self.conn.execute(
                "UPDATE observations SET topic_key = ?1, updated_at = ?2 WHERE id = ?3",
                params![tk, now, id],
            )?;
        }
        self.get_observation(id)
    }

    /// Soft-delete an observation (sets deleted_at, keeps data).
    pub fn delete_observation(&self, id: i64) -> DbResult<bool> {
        let affected = self.conn.execute(
            "UPDATE observations SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now_utc(), id],
        )?;
        Ok(affected > 0)
    }

    /// Full-text search across observations using FTS5.
    pub fn search(
        &self,
        query: &str,
        obs_type: Option<&str>,
        project: Option<&str>,
        limit: Option<i64>,
    ) -> DbResult<Vec<SearchResult>> {
        validation::validate_search_query(query)?;
        validation::validate_limit(limit)?;
        if let Some(t) = obs_type {
            validation::validate_observation_type(t)?;
        }
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(50);

        // Wrap each word in quotes for safe FTS5 matching
        let fts_query: String = query
            .split_whitespace()
            .map(|w| format!("\"{}\"", w.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" ");

        let sql = r#"
            SELECT o.id, o.session_id, o.type, o.title, o.content, o.project,
                   o.scope, o.topic_key, o.tags, o.revision_count, o.duplicate_count,
                   o.created_at, o.updated_at, o.deleted_at,
                   fts.rank,
                   snippet(observations_fts, 1, '→', '←', '...', 32) AS snippet
            FROM observations_fts fts
            JOIN observations o ON o.id = fts.rowid
            WHERE observations_fts MATCH ?1
              AND o.deleted_at IS NULL
        "#;

        let mut conditions = String::new();
        if obs_type.is_some() {
            conditions.push_str(" AND o.type = ?2");
        }
        if project.is_some() {
            conditions.push_str(if obs_type.is_some() {
                " AND o.project = ?3"
            } else {
                " AND o.project = ?2"
            });
        }

        let full_sql = format!(
            "{sql}{conditions} ORDER BY fts.rank LIMIT {limit}"
        );

        let mut stmt = self.conn.prepare(&full_sql)?;

        // Bind parameters dynamically based on which filters are active
        let rows: Vec<SearchResult> = match (obs_type, project) {
            (Some(t), Some(p)) => {
                let iter = stmt.query_map(params![fts_query, t, p], |row| {
                    Ok(SearchResult {
                        observation: Self::row_to_observation(row),
                        rank: row.get(14)?,
                        snippet: row.get(15)?,
                    })
                })?;
                iter.collect::<SqlResult<Vec<_>>>()?
            }
            (Some(t), None) => {
                let iter = stmt.query_map(params![fts_query, t], |row| {
                    Ok(SearchResult {
                        observation: Self::row_to_observation(row),
                        rank: row.get(14)?,
                        snippet: row.get(15)?,
                    })
                })?;
                iter.collect::<SqlResult<Vec<_>>>()?
            }
            (None, Some(p)) => {
                let iter = stmt.query_map(params![fts_query, p], |row| {
                    Ok(SearchResult {
                        observation: Self::row_to_observation(row),
                        rank: row.get(14)?,
                        snippet: row.get(15)?,
                    })
                })?;
                iter.collect::<SqlResult<Vec<_>>>()?
            }
            (None, None) => {
                let iter = stmt.query_map(params![fts_query], |row| {
                    Ok(SearchResult {
                        observation: Self::row_to_observation(row),
                        rank: row.get(14)?,
                        snippet: row.get(15)?,
                    })
                })?;
                iter.collect::<SqlResult<Vec<_>>>()?
            }
        };

        Ok(rows)
    }

    /// Recent observations for context loading at session start.
    pub fn recent_context(
        &self,
        project: Option<&str>,
        limit: Option<i64>,
    ) -> DbResult<Vec<Observation>> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(50);

        let (sql, bind_val) = if let Some(p) = project {
            (
                format!(
                    "SELECT id, session_id, type, title, content, project, scope,
                            topic_key, tags, revision_count, duplicate_count,
                            created_at, updated_at, deleted_at
                     FROM observations
                     WHERE deleted_at IS NULL AND project = ?1
                     ORDER BY datetime(updated_at) DESC
                     LIMIT {limit}"
                ),
                Some(p.to_string()),
            )
        } else {
            (
                format!(
                    "SELECT id, session_id, type, title, content, project, scope,
                            topic_key, tags, revision_count, duplicate_count,
                            created_at, updated_at, deleted_at
                     FROM observations
                     WHERE deleted_at IS NULL
                     ORDER BY datetime(updated_at) DESC
                     LIMIT {limit}"
                ),
                None,
            )
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let mut results = Vec::new();

        let mut rows = if let Some(ref p) = bind_val {
            stmt.query(params![p])?
        } else {
            stmt.query([])?
        };

        while let Some(row) = rows.next()? {
            results.push(Self::row_to_observation(row));
        }

        Ok(results)
    }

    /// Aggregate statistics about the memory store.
    pub fn stats(&self) -> DbResult<Stats> {
        let total_observations: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM observations WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )?;

        let total_sessions: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;

        let active_sessions: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE ended_at IS NULL",
            [],
            |r| r.get(0),
        )?;

        let mut by_type = HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT type, COUNT(*) FROM observations WHERE deleted_at IS NULL GROUP BY type",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (t, c) = row?;
            by_type.insert(t, c);
        }

        let mut by_project = HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT IFNULL(project, 'unset'), COUNT(*) FROM observations WHERE deleted_at IS NULL GROUP BY project",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (p, c) = row?;
            by_project.insert(p, c);
        }

        Ok(Stats {
            total_observations,
            total_sessions,
            active_sessions,
            by_type,
            by_project,
        })
    }

    // ─── Sessions ───────────────────────────────────────────────────

    /// Register a new session start.
    pub fn start_session(
        &self,
        id: &str,
        project: &str,
        directory: Option<&str>,
    ) -> DbResult<Session> {
        validation::validate_session(id, project)?;
        let now = now_utc();
        self.conn.execute(
            "INSERT INTO sessions (id, project, directory, started_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, project, directory, now],
        )?;
        self.get_session(id)
    }

    /// Mark a session as ended.
    pub fn end_session(&self, id: &str, summary: Option<&str>) -> DbResult<Session> {
        validation::require_non_empty(id, "session id")?;
        let now = now_utc();
        self.conn.execute(
            "UPDATE sessions SET ended_at = ?1, summary = COALESCE(?2, summary)
             WHERE id = ?3",
            params![now, summary, id],
        )?;
        self.get_session(id)
    }

    /// Save a session summary (can be called independently of end_session).
    pub fn save_session_summary(
        &self,
        content: &str,
        project: &str,
    ) -> DbResult<Session> {
        validation::require_non_empty(content, "content")?;
        validation::require_non_empty(project, "project")?;
        let clean = strip_private_tags(content);
        // Find the most recent active session for this project, or create one
        let session_id: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM sessions
                 WHERE project = ?1 AND ended_at IS NULL
                 ORDER BY datetime(started_at) DESC
                 LIMIT 1",
                params![project],
                |row| row.get(0),
            )
            .optional()?;

        match session_id {
            Some(id) => {
                self.conn.execute(
                    "UPDATE sessions SET summary = ?1 WHERE id = ?2",
                    params![clean, id],
                )?;
                self.get_session(&id)
            }
            None => {
                // No active session — create one with the summary
                let id = uuid::Uuid::new_v4().to_string();
                let now = now_utc();
                self.conn.execute(
                    "INSERT INTO sessions (id, project, started_at, ended_at, summary)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, project, now, now, clean],
                )?;
                self.get_session(&id)
            }
        }
    }

    fn get_session(&self, id: &str) -> DbResult<Session> {
        Ok(self.conn.query_row(
            "SELECT id, project, directory, started_at, ended_at, summary
             FROM sessions WHERE id = ?1",
            params![id],
            |row| {
                Ok(Session {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    directory: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    summary: row.get(5)?,
                })
            },
        )?)
    }

    // ─── Helpers ────────────────────────────────────────────────────

    fn row_to_observation(row: &rusqlite::Row) -> Observation {
        let tags_raw: Option<String> = row.get(8).unwrap_or(None);
        let tags: Option<Vec<String>> = tags_raw
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        Observation {
            id: row.get(0).unwrap_or_default(),
            session_id: row.get(1).unwrap_or(None),
            observation_type: row.get(2).unwrap_or_default(),
            title: row.get(3).unwrap_or_default(),
            content: row.get(4).unwrap_or_default(),
            project: row.get(5).unwrap_or(None),
            scope: row.get(6).unwrap_or_default(),
            topic_key: row.get(7).unwrap_or(None),
            tags,
            revision_count: row.get(9).unwrap_or(1),
            duplicate_count: row.get(10).unwrap_or(1),
            created_at: row.get(11).unwrap_or_default(),
            updated_at: row.get(12).unwrap_or_default(),
            deleted_at: row.get(13).unwrap_or(None),
        }
    }
}

// ─── Polyfill for optional queries ─────────────────────────────────

trait OptionalExt<T> {
    fn optional(self) -> SqlResult<Option<T>>;
}

impl<T> OptionalExt<T> for SqlResult<T> {
    fn optional(self) -> SqlResult<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        Database::open_in_memory().expect("failed to create in-memory db")
    }

    #[test]
    fn save_and_get() {
        let db = test_db();
        let obs = db
            .save_observation("Test title", "Test content", "decision", Some("myproj"), "project", None, None, None)
            .unwrap();
        assert_eq!(obs.title, "Test title");
        assert_eq!(obs.observation_type, "decision");

        let fetched = db.get_observation(obs.id).unwrap();
        assert_eq!(fetched.content, "Test content");
    }

    #[test]
    fn privacy_stripping() {
        let db = test_db();
        let obs = db
            .save_observation("Keys", "Use <private>sk-secret</private> for auth", "config", None, "project", None, None, None)
            .unwrap();
        assert_eq!(obs.content, "Use [REDACTED] for auth");
    }

    #[test]
    fn topic_key_upsert() {
        let db = test_db();
        let first = db
            .save_observation("Auth v1", "JWT tokens", "decision", Some("p"), "project", Some("arch/auth"), None, None)
            .unwrap();
        assert_eq!(first.revision_count, 1);

        let second = db
            .save_observation("Auth v2", "OAuth2 + PKCE", "decision", Some("p"), "project", Some("arch/auth"), None, None)
            .unwrap();
        assert_eq!(second.id, first.id, "should update same row");
        assert_eq!(second.revision_count, 2);
        assert_eq!(second.content, "OAuth2 + PKCE");
    }

    #[test]
    fn soft_delete() {
        let db = test_db();
        let obs = db
            .save_observation("Delete me", "content", "manual", None, "project", None, None, None)
            .unwrap();
        assert!(db.delete_observation(obs.id).unwrap());

        let deleted = db.get_observation(obs.id).unwrap();
        assert!(deleted.deleted_at.is_some());
    }

    #[test]
    fn search_finds_results() {
        let db = test_db();
        db.save_observation("Auth middleware", "JWT validation in Express", "decision", Some("web"), "project", None, None, None).unwrap();
        db.save_observation("Database setup", "PostgreSQL with Drizzle ORM", "config", Some("web"), "project", None, None, None).unwrap();

        let results = db.search("JWT middleware", None, None, None).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].observation.title.contains("Auth"));
    }

    #[test]
    fn context_returns_recent() {
        let db = test_db();
        for i in 0..5 {
            db.save_observation(&format!("Obs {i}"), &format!("Content {i}"), "manual", Some("proj"), "project", None, None, None).unwrap();
        }
        let ctx = db.recent_context(Some("proj"), Some(3)).unwrap();
        assert_eq!(ctx.len(), 3);
    }

    #[test]
    fn session_lifecycle() {
        let db = test_db();
        let s = db.start_session("s1", "myproj", Some("/code")).unwrap();
        assert!(s.ended_at.is_none());

        let s = db.end_session("s1", Some("Done with auth")).unwrap();
        assert!(s.ended_at.is_some());
        assert_eq!(s.summary.as_deref(), Some("Done with auth"));
    }

    #[test]
    fn stats_counts() {
        let db = test_db();
        db.save_observation("A", "a", "decision", Some("p1"), "project", None, None, None).unwrap();
        db.save_observation("B", "b", "bugfix", Some("p1"), "project", None, None, None).unwrap();
        db.save_observation("C", "c", "decision", Some("p2"), "project", None, None, None).unwrap();
        db.start_session("s1", "p1", None).unwrap();

        let stats = db.stats().unwrap();
        assert_eq!(stats.total_observations, 3);
        assert_eq!(stats.total_sessions, 1);
        assert_eq!(*stats.by_type.get("decision").unwrap(), 2);
        assert_eq!(*stats.by_project.get("p1").unwrap(), 2);
    }

    // ─── Validation integration tests ───────────────────────────────

    #[test]
    fn save_rejects_empty_title() {
        let db = test_db();
        let err = db.save_observation("", "content", "decision", None, "project", None, None, None);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code, crate::errors::ErrorCode::ValidationError);
    }

    #[test]
    fn save_rejects_invalid_type() {
        let db = test_db();
        let err = db.save_observation("Title", "content", "invalid", None, "project", None, None, None);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code, crate::errors::ErrorCode::ValidationError);
    }

    #[test]
    fn save_rejects_invalid_scope() {
        let db = test_db();
        let err = db.save_observation("Title", "content", "decision", None, "global", None, None, None);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code, crate::errors::ErrorCode::ValidationError);
    }

    #[test]
    fn search_rejects_empty_query() {
        let db = test_db();
        let err = db.search("", None, None, None);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code, crate::errors::ErrorCode::ValidationError);
    }

    #[test]
    fn search_rejects_zero_limit() {
        let db = test_db();
        let err = db.search("test", None, None, Some(0));
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code, crate::errors::ErrorCode::ValidationError);
    }

    #[test]
    fn update_rejects_empty_fields() {
        let db = test_db();
        let obs = db.save_observation("T", "C", "manual", None, "project", None, None, None).unwrap();
        let err = db.update_observation(obs.id, None, None, None, None, None);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code, crate::errors::ErrorCode::ValidationError);
    }

    #[test]
    fn update_rejects_invalid_type() {
        let db = test_db();
        let obs = db.save_observation("T", "C", "manual", None, "project", None, None, None).unwrap();
        let err = db.update_observation(obs.id, None, None, Some("nope"), None, None);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code, crate::errors::ErrorCode::ValidationError);
    }

    #[test]
    fn session_rejects_empty_id() {
        let db = test_db();
        let err = db.start_session("", "proj", None);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code, crate::errors::ErrorCode::ValidationError);
    }

    #[test]
    fn session_rejects_empty_project() {
        let db = test_db();
        let err = db.start_session("s1", "", None);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code, crate::errors::ErrorCode::ValidationError);
    }
}
