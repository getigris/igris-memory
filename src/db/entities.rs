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

        // Atomic: SELECT-then-INSERT/UPDATE plus alias registration must not
        // be observable as separate steps. unchecked_transaction is used
        // (rather than transaction, which needs &mut self) because BrainStore
        // methods take &self.
        let tx = self.conn.unchecked_transaction()?;

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

        tx.commit()?;
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

    fn update_entity(
        &self,
        id: i64,
        kind: Option<&str>,
        tier: Option<i32>,
        salience: Option<f64>,
        add_aliases: &[String],
    ) -> DbResult<Entity> {
        validation::validate_entity_update_has_fields(kind, tier, salience, add_aliases)
            .map_err(IgrisError::validation)?;
        self.get_entity(id)?;

        if let Some(k) = kind {
            validation::validate_entity_kind(k).map_err(IgrisError::validation)?;
        }
        let now = now_utc();

        if let Some(k) = kind {
            self.conn.execute(
                "UPDATE entities SET kind = ?1, updated_at = ?2 WHERE id = ?3",
                params![k, now, id],
            )?;
        }
        if let Some(t) = tier {
            self.conn.execute(
                "UPDATE entities SET tier = ?1, updated_at = ?2 WHERE id = ?3",
                params![t, now, id],
            )?;
        }
        if let Some(s) = salience {
            self.conn.execute(
                "UPDATE entities SET salience = ?1, updated_at = ?2 WHERE id = ?3",
                params![s, now, id],
            )?;
        }
        for alias in add_aliases {
            self.add_alias(id, alias, "provided")?;
        }

        self.get_entity(id)
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

    fn search_entities(
        &self,
        query: &str,
        kind: Option<&str>,
        project: Option<&str>,
        scope: Option<&str>,
        limit: i64,
    ) -> DbResult<Vec<Entity>> {
        let like = format!("%{}%", crate::utils::normalize_alias(query));
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT e.id, e.kind, e.canonical_name, e.slug, e.tier, e.salience,
                    e.compiled_truth, e.compiled_at, e.project, e.scope,
                    e.created_at, e.updated_at, e.deleted_at
             FROM entities e
             JOIN entity_aliases a ON a.entity_id = e.id
             WHERE a.alias_normalized LIKE ?1
               AND e.deleted_at IS NULL
               AND (?2 IS NULL OR e.kind = ?2)
               AND (?3 IS NULL OR IFNULL(e.project, '') = IFNULL(?3, ''))
               AND (?4 IS NULL OR e.scope = ?4)
             ORDER BY e.salience DESC, datetime(e.updated_at) DESC, e.id DESC
             LIMIT ?5",
        )?;
        let rows: Vec<Entity> = stmt
            .query_map(params![like, kind, project, scope, limit], |row| {
                Ok(Self::row_to_entity(row))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    fn list_entities(
        &self,
        kind: Option<&str>,
        project: Option<&str>,
        scope: Option<&str>,
        limit: i64,
    ) -> DbResult<Vec<Entity>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, canonical_name, slug, tier, salience, compiled_truth,
                    compiled_at, project, scope, created_at, updated_at, deleted_at
             FROM entities
             WHERE deleted_at IS NULL
               AND (?1 IS NULL OR kind = ?1)
               AND (?2 IS NULL OR IFNULL(project, '') = IFNULL(?2, ''))
               AND (?3 IS NULL OR scope = ?3)
             ORDER BY datetime(updated_at) DESC, id DESC
             LIMIT ?4",
        )?;
        let rows: Vec<Entity> = stmt
            .query_map(params![kind, project, scope, limit], |row| {
                Ok(Self::row_to_entity(row))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    fn delete_entity(&self, id: i64) -> DbResult<bool> {
        let affected = self.conn.execute(
            "UPDATE entities SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now_utc(), id],
        )?;
        Ok(affected > 0)
    }

    fn unlink_entities(&self, src_id: i64, dst_id: i64, relation: &str) -> DbResult<i64> {
        let affected = self.conn.execute(
            "UPDATE edges SET deleted_at = ?1
             WHERE edge_type = ?2 AND deleted_at IS NULL
               AND ((src_entity_id = ?3 AND dst_entity_id = ?4)
                 OR (src_entity_id = ?4 AND dst_entity_id = ?3))",
            params![now_utc(), relation, src_id, dst_id],
        )?;
        Ok(affected as i64)
    }

    fn merge_entities(&self, source_id: i64, target_id: i64) -> DbResult<Entity> {
        if source_id == target_id {
            return Err(IgrisError::validation("cannot merge an entity into itself"));
        }
        let source_entity = self.get_entity(source_id)?;
        let _target = self.get_entity(target_id)?;
        let now = now_utc();

        // Aliases -> target (idempotent via the unique (alias_normalized, entity_id) index).
        self.conn.execute(
            "INSERT OR IGNORE INTO entity_aliases (entity_id, alias_normalized, source)
             SELECT ?1, alias_normalized, 'merged' FROM entity_aliases WHERE entity_id = ?2",
            params![target_id, source_id],
        )?;
        self.add_alias(target_id, &source_entity.canonical_name, "merged")?;

        // Mentions -> target, deduped by the (observation_id, entity_id) unique index.
        self.conn.execute(
            "INSERT OR IGNORE INTO mentions (observation_id, entity_id)
             SELECT observation_id, ?1 FROM mentions WHERE entity_id = ?2",
            params![target_id, source_id],
        )?;
        self.conn.execute(
            "DELETE FROM mentions WHERE entity_id = ?1",
            params![source_id],
        )?;

        // Edges -> target: repoint each edge where source is an endpoint, then
        // soft-delete all of source's original edges.
        struct SourceEdge {
            src: i64,
            dst: i64,
            edge_type: String,
            evidence_count: i64,
            confidence: f64,
            first_seen: String,
            last_seen: String,
        }
        let source_edges: Vec<SourceEdge> = {
            let mut stmt = self.conn.prepare(
                "SELECT src_entity_id, dst_entity_id, edge_type, evidence_count, confidence,
                        first_seen, last_seen
                 FROM edges
                 WHERE (src_entity_id = ?1 OR dst_entity_id = ?1) AND deleted_at IS NULL",
            )?;
            stmt.query_map(params![source_id], |row| {
                Ok(SourceEdge {
                    src: row.get(0)?,
                    dst: row.get(1)?,
                    edge_type: row.get(2)?,
                    evidence_count: row.get(3)?,
                    confidence: row.get(4)?,
                    first_seen: row.get(5)?,
                    last_seen: row.get(6)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect()
        };

        for edge in &source_edges {
            let other = if edge.src == source_id {
                edge.dst
            } else {
                edge.src
            };
            if other == target_id {
                // Would become a target-target self-loop; drop it.
                continue;
            }
            let (a, b) = if edge.edge_type == "co_mentioned" {
                (target_id.min(other), target_id.max(other))
            } else if edge.src == source_id {
                (target_id, other)
            } else {
                (other, target_id)
            };
            self.conn.execute(
                "INSERT OR IGNORE INTO edges
                     (src_entity_id, dst_entity_id, edge_type, evidence_count, confidence,
                      first_seen, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    a,
                    b,
                    edge.edge_type,
                    edge.evidence_count,
                    edge.confidence,
                    edge.first_seen,
                    edge.last_seen
                ],
            )?;
        }

        self.conn.execute(
            "UPDATE edges SET deleted_at = ?1
             WHERE (src_entity_id = ?2 OR dst_entity_id = ?2) AND deleted_at IS NULL",
            params![now, source_id],
        )?;

        // Soft-delete the source entity.
        self.conn.execute(
            "UPDATE entities SET deleted_at = ?1 WHERE id = ?2",
            params![now, source_id],
        )?;

        self.get_entity(target_id)
    }
}
