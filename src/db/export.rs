use crate::models::{ExportData, ImportResult, Session};
use crate::utils::{hash_content, now_utc};
use rusqlite::params;

use super::{Database, DbResult};

impl Database {
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
}
