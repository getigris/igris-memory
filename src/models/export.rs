use serde::{Deserialize, Serialize};

use super::entity::{Edge, Entity, EntityAlias, Mention};
use super::observation::Observation;
use super::session::Session;

/// Full export of the memory store for backup/migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportData {
    pub version: u32,
    pub exported_at: String,
    pub observations: Vec<Observation>,
    pub sessions: Vec<Session>,
    #[serde(default)]
    pub entities: Vec<Entity>,
    #[serde(default)]
    pub entity_aliases: Vec<EntityAlias>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub mentions: Vec<Mention>,
}

/// Result of an import operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub observations_imported: i64,
    pub observations_skipped: i64,
    pub sessions_imported: i64,
    pub sessions_skipped: i64,
    #[serde(default)]
    pub entities_imported: i64,
    #[serde(default)]
    pub entities_skipped: i64,
    #[serde(default)]
    pub edges_imported: i64,
    #[serde(default)]
    pub mentions_imported: i64,
}
