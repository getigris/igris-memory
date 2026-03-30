use crate::errors::IgrisError;
use crate::models::PurgeResult;
use rusqlite::params;

use super::{Database, DbResult};

impl Database {
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
}
