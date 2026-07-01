use crate::errors::IgrisError;
use crate::models::Entity;
use crate::store::BrainStore;
use crate::utils::{entity_slug, normalize_alias, now_utc};
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
        let normalized = normalize_alias(raw);
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
        validation::validate_entity(kind, canonical_name, scope).map_err(IgrisError::validation)?;
        let slug = entity_slug(canonical_name);
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
                params![kind, canonical_name, now, id],
            )?;
            id
        } else {
            self.conn.execute(
                "INSERT INTO entities
                 (kind, canonical_name, slug, project, scope, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![kind, canonical_name, slug, project, scope, now, now],
            )?;
            self.conn.last_insert_rowid()
        };

        // Register canonical name + provided aliases as normalized aliases.
        self.add_alias(entity_id, canonical_name, "canonical")?;
        for alias in aliases {
            self.add_alias(entity_id, alias, "provided")?;
        }

        self.get_entity(entity_id)
    }

    fn get_entity(&self, id: i64) -> DbResult<Entity> {
        Ok(self.conn.query_row(
            &format!(
                "SELECT {} FROM entities WHERE id = ?1 AND deleted_at IS NULL",
                Self::ENTITY_COLS
            ),
            params![id],
            |row| Ok(Self::row_to_entity(row)),
        )?)
    }

    fn get_entity_by_slug(
        &self,
        slug: &str,
        project: Option<&str>,
        scope: &str,
    ) -> DbResult<Entity> {
        Ok(self.conn.query_row(
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
        )?)
    }
}
