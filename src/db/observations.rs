use crate::models::Observation;
use crate::utils::{hash_content, now_utc, strip_private_tags};
use crate::validation;
use rusqlite::params;

use super::{Database, DbResult, OptionalExt, DEDUPE_WINDOW_MINUTES};

impl Database {
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
}
