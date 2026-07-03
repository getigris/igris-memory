use crate::embed::{blob_to_vec, cosine_similarity, vec_to_blob};
use crate::models::{Observation, SearchResult};
use crate::utils::now_utc;
use rusqlite::params;
use std::collections::HashMap;

use super::{Database, DbResult};

impl Database {
    /// Store (or replace) an embedding for an object under a given model.
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

    /// Hybrid retrieval: FTS5 fused with vector similarity via Reciprocal Rank
    /// Fusion. With `query_embedding = None`, returns pure FTS (identical to
    /// `search`). The returned `SearchResult.rank` carries the fused RRF score.
    pub fn hybrid_search(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        model: &str,
        obs_type: Option<&str>,
        project: Option<&str>,
        limit: Option<i64>,
    ) -> DbResult<Vec<SearchResult>> {
        crate::validation::validate_limit(limit)?;
        let limit = limit.unwrap_or(super::DEFAULT_LIMIT).min(50);
        let candidate_k = 50;

        let fts = self.search(query, obs_type, project, Some(candidate_k))?;

        let query_embedding = match query_embedding {
            None => {
                let mut r = fts;
                r.truncate(limit.max(0) as usize);
                return Ok(r);
            }
            Some(q) => q,
        };

        let vec_hits =
            self.vector_search(query_embedding, model, obs_type, project, candidate_k)?;

        const K: f64 = 60.0;
        let mut score: HashMap<i64, f64> = HashMap::new();
        let mut by_id: HashMap<i64, SearchResult> = HashMap::new();

        for (rank, sr) in fts.iter().enumerate() {
            let id = sr.observation.id;
            *score.entry(id).or_insert(0.0) += 1.0 / (K + rank as f64 + 1.0);
            by_id.entry(id).or_insert_with(|| sr.clone());
        }
        for (rank, (obs, _sim)) in vec_hits.iter().enumerate() {
            let id = obs.id;
            *score.entry(id).or_insert(0.0) += 1.0 / (K + rank as f64 + 1.0);
            by_id.entry(id).or_insert_with(|| SearchResult {
                observation: obs.clone(),
                rank: 0.0,
                snippet: None,
            });
        }

        let mut fused: Vec<SearchResult> = score
            .into_iter()
            .filter_map(|(id, s)| {
                by_id.remove(&id).map(|mut sr| {
                    sr.rank = s;
                    sr
                })
            })
            .collect();
        fused.sort_by(|a, b| {
            b.rank
                .partial_cmp(&a.rank)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.observation.id.cmp(&b.observation.id))
        });
        fused.truncate(limit.max(0) as usize);
        Ok(fused)
    }

    /// Non-deleted observations that have no embedding for `model` (for backfill).
    pub fn observations_needing_embedding(&self, model: &str) -> DbResult<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT o.id, o.content FROM observations o
             WHERE o.deleted_at IS NULL
               AND NOT EXISTS (
                 SELECT 1 FROM embeddings e
                 WHERE e.object_type = 'observation'
                   AND e.object_id = o.id
                   AND e.model = ?1
               )
             ORDER BY o.id",
        )?;
        let rows = stmt
            .query_map(params![model], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }
}
