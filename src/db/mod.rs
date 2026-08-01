mod codegraph;
mod embeddings;
mod entities;
mod export;
mod graph;
mod observations;
mod purge;
mod search;
mod sessions;
mod timeline;
mod vecindex;

use crate::errors::IgrisError;
use crate::models::Observation;
use crate::schema::{
    PRAGMAS, SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_VERSION,
};
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
    pub(crate) vector_index: bool,
}

impl Database {
    /// Open (or create) the database at the given path and run migrations.
    /// If `key` is provided, the database is encrypted with SQLCipher.
    #[allow(dead_code)] // kept as the default entry point for callers who don't need `vector_index`
    pub fn open(path: &Path, key: Option<&str>) -> SqlResult<Self> {
        Self::open_with(path, key, false)
    }

    /// Open (or create) the database at the given path and run migrations,
    /// selecting the vector search backend (`vector_index`: false = brute-force, true = sqlite-vec ANN).
    /// If `key` is provided, the database is encrypted with SQLCipher.
    pub fn open_with(path: &Path, key: Option<&str>, vector_index: bool) -> SqlResult<Self> {
        crate::embed::register_sqlite_vec();
        let conn = Connection::open(path)?;
        if let Some(k) = key {
            conn.pragma_update(None, "key", k)?;
        }
        let db = Self { conn, vector_index };
        db.init()?;
        Ok(db)
    }

    /// Open an in-memory database (for tests).
    #[cfg(test)]
    pub fn open_in_memory() -> SqlResult<Self> {
        crate::embed::register_sqlite_vec();
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn,
            vector_index: false,
        };
        db.init()?;
        Ok(db)
    }

    /// Open an in-memory database with the sqlite-vec index enabled (for tests).
    #[cfg(test)]
    pub fn open_in_memory_vec() -> SqlResult<Self> {
        crate::embed::register_sqlite_vec();
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn,
            vector_index: true,
        };
        db.init()?;
        Ok(db)
    }

    // Every SCHEMA_Vn block below is idempotent (`IF NOT EXISTS` guards), so
    // `version < N` vs `version <= N` is unobservable — equivalent mutant.
    #[cfg_attr(test, mutants::skip)]
    fn init(&self) -> SqlResult<()> {
        self.conn.execute_batch(PRAGMAS)?;
        let version: u32 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            self.conn.execute_batch(SCHEMA_V1)?;
        }
        if version < 2 {
            self.conn.execute_batch(SCHEMA_V2)?;
        }
        if version < 3 {
            self.conn.execute_batch(SCHEMA_V3)?;
        }
        if version < 4 {
            self.conn.execute_batch(SCHEMA_V4)?;
        }
        if version < 5 {
            self.conn.execute_batch(SCHEMA_V5)?;
        }
        if version < SCHEMA_VERSION {
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

#[cfg(test)]
#[path = "tests/entity_test.rs"]
mod entity_tests;

#[cfg(test)]
#[path = "tests/codegraph_test.rs"]
mod codegraph_tests;

#[cfg(test)]
#[path = "tests/embed_test.rs"]
mod embed_tests;

#[cfg(test)]
#[path = "tests/vec_test.rs"]
mod vec_tests;
