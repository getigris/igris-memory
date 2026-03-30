use crate::models::Timeline;
use rusqlite::params;

use super::{Database, DbResult};

impl Database {
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
}
