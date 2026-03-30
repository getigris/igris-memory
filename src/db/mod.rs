mod export;
mod observations;
mod purge;
mod search;
mod sessions;
mod timeline;

use crate::errors::IgrisError;
use crate::models::Observation;
use crate::schema::{PRAGMAS, SCHEMA_V1, SCHEMA_VERSION};
use rusqlite::{Connection, Result as SqlResult};
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
    pub(crate) conn: Connection,
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

    // ─── Helpers ────────────────────────────────────────────────────

    pub(crate) fn row_to_observation(row: &rusqlite::Row) -> Observation {
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

pub(crate) trait OptionalExt<T> {
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
#[path = "tests/db_test.rs"]
mod tests;
