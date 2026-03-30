use crate::errors::IgrisError;
use crate::models::{ExportData, ImportResult, Observation, PurgeResult, SearchResult, Session, Stats, Timeline};
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

    // ─── Timeline ───────────────────────────────────────────────────

    /// Chronological view around an observation.
    /// Returns the anchor observation plus `before` entries before it
    /// and `after` entries after it, ordered by creation time.
    pub fn timeline(
        &self,
        observation_id: i64,
        before: Option<i64>,
        after: Option<i64>,
    ) -> DbResult<Timeline> {
        let before_limit = before.unwrap_or(5).min(50);
        let after_limit = after.unwrap_or(5).min(50);

        // Fetch the anchor observation
        let anchor = self.get_observation(observation_id)?;

        // Observations before the anchor, using (created_at, id) for stable ordering
        let sql_before = format!(
            "SELECT id, session_id, type, title, content, project, scope,
                    topic_key, tags, revision_count, duplicate_count,
                    created_at, updated_at, deleted_at
             FROM observations
             WHERE deleted_at IS NULL
               AND (datetime(created_at) < datetime(?1) OR (created_at = ?1 AND id < ?2))
             ORDER BY datetime(created_at) DESC, id DESC
             LIMIT {before_limit}"
        );
        let mut stmt = self.conn.prepare(&sql_before)?;
        let mut before_rows = Vec::new();
        let mut rows = stmt.query(params![anchor.created_at, observation_id])?;
        while let Some(row) = rows.next()? {
            before_rows.push(Self::row_to_observation(row));
        }
        // Reverse so oldest is first (chronological order)
        before_rows.reverse();

        // Observations after the anchor, using (created_at, id) for stable ordering
        let sql_after = format!(
            "SELECT id, session_id, type, title, content, project, scope,
                    topic_key, tags, revision_count, duplicate_count,
                    created_at, updated_at, deleted_at
             FROM observations
             WHERE deleted_at IS NULL
               AND (datetime(created_at) > datetime(?1) OR (created_at = ?1 AND id > ?2))
             ORDER BY datetime(created_at) ASC, id ASC
             LIMIT {after_limit}"
        );
        let mut stmt = self.conn.prepare(&sql_after)?;
        let mut after_rows = Vec::new();
        let mut rows = stmt.query(params![anchor.created_at, observation_id])?;
        while let Some(row) = rows.next()? {
            after_rows.push(Self::row_to_observation(row));
        }

        Ok(Timeline {
            anchor,
            before: before_rows,
            after: after_rows,
        })
    }

    // ─── Export / Import ─────────────────────────────────────────────

    /// Export all observations and sessions as a portable JSON structure.
    pub fn export_all(&self) -> DbResult<ExportData> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, type, title, content, project, scope,
                    topic_key, tags, revision_count, duplicate_count,
                    created_at, updated_at, deleted_at
             FROM observations ORDER BY id"
        )?;
        let mut observations = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            observations.push(Self::row_to_observation(row));
        }

        let mut stmt = self.conn.prepare(
            "SELECT id, project, directory, started_at, ended_at, summary
             FROM sessions ORDER BY started_at"
        )?;
        let mut sessions = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            sessions.push(Session {
                id: row.get(0)?,
                project: row.get(1)?,
                directory: row.get(2)?,
                started_at: row.get(3)?,
                ended_at: row.get(4)?,
                summary: row.get(5)?,
            });
        }

        Ok(ExportData {
            version: crate::schema::SCHEMA_VERSION,
            exported_at: now_utc(),
            observations,
            sessions,
        })
    }

    /// Import observations and sessions from an export, deduplicating by content hash.
    pub fn import_data(&self, data: &ExportData) -> DbResult<ImportResult> {
        let mut obs_imported: i64 = 0;
        let mut obs_skipped: i64 = 0;
        let mut sess_imported: i64 = 0;
        let mut sess_skipped: i64 = 0;

        // Import sessions first (observations may reference them)
        for session in &data.sessions {
            let exists: bool = self.conn.query_row(
                "SELECT COUNT(*) > 0 FROM sessions WHERE id = ?1",
                params![session.id],
                |row| row.get(0),
            )?;
            if exists {
                sess_skipped += 1;
                continue;
            }
            self.conn.execute(
                "INSERT INTO sessions (id, project, directory, started_at, ended_at, summary)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![session.id, session.project, session.directory, session.started_at, session.ended_at, session.summary],
            )?;
            sess_imported += 1;
        }

        // Import observations, dedup by normalized_hash
        for obs in &data.observations {
            // Skip soft-deleted observations
            if obs.deleted_at.is_some() {
                obs_skipped += 1;
                continue;
            }

            let content_hash = hash_content(&obs.content);
            let dup_exists: bool = self.conn.query_row(
                "SELECT COUNT(*) > 0 FROM observations
                 WHERE normalized_hash = ?1
                   AND IFNULL(project, '') = IFNULL(?2, '')
                   AND scope = ?3
                   AND type = ?4
                   AND deleted_at IS NULL",
                params![content_hash, obs.project, obs.scope, obs.observation_type],
                |row| row.get(0),
            )?;
            if dup_exists {
                obs_skipped += 1;
                continue;
            }

            let tags_json = obs.tags.as_ref().map(|t| serde_json::to_string(t).unwrap_or_default());
            self.conn.execute(
                "INSERT INTO observations
                 (session_id, type, title, content, project, scope, topic_key,
                  normalized_hash, tags, revision_count, duplicate_count,
                  created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    obs.session_id,
                    obs.observation_type,
                    obs.title,
                    obs.content,
                    obs.project,
                    obs.scope,
                    obs.topic_key,
                    content_hash,
                    tags_json,
                    obs.revision_count,
                    obs.duplicate_count,
                    obs.created_at,
                    obs.updated_at,
                ],
            )?;
            obs_imported += 1;
        }

        Ok(ImportResult {
            observations_imported: obs_imported,
            observations_skipped: obs_skipped,
            sessions_imported: sess_imported,
            sessions_skipped: sess_skipped,
        })
    }

    // ─── Purge ──────────────────────────────────────────────────────

    /// Permanently delete observations that were soft-deleted more than
    /// `older_than_days` days ago. Runs VACUUM afterwards to reclaim space.
    pub fn purge(&self, older_than_days: i64) -> DbResult<PurgeResult> {
        if older_than_days < 0 {
            return Err(IgrisError::validation(format!(
                "older_than_days must be >= 0, got {older_than_days}"
            )));
        }

        let affected = self.conn.execute(
            "DELETE FROM observations
             WHERE deleted_at IS NOT NULL
               AND datetime(deleted_at) <= datetime('now', ?1)",
            params![format!("-{older_than_days} days")],
        )?;

        // Reclaim disk space
        self.conn.execute_batch("VACUUM")?;

        Ok(PurgeResult {
            observations_purged: affected as i64,
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

    // ─── Timeline tests ──────────────────────────────────────────────

    #[test]
    fn timeline_returns_anchor() {
        let db = test_db();
        let obs = db.save_observation("A", "content a", "decision", Some("p"), "project", None, None, None).unwrap();
        let tl = db.timeline(obs.id, None, None).unwrap();
        assert_eq!(tl.anchor.id, obs.id);
    }

    #[test]
    fn timeline_returns_before_and_after() {
        let db = test_db();
        let o1 = db.save_observation("First", "c1", "manual", Some("p"), "project", None, None, None).unwrap();
        let o2 = db.save_observation("Second", "c2", "manual", Some("p"), "project", None, None, None).unwrap();
        let o3 = db.save_observation("Third", "c3", "manual", Some("p"), "project", None, None, None).unwrap();

        let tl = db.timeline(o2.id, Some(5), Some(5)).unwrap();
        assert_eq!(tl.anchor.id, o2.id);
        assert!(tl.before.iter().any(|o| o.id == o1.id), "should include o1 before anchor");
        assert!(tl.after.iter().any(|o| o.id == o3.id), "should include o3 after anchor");
    }

    #[test]
    fn timeline_respects_limits() {
        let db = test_db();
        for i in 0..10 {
            db.save_observation(&format!("Obs {i}"), &format!("content {i}"), "manual", Some("p"), "project", None, None, None).unwrap();
        }
        // anchor is obs 5 (id=6 since autoincrement starts at 1)
        let mid = db.save_observation("Middle", "mid", "manual", Some("p"), "project", None, None, None).unwrap();
        for i in 11..20 {
            db.save_observation(&format!("Obs {i}"), &format!("content {i}"), "manual", Some("p"), "project", None, None, None).unwrap();
        }

        let tl = db.timeline(mid.id, Some(3), Some(2)).unwrap();
        assert_eq!(tl.before.len(), 3);
        assert_eq!(tl.after.len(), 2);
    }

    #[test]
    fn timeline_excludes_deleted() {
        let db = test_db();
        let o1 = db.save_observation("A", "a", "manual", Some("p"), "project", None, None, None).unwrap();
        let o2 = db.save_observation("B", "b", "manual", Some("p"), "project", None, None, None).unwrap();
        let o3 = db.save_observation("C", "c", "manual", Some("p"), "project", None, None, None).unwrap();
        db.delete_observation(o1.id).unwrap();

        let tl = db.timeline(o2.id, Some(5), Some(5)).unwrap();
        assert!(tl.before.is_empty(), "deleted obs should not appear");
        assert_eq!(tl.after.len(), 1);
    }

    #[test]
    fn timeline_nonexistent_id_errors() {
        let db = test_db();
        let err = db.timeline(9999, None, None);
        assert!(err.is_err());
    }

    #[test]
    fn timeline_defaults_without_limits() {
        let db = test_db();
        for i in 0..10 {
            db.save_observation(&format!("Obs {i}"), &format!("c{i}"), "manual", None, "project", None, None, None).unwrap();
        }
        // Default should be 5 before / 5 after
        let tl = db.timeline(5, None, None).unwrap();
        assert!(tl.before.len() <= 5);
        assert!(tl.after.len() <= 5);
    }

    // ─── Export / Import tests ────────────────────────────────────────

    #[test]
    fn export_includes_all_data() {
        let db = test_db();
        db.save_observation("A", "a", "decision", Some("p1"), "project", None, None, None).unwrap();
        db.save_observation("B", "b", "bugfix", Some("p1"), "project", None, None, None).unwrap();
        db.start_session("s1", "p1", None).unwrap();

        let data = db.export_all().unwrap();
        assert_eq!(data.observations.len(), 2);
        assert_eq!(data.sessions.len(), 1);
        assert_eq!(data.version, 1);
        assert!(!data.exported_at.is_empty());
    }

    #[test]
    fn export_includes_deleted() {
        let db = test_db();
        let obs = db.save_observation("A", "a", "manual", None, "project", None, None, None).unwrap();
        db.delete_observation(obs.id).unwrap();

        let data = db.export_all().unwrap();
        assert_eq!(data.observations.len(), 1, "export should include soft-deleted");
    }

    #[test]
    fn import_into_empty_db() {
        let db1 = test_db();
        db1.save_observation("A", "a", "decision", Some("p"), "project", None, None, None).unwrap();
        db1.start_session("s1", "p", Some("/code")).unwrap();
        let data = db1.export_all().unwrap();

        let db2 = test_db();
        let result = db2.import_data(&data).unwrap();
        assert_eq!(result.observations_imported, 1);
        assert_eq!(result.sessions_imported, 1);

        let stats = db2.stats().unwrap();
        assert_eq!(stats.total_observations, 1);
        assert_eq!(stats.total_sessions, 1);
    }

    #[test]
    fn import_deduplicates_by_hash() {
        let db = test_db();
        db.save_observation("A", "same content", "decision", Some("p"), "project", None, None, None).unwrap();
        let data = db.export_all().unwrap();

        // Import into the same DB — should skip the duplicate
        let result = db.import_data(&data).unwrap();
        assert_eq!(result.observations_imported, 0);
        assert_eq!(result.observations_skipped, 1);
    }

    #[test]
    fn import_deduplicates_sessions_by_id() {
        let db = test_db();
        db.start_session("s1", "p", None).unwrap();
        let data = db.export_all().unwrap();

        let result = db.import_data(&data).unwrap();
        assert_eq!(result.sessions_imported, 0);
        assert_eq!(result.sessions_skipped, 1);
    }

    #[test]
    fn import_roundtrip_preserves_data() {
        let db1 = test_db();
        db1.save_observation("Title", "Content here", "architecture", Some("proj"), "project", Some("arch/auth"), Some(&["rust".to_string(), "auth".to_string()]), None).unwrap();
        let data = db1.export_all().unwrap();

        let db2 = test_db();
        db2.import_data(&data).unwrap();

        let ctx = db2.recent_context(Some("proj"), Some(1)).unwrap();
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0].title, "Title");
        assert_eq!(ctx[0].content, "Content here");
        assert_eq!(ctx[0].observation_type, "architecture");
        assert_eq!(ctx[0].topic_key.as_deref(), Some("arch/auth"));
        assert_eq!(ctx[0].tags.as_ref().unwrap().len(), 2);
    }

    // ─── Purge tests ─────────────────────────────────────────────────

    #[test]
    fn purge_removes_old_deleted() {
        let db = test_db();
        let obs = db.save_observation("Old", "old content", "manual", None, "project", None, None, None).unwrap();
        db.delete_observation(obs.id).unwrap();

        // Backdate the deleted_at to 60 days ago
        db.conn.execute(
            "UPDATE observations SET deleted_at = datetime('now', '-60 days') WHERE id = ?1",
            params![obs.id],
        ).unwrap();

        let result = db.purge(30).unwrap();
        assert_eq!(result.observations_purged, 1);

        // Verify it's truly gone
        let err = db.get_observation(obs.id);
        assert!(err.is_err(), "purged observation should not exist");
    }

    #[test]
    fn purge_keeps_recently_deleted() {
        let db = test_db();
        let obs = db.save_observation("Recent", "recent content", "manual", None, "project", None, None, None).unwrap();
        db.delete_observation(obs.id).unwrap();
        // deleted_at is now() — should NOT be purged with 30 day threshold

        let result = db.purge(30).unwrap();
        assert_eq!(result.observations_purged, 0);

        // Still accessible
        let fetched = db.get_observation(obs.id).unwrap();
        assert!(fetched.deleted_at.is_some());
    }

    #[test]
    fn purge_keeps_non_deleted() {
        let db = test_db();
        db.save_observation("Active", "active content", "manual", None, "project", None, None, None).unwrap();

        let result = db.purge(0).unwrap();
        assert_eq!(result.observations_purged, 0);
    }

    #[test]
    fn purge_with_zero_days_purges_all_deleted() {
        let db = test_db();
        let obs = db.save_observation("A", "a", "manual", None, "project", None, None, None).unwrap();
        db.delete_observation(obs.id).unwrap();

        let result = db.purge(0).unwrap();
        assert_eq!(result.observations_purged, 1);
    }

    #[test]
    fn purge_rejects_negative_days() {
        let db = test_db();
        let err = db.purge(-1);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code, crate::errors::ErrorCode::ValidationError);
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
