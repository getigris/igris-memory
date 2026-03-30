use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result of a purge operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgeResult {
    pub observations_purged: i64,
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
