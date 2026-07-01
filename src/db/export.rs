use crate::models::{ExportData, ImportResult, Session};
use crate::utils::{hash_content, now_utc};
use rusqlite::params;

use super::{Database, DbResult, OptionalExt};

impl Database {
    /// Export all observations, sessions, entities, aliases, edges, and mentions
    /// as a portable JSON structure. All non-deleted rows are included; entities
    /// and edges are also filtered to exclude soft-deleted entries.
    pub fn export_all(&self) -> DbResult<ExportData> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, type, title, content, project, scope,
                    topic_key, tags, revision_count, duplicate_count,
                    created_at, updated_at, deleted_at
             FROM observations ORDER BY id",
        )?;
        let mut observations = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            observations.push(Self::row_to_observation(row));
        }

        let mut stmt = self.conn.prepare(
            "SELECT id, project, directory, started_at, ended_at, summary
             FROM sessions ORDER BY started_at",
        )?;
        let mut sessions = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            sessions.push(Session {
                id: row.get(0)?,
                project: row.get(1)?,
                directory: row.get(2)?,
                started_at: row.get(3)?,
                ended_at: row.get(4)?,
                summary: row.get(5)?,
            });
        }

        // Entities (skip soft-deleted)
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, canonical_name, slug, tier, salience, compiled_truth,
                    compiled_at, project, scope, created_at, updated_at, deleted_at
             FROM entities WHERE deleted_at IS NULL ORDER BY id",
        )?;
        let mut entities = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            entities.push(Self::row_to_entity(row));
        }

        // Aliases
        let mut stmt = self.conn.prepare(
            "SELECT entity_id, alias_normalized, source FROM entity_aliases ORDER BY id",
        )?;
        let mut entity_aliases = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            entity_aliases.push(crate::models::EntityAlias {
                entity_id: row.get(0)?,
                alias_normalized: row.get(1)?,
                source: row.get(2)?,
            });
        }

        // Edges (skip soft-deleted)
        let mut stmt = self.conn.prepare(
            "SELECT id, src_entity_id, dst_entity_id, edge_type, evidence_count,
                    confidence, first_seen, last_seen, deleted_at
             FROM edges WHERE deleted_at IS NULL ORDER BY id",
        )?;
        let mut edges = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            edges.push(Self::row_to_edge(row));
        }

        // Mentions
        let mut stmt = self
            .conn
            .prepare("SELECT observation_id, entity_id FROM mentions ORDER BY id")?;
        let mut mentions = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            mentions.push(crate::models::Mention {
                observation_id: row.get(0)?,
                entity_id: row.get(1)?,
            });
        }

        Ok(ExportData {
            version: crate::schema::SCHEMA_VERSION,
            exported_at: now_utc(),
            observations,
            sessions,
            entities,
            entity_aliases,
            edges,
            mentions,
        })
    }

    /// Import observations, sessions, and the entity graph from an export.
    /// Deduplicates observations by content hash and entities by slug+project+scope;
    /// remaps old ids to the destination's ids so aliases/edges/mentions stay linked.
    pub fn import_data(&self, data: &ExportData) -> DbResult<ImportResult> {
        use std::collections::HashMap;

        let mut obs_imported: i64 = 0;
        let mut obs_skipped: i64 = 0;
        let mut sess_imported: i64 = 0;
        let mut sess_skipped: i64 = 0;
        let mut ent_imported: i64 = 0;
        let mut ent_skipped: i64 = 0;
        let mut edges_imported: i64 = 0;
        let mut mentions_imported: i64 = 0;

        // Sessions first (observations may reference them).
        for session in &data.sessions {
            let exists: bool = self.conn.query_row(
                "SELECT COUNT(*) > 0 FROM sessions WHERE id = ?1",
                params![session.id],
                |row| row.get(0),
            )?;
            if exists {
                sess_skipped += 1;
                continue;
            }
            self.conn.execute(
                "INSERT INTO sessions (id, project, directory, started_at, ended_at, summary)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    session.id,
                    session.project,
                    session.directory,
                    session.started_at,
                    session.ended_at,
                    session.summary
                ],
            )?;
            sess_imported += 1;
        }

        // Observations — dedup by hash; record old->new id (dupes map to existing id).
        let mut obs_map: HashMap<i64, i64> = HashMap::new();
        for obs in &data.observations {
            if obs.deleted_at.is_some() {
                obs_skipped += 1;
                continue;
            }
            let content_hash = hash_content(&obs.content);
            let existing: Option<i64> = self
                .conn
                .query_row(
                    "SELECT id FROM observations
                     WHERE normalized_hash = ?1
                       AND IFNULL(project, '') = IFNULL(?2, '')
                       AND scope = ?3
                       AND type = ?4
                       AND deleted_at IS NULL",
                    params![content_hash, obs.project, obs.scope, obs.observation_type],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(id) = existing {
                obs_skipped += 1;
                obs_map.insert(obs.id, id);
                continue;
            }
            let tags_json = obs
                .tags
                .as_ref()
                .map(|t| serde_json::to_string(t).unwrap_or_default());
            self.conn.execute(
                "INSERT INTO observations
                 (session_id, type, title, content, project, scope, topic_key,
                  normalized_hash, tags, revision_count, duplicate_count,
                  created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    obs.session_id,
                    obs.observation_type,
                    obs.title,
                    obs.content,
                    obs.project,
                    obs.scope,
                    obs.topic_key,
                    content_hash,
                    tags_json,
                    obs.revision_count,
                    obs.duplicate_count,
                    obs.created_at,
                    obs.updated_at,
                ],
            )?;
            obs_map.insert(obs.id, self.conn.last_insert_rowid());
            obs_imported += 1;
        }

        // Entities — dedup by (slug, project, scope); record old->new id.
        let mut ent_map: HashMap<i64, i64> = HashMap::new();
        for e in &data.entities {
            if e.deleted_at.is_some() {
                ent_skipped += 1;
                continue;
            }
            let existing: Option<i64> = self
                .conn
                .query_row(
                    "SELECT id FROM entities
                     WHERE slug = ?1
                       AND IFNULL(project, '') = IFNULL(?2, '')
                       AND scope = ?3
                       AND deleted_at IS NULL",
                    params![e.slug, e.project, e.scope],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(id) = existing {
                ent_skipped += 1;
                ent_map.insert(e.id, id);
                continue;
            }
            self.conn.execute(
                "INSERT INTO entities
                 (kind, canonical_name, slug, tier, salience, compiled_truth,
                  compiled_at, project, scope, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    e.kind,
                    e.canonical_name,
                    e.slug,
                    e.tier,
                    e.salience,
                    e.compiled_truth,
                    e.compiled_at,
                    e.project,
                    e.scope,
                    e.created_at,
                    e.updated_at,
                ],
            )?;
            ent_map.insert(e.id, self.conn.last_insert_rowid());
            ent_imported += 1;
        }

        // Aliases — remap entity_id, idempotent via unique index.
        for a in &data.entity_aliases {
            if let Some(&eid) = ent_map.get(&a.entity_id) {
                self.conn.execute(
                    "INSERT OR IGNORE INTO entity_aliases (entity_id, alias_normalized, source)
                     VALUES (?1, ?2, ?3)",
                    params![eid, a.alias_normalized, a.source],
                )?;
            }
        }

        // Edges — remap both endpoints; skip soft-deleted; count only new rows.
        for ed in &data.edges {
            if ed.deleted_at.is_some() {
                continue;
            }
            if let (Some(&s), Some(&d)) = (
                ent_map.get(&ed.src_entity_id),
                ent_map.get(&ed.dst_entity_id),
            ) {
                let changed = self.conn.execute(
                    "INSERT OR IGNORE INTO edges
                     (src_entity_id, dst_entity_id, edge_type, evidence_count,
                      confidence, first_seen, last_seen)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        s,
                        d,
                        ed.edge_type,
                        ed.evidence_count,
                        ed.confidence,
                        ed.first_seen,
                        ed.last_seen
                    ],
                )?;
                edges_imported += changed as i64;
            }
        }

        // Mentions — remap observation_id and entity_id; count only new rows.
        for m in &data.mentions {
            if let (Some(&oid), Some(&eid)) =
                (obs_map.get(&m.observation_id), ent_map.get(&m.entity_id))
            {
                let changed = self.conn.execute(
                    "INSERT OR IGNORE INTO mentions (observation_id, entity_id) VALUES (?1, ?2)",
                    params![oid, eid],
                )?;
                mentions_imported += changed as i64;
            }
        }

        Ok(ImportResult {
            observations_imported: obs_imported,
            observations_skipped: obs_skipped,
            sessions_imported: sess_imported,
            sessions_skipped: sess_skipped,
            entities_imported: ent_imported,
            entities_skipped: ent_skipped,
            edges_imported,
            mentions_imported,
        })
    }
}
