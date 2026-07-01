//! The `BrainStore` trait — the storage contract for the agentic-brain layer.
//!
//! The local backend implements it over SQLite (`crate::db::Database`); a future
//! cloud fork implements the same contract over another engine. Keeping the
//! contract explicit is what makes the local↔cloud fork a backend swap rather
//! than a rewrite. Entity/graph operations are added here as they land per phase.

use crate::db::DbResult;
use crate::models::Entity;

/// Storage contract for the entity/graph layer.
#[allow(dead_code)] // TODO(fase-0a): remove once used in Task 5
pub trait BrainStore {
    /// Create an entity, or update it in place if one with the same slug already
    /// exists (within the same project + scope). Registers `canonical_name` and
    /// every provided alias as normalized aliases.
    fn upsert_entity(
        &self,
        kind: &str,
        canonical_name: &str,
        aliases: &[String],
        project: Option<&str>,
        scope: &str,
    ) -> DbResult<Entity>;

    /// Fetch a single entity by numeric id.
    fn get_entity(&self, id: i64) -> DbResult<Entity>;

    /// Fetch a single entity by slug within a project + scope.
    fn get_entity_by_slug(
        &self,
        slug: &str,
        project: Option<&str>,
        scope: &str,
    ) -> DbResult<Entity>;
}
