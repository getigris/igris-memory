use crate::embed::vec_to_blob;
use rusqlite::params;

use super::{Database, DbResult, OptionalExt};

impl Database {
    /// Ensure the vec0 index exists for (dim, model). If it currently holds a
    /// different dim/model, it is reset (dropped + recreated) — a rebuild
    /// repopulates it. Uses a single-row meta table (id = 1).
    pub fn vec_index_ensure(&self, dim: i64, model: &str) -> DbResult<()> {
        let current: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT dim, model FROM vec_index_meta WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match current {
            Some((d, m)) if d == dim && m == model => Ok(()),
            _ => {
                self.conn
                    .execute_batch("DROP TABLE IF EXISTS embeddings_vec;")?;
                self.conn.execute_batch(&format!(
                    "CREATE VIRTUAL TABLE embeddings_vec USING vec0(embedding float[{dim}]);"
                ))?;
                self.conn.execute("DELETE FROM vec_index_meta", [])?;
                self.conn.execute(
                    "INSERT INTO vec_index_meta (id, dim, model) VALUES (1, ?1, ?2)",
                    params![dim, model],
                )?;
                Ok(())
            }
        }
    }

    /// Insert/replace an observation's vector in the vec0 index.
    pub fn vec_index_upsert(&self, obs_id: i64, model: &str, vector: &[f32]) -> DbResult<()> {
        self.vec_index_ensure(vector.len() as i64, model)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO embeddings_vec(rowid, embedding) VALUES (?1, ?2)",
            params![obs_id, vec_to_blob(vector)],
        )?;
        Ok(())
    }

    /// Rebuild the vec0 index from the durable embeddings table for `model`.
    pub fn vec_index_rebuild(&self, model: &str) -> DbResult<i64> {
        self.conn
            .execute_batch("DROP TABLE IF EXISTS embeddings_vec;")?;
        self.conn.execute("DELETE FROM vec_index_meta", [])?;
        let mut stmt = self.conn.prepare(
            "SELECT object_id, vector FROM embeddings WHERE object_type = 'observation' AND model = ?1",
        )?;
        let rows: Vec<(i64, Vec<u8>)> = stmt
            .query_map(params![model], |r| Ok((r.get(0)?, r.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        let mut n = 0i64;
        for (obs_id, blob) in rows {
            let vec = crate::embed::blob_to_vec(&blob);
            self.vec_index_upsert(obs_id, model, &vec)?;
            n += 1;
        }
        Ok(n)
    }
}
