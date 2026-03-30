use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

/// A working session — groups observations by time period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub project: String,
    pub directory: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub summary: Option<String>,
}

/// A search result with relevance ranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    #[serde(flatten)]
    pub observation: Observation,
    pub rank: f64,
    pub snippet: Option<String>,
}

/// Aggregate statistics about the memory store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub total_observations: i64,
    pub total_sessions: i64,
    pub active_sessions: i64,
    pub by_type: HashMap<String, i64>,
    pub by_project: HashMap<String, i64>,
}
