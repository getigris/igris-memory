use serde::{Deserialize, Serialize};

use super::observation::Observation;
use super::session::Session;

/// Full export of the memory store for backup/migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportData {
    pub version: u32,
    pub exported_at: String,
    pub observations: Vec<Observation>,
    pub sessions: Vec<Session>,
}

/// Result of an import operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub observations_imported: i64,
    pub observations_skipped: i64,
    pub sessions_imported: i64,
    pub sessions_skipped: i64,
}
