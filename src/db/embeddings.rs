use crate::embed::vec_to_blob;
use crate::utils::now_utc;
use rusqlite::params;

use super::{Database, DbResult};

impl Database {
    /// Store (or replace) an embedding for an object under a given model.
    #[allow(dead_code)] // TODO(fase-1b): remove once wired into server/CLI
    pub fn upsert_embedding(
        &self,
        object_type: &str,
        object_id: i64,
        model: &str,
        vector: &[f32],
    ) -> DbResult<()> {
        let blob = vec_to_blob(vector);
        let dim = vector.len() as i64;
        self.conn.execute(
            "INSERT INTO embeddings (object_type, object_id, model, dim, vector, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(object_type, object_id, model)
             DO UPDATE SET dim = ?4, vector = ?5, created_at = ?6",
            params![object_type, object_id, model, dim, blob, now_utc()],
        )?;
        Ok(())
    }
}
