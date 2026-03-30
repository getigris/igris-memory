use crate::models::Session;
use crate::utils::{now_utc, strip_private_tags};
use crate::validation;
use rusqlite::params;

use super::{Database, DbResult, OptionalExt};

impl Database {
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

    pub(crate) fn get_session(&self, id: &str) -> DbResult<Session> {
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
}
