//! The `BrainStore` trait — the storage contract for the agentic-brain layer.
//!
//! The local backend implements it over SQLite (`crate::db::Database`); a future
//! cloud fork implements the same contract over another engine. Keeping the
//! contract explicit is what makes the local↔cloud fork a backend swap rather
//! than a rewrite. Entity/graph operations are added here as they land per phase.

use crate::db::DbResult;
use crate::models::{Edge, Entity, EntityBrief, EntityNeighbor, Observation};

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

    /// Partially update an entity's mutable fields by id. `slug` is the stable
    /// identity and is never changed here. Requires at least one of
    /// kind/tier/salience to be `Some`, or `add_aliases` to be non-empty.
    fn update_entity(
        &self,
        id: i64,
        kind: Option<&str>,
        tier: Option<i32>,
        salience: Option<f64>,
        add_aliases: &[String],
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

    /// The entity's Timeline: observations that mention it, most recent first.
    fn entity_timeline(&self, entity_id: i64, limit: i64) -> DbResult<Vec<Observation>>;

    /// Regenerate the entity's Compiled Truth from a deterministic template
    /// (mention count, top connections, recent mentions), persist it into
    /// `compiled_truth`/`compiled_at`, and return it. No LLM.
    fn compile_entity_truth(&self, entity_id: i64) -> DbResult<String>;

    /// Assemble a one-call brief: recompiles the truth (so `entity.compiled_truth`
    /// is fresh), then bundles the entity, its top neighbors, and recent timeline.
    fn entity_brief(&self, entity_id: i64) -> DbResult<EntityBrief>;

    /// Find entities whose name/alias matches `query` (substring, normalized),
    /// optionally filtered by kind/project/scope. Strongest (salience, recency) first.
    fn search_entities(
        &self,
        query: &str,
        kind: Option<&str>,
        project: Option<&str>,
        scope: Option<&str>,
        limit: i64,
    ) -> DbResult<Vec<Entity>>;

    /// List entities, most recently updated first, optionally filtered by
    /// kind/project/scope. Use to browse the graph.
    fn list_entities(
        &self,
        kind: Option<&str>,
        project: Option<&str>,
        scope: Option<&str>,
        limit: i64,
    ) -> DbResult<Vec<Entity>>;

    /// Soft-delete an entity (sets `deleted_at`); returns false if not found or
    /// already deleted. The entity is hidden from get/search/list/neighbors.
    fn delete_entity(&self, id: i64) -> DbResult<bool>;

    /// Soft-delete edges of type `relation` between two entities (either
    /// direction). Returns how many edges were removed.
    fn unlink_entities(&self, src_id: i64, dst_id: i64, relation: &str) -> DbResult<i64>;

    /// Fold a duplicate entity (`source_id`) into another (`target_id`):
    /// moves `source`'s aliases, mentions, and edges onto `target`, then
    /// soft-deletes `source`. `target` keeps its identity (id/slug).
    fn merge_entities(&self, source_id: i64, target_id: i64) -> DbResult<Entity>;
}
