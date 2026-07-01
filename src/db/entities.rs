use crate::errors::IgrisError;
use crate::models::{Edge, Entity, EntityBrief, EntityNeighbor, Observation};
use crate::store::BrainStore;
use crate::utils::{entity_slug, normalize_alias, now_utc, strip_private_tags};
use crate::validation;
use rusqlite::params;

use super::{Database, DbResult, OptionalExt};

impl Database {
    pub(crate) fn row_to_entity(row: &rusqlite::Row) -> Entity {
        Entity {
            id: row.get(0).unwrap_or_default(),
            kind: row.get(1).unwrap_or_default(),
            canonical_name: row.get(2).unwrap_or_default(),
            slug: row.get(3).unwrap_or_default(),
            tier: row.get(4).unwrap_or(3),
            salience: row.get(5).unwrap_or(0.0),
            compiled_truth: row.get(6).unwrap_or(None),
            compiled_at: row.get(7).unwrap_or(None),
            project: row.get(8).unwrap_or(None),
            scope: row.get(9).unwrap_or_default(),
            created_at: row.get(10).unwrap_or_default(),
            updated_at: row.get(11).unwrap_or_default(),
            deleted_at: row.get(12).unwrap_or(None),
        }
    }

    const ENTITY_COLS: &'static str = "id, kind, canonical_name, slug, tier, salience, compiled_truth, \
         compiled_at, project, scope, created_at, updated_at, deleted_at";

    /// Register one alias for an entity (idempotent via unique index).
    fn add_alias(&self, entity_id: i64, raw: &str, source: &str) -> DbResult<()> {
        let normalized = normalize_alias(&strip_private_tags(raw));
        if normalized.is_empty() {
            return Ok(());
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO entity_aliases (entity_id, alias_normalized, source)
             VALUES (?1, ?2, ?3)",
            params![entity_id, normalized, source],
        )?;
        Ok(())
    }

    /// Deterministic Compiled Truth v0 template — pure, no LLM, no I/O.
    fn format_compiled_truth(
        entity: &Entity,
        mention_count: i64,
        neighbors: &[EntityNeighbor],
        recent: &[Observation],
    ) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", entity.canonical_name));
        out.push_str(&format!("- **Kind:** {}\n", entity.kind));
        out.push_str(&format!("- **Mentions:** {mention_count}\n"));
        out.push_str(&format!("- **Connections:** {}\n\n", neighbors.len()));

        out.push_str("## Connections\n");
        if neighbors.is_empty() {
            out.push_str("_None yet._\n");
        } else {
            for n in neighbors {
                out.push_str(&format!(
                    "- {} — {} ({}×)\n",
                    n.entity.canonical_name, n.edge.edge_type, n.edge.evidence_count
                ));
            }
        }

        out.push_str("\n## Recent mentions\n");
        if recent.is_empty() {
            out.push_str("_None yet._\n");
        } else {
            for o in recent {
                out.push_str(&format!("- {} — {} (#{})\n", o.created_at, o.title, o.id));
            }
        }
        out
    }
}

impl BrainStore for Database {
    fn upsert_entity(
        &self,
        kind: &str,
        canonical_name: &str,
        aliases: &[String],
        project: Option<&str>,
        scope: &str,
    ) -> DbResult<Entity> {
        let clean_name = strip_private_tags(canonical_name);
        validation::validate_entity(kind, &clean_name, scope).map_err(IgrisError::validation)?;
        let slug = entity_slug(&clean_name);
        let now = now_utc();

        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM entities
                 WHERE slug = ?1
                   AND IFNULL(project, '') = IFNULL(?2, '')
                   AND scope = ?3
                   AND deleted_at IS NULL
                 LIMIT 1",
                params![slug, project, scope],
                |row| row.get(0),
            )
            .optional()?;

        let entity_id = if let Some(id) = existing {
            self.conn.execute(
                "UPDATE entities
                 SET kind = ?1, canonical_name = ?2, updated_at = ?3
                 WHERE id = ?4",
                params![kind, clean_name, now, id],
            )?;
            id
        } else {
            self.conn.execute(
                "INSERT INTO entities
                 (kind, canonical_name, slug, project, scope, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![kind, clean_name, slug, project, scope, now, now],
            )?;
            self.conn.last_insert_rowid()
        };

        // Register canonical name + provided aliases as normalized aliases.
        self.add_alias(entity_id, &clean_name, "canonical")?;
        for alias in aliases {
            self.add_alias(entity_id, alias, "provided")?;
        }

        self.get_entity(entity_id)
    }

    fn get_entity(&self, id: i64) -> DbResult<Entity> {
        let found = self
            .conn
            .query_row(
                &format!(
                    "SELECT {} FROM entities WHERE id = ?1 AND deleted_at IS NULL",
                    Self::ENTITY_COLS
                ),
                params![id],
                |row| Ok(Self::row_to_entity(row)),
            )
            .optional()?;
        found.ok_or_else(|| IgrisError::not_found(format!("Entity {id} not found")))
    }

    fn get_entity_by_slug(
        &self,
        slug: &str,
        project: Option<&str>,
        scope: &str,
    ) -> DbResult<Entity> {
        let found = self
            .conn
            .query_row(
                &format!(
                    "SELECT {} FROM entities
                     WHERE slug = ?1
                       AND IFNULL(project, '') = IFNULL(?2, '')
                       AND scope = ?3
                       AND deleted_at IS NULL
                     LIMIT 1",
                    Self::ENTITY_COLS
                ),
                params![slug, project, scope],
                |row| Ok(Self::row_to_entity(row)),
            )
            .optional()?;
        found.ok_or_else(|| IgrisError::not_found(format!("Entity '{slug}' not found")))
    }

    fn add_mention(&self, observation_id: i64, entity_id: i64) -> DbResult<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO mentions (observation_id, entity_id) VALUES (?1, ?2)",
            params![observation_id, entity_id],
        )?;
        Ok(())
    }

    fn resolve_or_stub_entity(
        &self,
        mention: &str,
        project: Option<&str>,
        scope: &str,
    ) -> DbResult<Entity> {
        let normalized = normalize_alias(mention);
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT a.entity_id
                 FROM entity_aliases a
                 JOIN entities e ON e.id = a.entity_id
                 WHERE a.alias_normalized = ?1
                   AND IFNULL(e.project, '') = IFNULL(?2, '')
                   AND e.scope = ?3
                   AND e.deleted_at IS NULL
                 ORDER BY datetime(e.updated_at) DESC
                 LIMIT 1",
                params![normalized, project, scope],
                |row| row.get(0),
            )
            .optional()?;

        match existing {
            Some(id) => self.get_entity(id),
            None => self.upsert_entity("other", mention, &[], project, scope),
        }
    }

    fn upsert_edge(
        &self,
        src_entity_id: i64,
        dst_entity_id: i64,
        edge_type: &str,
    ) -> DbResult<Edge> {
        let now = now_utc();
        self.conn.execute(
            "INSERT INTO edges
                 (src_entity_id, dst_entity_id, edge_type, evidence_count, confidence, first_seen, last_seen)
             VALUES (?1, ?2, ?3, 1, 1.0, ?4, ?4)
             ON CONFLICT(src_entity_id, dst_entity_id, edge_type)
             DO UPDATE SET evidence_count = evidence_count + 1, last_seen = ?4",
            params![src_entity_id, dst_entity_id, edge_type, now],
        )?;
        Ok(self.conn.query_row(
            &format!(
                "SELECT {} FROM edges
                 WHERE src_entity_id = ?1 AND dst_entity_id = ?2 AND edge_type = ?3",
                Self::EDGE_COLS
            ),
            params![src_entity_id, dst_entity_id, edge_type],
            |row| Ok(Self::row_to_edge(row)),
        )?)
    }

    fn record_mentions(
        &self,
        observation_id: i64,
        mentions: &[String],
        project: Option<&str>,
        scope: &str,
    ) -> DbResult<Vec<Entity>> {
        let mut resolved: Vec<Entity> = Vec::new();
        let mut ids: Vec<i64> = Vec::new();
        for mention in mentions {
            if normalize_alias(mention).is_empty() {
                continue;
            }
            let entity = self.resolve_or_stub_entity(mention, project, scope)?;
            if !ids.contains(&entity.id) {
                self.add_mention(observation_id, entity.id)?;
                ids.push(entity.id);
                resolved.push(entity);
            }
        }
        // Co-occurrence edges between every distinct pair (stored src < dst).
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let (a, b) = if ids[i] < ids[j] {
                    (ids[i], ids[j])
                } else {
                    (ids[j], ids[i])
                };
                self.upsert_edge(a, b, "co_mentioned")?;
            }
        }
        Ok(resolved)
    }

    fn entity_neighbors(&self, entity_id: i64, limit: i64) -> DbResult<Vec<EntityNeighbor>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM edges
             WHERE (src_entity_id = ?1 OR dst_entity_id = ?1)
               AND deleted_at IS NULL
             ORDER BY evidence_count DESC, datetime(last_seen) DESC, id DESC
             LIMIT ?2",
            Self::EDGE_COLS
        ))?;
        let edges: Vec<Edge> = stmt
            .query_map(params![entity_id, limit], |row| Ok(Self::row_to_edge(row)))?
            .filter_map(|r| r.ok())
            .collect();

        let mut neighbors = Vec::new();
        for edge in edges {
            let other_id = if edge.src_entity_id == entity_id {
                edge.dst_entity_id
            } else {
                edge.src_entity_id
            };
            if let Ok(entity) = self.get_entity(other_id) {
                neighbors.push(EntityNeighbor { edge, entity });
            }
        }
        Ok(neighbors)
    }

    fn entity_timeline(&self, entity_id: i64, limit: i64) -> DbResult<Vec<Observation>> {
        let mut stmt = self.conn.prepare(
            "SELECT o.id, o.session_id, o.type, o.title, o.content, o.project, o.scope,
                    o.topic_key, o.tags, o.revision_count, o.duplicate_count,
                    o.created_at, o.updated_at, o.deleted_at
             FROM observations o
             JOIN mentions m ON m.observation_id = o.id
             WHERE m.entity_id = ?1 AND o.deleted_at IS NULL
             ORDER BY datetime(o.created_at) DESC, o.id DESC
             LIMIT ?2",
        )?;
        let rows: Vec<Observation> = stmt
            .query_map(params![entity_id, limit], |row| {
                Ok(Self::row_to_observation(row))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    fn compile_entity_truth(&self, entity_id: i64) -> DbResult<String> {
        let entity = self.get_entity(entity_id)?;
        let mention_count: i64 = self.conn.query_row(
            "SELECT count(*) FROM mentions WHERE entity_id = ?1",
            params![entity_id],
            |r| r.get(0),
        )?;
        let neighbors = self.entity_neighbors(entity_id, 10)?;
        let recent = self.entity_timeline(entity_id, 5)?;
        let truth = Self::format_compiled_truth(&entity, mention_count, &neighbors, &recent);

        let now = now_utc();
        self.conn.execute(
            "UPDATE entities SET compiled_truth = ?1, compiled_at = ?2 WHERE id = ?3",
            params![truth, now, entity_id],
        )?;
        Ok(truth)
    }

    fn entity_brief(&self, entity_id: i64) -> DbResult<EntityBrief> {
        // Recompile so the returned entity carries a fresh Compiled Truth.
        self.compile_entity_truth(entity_id)?;
        let entity = self.get_entity(entity_id)?;
        let neighbors = self.entity_neighbors(entity_id, 10)?;
        let recent = self.entity_timeline(entity_id, 10)?;
        Ok(EntityBrief {
            entity,
            neighbors,
            recent,
        })
    }
}
