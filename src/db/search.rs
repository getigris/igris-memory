use crate::models::{Observation, SearchResult, Stats};
use crate::validation;
use rusqlite::{params, Result as SqlResult};
use std::collections::HashMap;

use super::{Database, DbResult, DEFAULT_LIMIT};

impl Database {
    /// Full-text search across observations using FTS5.
    pub fn search(
        &self,
        query: &str,
        obs_type: Option<&str>,
        project: Option<&str>,
        limit: Option<i64>,
    ) -> DbResult<Vec<SearchResult>> {
        validation::validate_search_query(query)?;
        validation::validate_limit(limit)?;
        if let Some(t) = obs_type {
            validation::validate_observation_type(t)?;
        }
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(50);

        // Wrap each word in quotes for safe FTS5 matching
        let fts_query: String = query
            .split_whitespace()
            .map(|w| format!("\"{}\"", w.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" ");

        let sql = r#"
            SELECT o.id, o.session_id, o.type, o.title, o.content, o.project,
                   o.scope, o.topic_key, o.tags, o.revision_count, o.duplicate_count,
                   o.created_at, o.updated_at, o.deleted_at,
                   fts.rank,
                   snippet(observations_fts, 1, '→', '←', '...', 32) AS snippet
            FROM observations_fts fts
            JOIN observations o ON o.id = fts.rowid
            WHERE observations_fts MATCH ?1
              AND o.deleted_at IS NULL
        "#;

        let mut conditions = String::new();
        if obs_type.is_some() {
            conditions.push_str(" AND o.type = ?2");
        }
        if project.is_some() {
            conditions.push_str(if obs_type.is_some() {
                " AND o.project = ?3"
            } else {
                " AND o.project = ?2"
            });
        }

        let full_sql = format!(
            "{sql}{conditions} ORDER BY fts.rank LIMIT {limit}"
        );

        let mut stmt = self.conn.prepare(&full_sql)?;

        // Bind parameters dynamically based on which filters are active
        let rows: Vec<SearchResult> = match (obs_type, project) {
            (Some(t), Some(p)) => {
                let iter = stmt.query_map(params![fts_query, t, p], |row| {
                    Ok(SearchResult {
                        observation: Self::row_to_observation(row),
                        rank: row.get(14)?,
                        snippet: row.get(15)?,
                    })
                })?;
                iter.collect::<SqlResult<Vec<_>>>()?
            }
            (Some(t), None) => {
                let iter = stmt.query_map(params![fts_query, t], |row| {
                    Ok(SearchResult {
                        observation: Self::row_to_observation(row),
                        rank: row.get(14)?,
                        snippet: row.get(15)?,
                    })
                })?;
                iter.collect::<SqlResult<Vec<_>>>()?
            }
            (None, Some(p)) => {
                let iter = stmt.query_map(params![fts_query, p], |row| {
                    Ok(SearchResult {
                        observation: Self::row_to_observation(row),
                        rank: row.get(14)?,
                        snippet: row.get(15)?,
                    })
                })?;
                iter.collect::<SqlResult<Vec<_>>>()?
            }
            (None, None) => {
                let iter = stmt.query_map(params![fts_query], |row| {
                    Ok(SearchResult {
                        observation: Self::row_to_observation(row),
                        rank: row.get(14)?,
                        snippet: row.get(15)?,
                    })
                })?;
                iter.collect::<SqlResult<Vec<_>>>()?
            }
        };

        Ok(rows)
    }

    /// Recent observations for context loading at session start.
    pub fn recent_context(
        &self,
        project: Option<&str>,
        limit: Option<i64>,
    ) -> DbResult<Vec<Observation>> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(50);

        let (sql, bind_val) = if let Some(p) = project {
            (
                format!(
                    "SELECT id, session_id, type, title, content, project, scope,
                            topic_key, tags, revision_count, duplicate_count,
                            created_at, updated_at, deleted_at
                     FROM observations
                     WHERE deleted_at IS NULL AND project = ?1
                     ORDER BY datetime(updated_at) DESC
                     LIMIT {limit}"
                ),
                Some(p.to_string()),
            )
        } else {
            (
                format!(
                    "SELECT id, session_id, type, title, content, project, scope,
                            topic_key, tags, revision_count, duplicate_count,
                            created_at, updated_at, deleted_at
                     FROM observations
                     WHERE deleted_at IS NULL
                     ORDER BY datetime(updated_at) DESC
                     LIMIT {limit}"
                ),
                None,
            )
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let mut results = Vec::new();

        let mut rows = if let Some(ref p) = bind_val {
            stmt.query(params![p])?
        } else {
            stmt.query([])?
        };

        while let Some(row) = rows.next()? {
            results.push(Self::row_to_observation(row));
        }

        Ok(results)
    }

    /// Aggregate statistics about the memory store.
    pub fn stats(&self) -> DbResult<Stats> {
        let total_observations: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM observations WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )?;

        let total_sessions: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;

        let active_sessions: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE ended_at IS NULL",
            [],
            |r| r.get(0),
        )?;

        let mut by_type = HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT type, COUNT(*) FROM observations WHERE deleted_at IS NULL GROUP BY type",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (t, c) = row?;
            by_type.insert(t, c);
        }

        let mut by_project = HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT IFNULL(project, 'unset'), COUNT(*) FROM observations WHERE deleted_at IS NULL GROUP BY project",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (p, c) = row?;
            by_project.insert(p, c);
        }

        Ok(Stats {
            total_observations,
            total_sessions,
            active_sessions,
            by_type,
            by_project,
        })
    }
}
