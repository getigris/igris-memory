use serde::{Deserialize, Serialize};

/// A single memory unit stored by Igris.
/// Represents a decision, bugfix, pattern, or any knowledge worth persisting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: i64,
    pub session_id: Option<String>,
    #[serde(rename = "type")]
    pub observation_type: String,
    pub title: String,
    pub content: String,
    pub project: Option<String>,
    pub scope: String,
    pub topic_key: Option<String>,
    pub tags: Option<Vec<String>>,
    pub revision_count: i32,
    pub duplicate_count: i32,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

/// A search result with relevance ranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    #[serde(flatten)]
    pub observation: Observation,
    pub rank: f64,
    pub snippet: Option<String>,
}

/// Chronological view around an observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub anchor: Observation,
    pub before: Vec<Observation>,
    pub after: Vec<Observation>,
}
