use crate::embed::{blob_to_vec, cosine_similarity, vec_to_blob};
use crate::models::Observation;
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

    /// Brute-force cosine search over stored observation embeddings for `model`.
    #[allow(dead_code)] // TODO(fase-1b): remove once wired into server/CLI
    pub fn vector_search(
        &self,
        query_vec: &[f32],
        model: &str,
        obs_type: Option<&str>,
        project: Option<&str>,
        top_k: i64,
    ) -> DbResult<Vec<(Observation, f32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT o.id, o.session_id, o.type, o.title, o.content, o.project, o.scope,
                    o.topic_key, o.tags, o.revision_count, o.duplicate_count,
                    o.created_at, o.updated_at, o.deleted_at, e.vector
             FROM embeddings e
             JOIN observations o ON o.id = e.object_id
             WHERE e.object_type = 'observation'
               AND e.model = ?1
               AND o.deleted_at IS NULL
               AND (?2 IS NULL OR o.type = ?2)
               AND (?3 IS NULL OR IFNULL(o.project, '') = IFNULL(?3, ''))",
        )?;
        let mut scored: Vec<(Observation, f32)> = stmt
            .query_map(params![model, obs_type, project], |row| {
                let obs = Self::row_to_observation(row);
                let blob: Vec<u8> = row.get(14)?;
                Ok((obs, blob))
            })?
            .filter_map(|r| r.ok())
            .map(|(obs, blob)| {
                let sim = cosine_similarity(query_vec, &blob_to_vec(&blob));
                (obs, sim)
            })
            .collect();

        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.id.cmp(&b.0.id)) // stable tiebreak
        });
        scored.truncate(top_k.max(0) as usize);
        Ok(scored)
    }
}
