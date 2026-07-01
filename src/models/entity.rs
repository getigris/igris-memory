use serde::{Deserialize, Serialize};

/// A first-class knowledge node: a person, company, project, concept, etc.
/// Compiled Truth and Timeline are derived; this struct holds the stored row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: i64,
    pub kind: String,
    pub canonical_name: String,
    pub slug: String,
    pub tier: i32,
    pub salience: f64,
    pub compiled_truth: Option<String>,
    pub compiled_at: Option<String>,
    pub project: Option<String>,
    pub scope: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

/// A typed relation between two entities. Symmetric edges (e.g. `co_mentioned`)
/// are stored once with `src_entity_id < dst_entity_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: i64,
    pub src_entity_id: i64,
    pub dst_entity_id: i64,
    pub edge_type: String,
    pub evidence_count: i64,
    pub confidence: f64,
    pub first_seen: String,
    pub last_seen: String,
    pub deleted_at: Option<String>,
}

/// A neighbor in the entity graph: the connecting edge plus the entity on the
/// other end.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityNeighbor {
    pub edge: Edge,
    pub entity: Entity,
}

/// A normalized alias row for portability (export/import).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityAlias {
    pub entity_id: i64,
    pub alias_normalized: String,
    pub source: Option<String>,
}

/// An observation↔entity mention row for portability (export/import).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mention {
    pub observation_id: i64,
    pub entity_id: i64,
}
