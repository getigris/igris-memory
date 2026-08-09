use crate::models::Observation;
use crate::utils::{hash_content, now_utc, strip_private_tags};
use crate::validation;
use rusqlite::params;

use super::{DEDUPE_WINDOW_MINUTES, Database, DbResult, OptionalExt};

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

    /// Observations with no `mentions` row, eligible for retroactive entity
    /// backfill. Never-reviewed observations sort first, then observations
    /// whose review is older than `reconsider_after_days` — oldest first.
    #[allow(dead_code)]
    pub fn list_backfill_candidates(
        &self,
        project: Option<&str>,
        scope: Option<&str>,
        limit: i64,
        reconsider_after_days: i64,
    ) -> DbResult<Vec<crate::models::BackfillCandidate>> {
        let limit = limit.clamp(1, 50);
        let reconsider_after_days = reconsider_after_days.max(0);
        let mut stmt = self.conn.prepare(
            "SELECT id, title, content, type, created_at, entities_reviewed_at
             FROM observations
             WHERE deleted_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM mentions m WHERE m.observation_id = observations.id)
               AND (entities_reviewed_at IS NULL
                    OR entities_reviewed_at < datetime('now', '-' || ?1 || ' days'))
               AND (?2 IS NULL OR project = ?2)
               AND (?3 IS NULL OR scope = ?3)
             ORDER BY entities_reviewed_at IS NULL DESC, entities_reviewed_at ASC, created_at ASC
             LIMIT ?4",
        )?;
        let rows: Vec<crate::models::BackfillCandidate> = stmt
            .query_map(
                params![reconsider_after_days, project, scope, limit],
                |row| {
                    Ok(crate::models::BackfillCandidate {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        content: row.get(2)?,
                        observation_type: row.get(3)?,
                        created_at: row.get(4)?,
                        entities_reviewed_at: row.get(5)?,
                    })
                },
            )?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Marks an observation as evaluated for entity backfill (with nothing
    /// found to link). Returns `false` if the id doesn't exist or is already
    /// soft-deleted — mirrors `delete_observation`'s existence-check shape.
    #[allow(dead_code)]
    pub fn mark_entities_reviewed(&self, id: i64) -> DbResult<bool> {
        let affected = self.conn.execute(
            "UPDATE observations SET entities_reviewed_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now_utc(), id],
        )?;
        Ok(affected > 0)
    }
}
