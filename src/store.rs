//! The `BrainStore` trait — the storage contract for the agentic-brain layer.
//!
//! The local backend implements it over SQLite (`crate::db::Database`); a future
//! cloud fork implements the same contract over another engine. Keeping the
//! contract explicit is what makes the local↔cloud fork a backend swap rather
//! than a rewrite. Entity/graph operations are added here as they land per phase.

use crate::db::DbResult;
use crate::models::{Edge, Entity, EntityNeighbor};

/// Storage contract for the entity/graph layer.
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

    /// Link an observation to an entity (idempotent).
    fn add_mention(&self, observation_id: i64, entity_id: i64) -> DbResult<()>;

    /// Resolve a mention string to an existing entity by normalized alias
    /// (within project + scope), or create a tier-3 stub entity (`kind="other"`)
    /// when none matches. Deterministic — no LLM.
    fn resolve_or_stub_entity(
        &self,
        mention: &str,
        project: Option<&str>,
        scope: &str,
    ) -> DbResult<Entity>;

    /// Create or strengthen a typed edge between two entities. Increments
    /// `evidence_count` and refreshes `last_seen` when the edge already exists.
    fn upsert_edge(
        &self,
        src_entity_id: i64,
        dst_entity_id: i64,
        edge_type: &str,
    ) -> DbResult<Edge>;

    /// Resolve every mention to an entity (auto-stubbing unknowns), link each to
    /// the observation, and create `co_mentioned` edges between every distinct
    /// pair. Returns the resolved entities (deduplicated, in first-seen order).
    fn record_mentions(
        &self,
        observation_id: i64,
        mentions: &[String],
        project: Option<&str>,
        scope: &str,
    ) -> DbResult<Vec<Entity>>;

    /// Return an entity's neighbors (connecting edge + entity on the other end),
    /// strongest edges first, capped at `limit`.
    fn entity_neighbors(&self, entity_id: i64, limit: i64) -> DbResult<Vec<EntityNeighbor>>;
}
