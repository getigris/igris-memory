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
