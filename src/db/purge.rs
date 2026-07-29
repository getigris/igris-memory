use crate::errors::IgrisError;
use crate::models::PurgeResult;
use rusqlite::params;

use super::{Database, DbResult};

impl Database {
    /// Permanently delete observations that were soft-deleted more than
    /// `older_than_days` days ago. Runs VACUUM afterwards to reclaim space.
    pub fn purge(&self, older_than_days: i64) -> DbResult<PurgeResult> {
        self.purge_with_progress(older_than_days, |_, _, _| {})
    }

    /// Same as [`Database::purge`], reporting progress after each of the 2 phases
    /// completes via `on_progress(phase_name, phases_done, phases_total)`.
    pub fn purge_with_progress(
        &self,
        older_than_days: i64,
        mut on_progress: impl FnMut(&str, u64, u64),
    ) -> DbResult<PurgeResult> {
        const TOTAL: u64 = 2;
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
        on_progress("hard_delete", 1, TOTAL);

        // Reclaim disk space
        self.conn.execute_batch("VACUUM")?;
        on_progress("vacuum", 2, TOTAL);

        Ok(PurgeResult {
            observations_purged: affected as i64,
        })
    }
}
