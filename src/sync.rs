use crate::db::Database;
use crate::errors::IgrisError;
use crate::models::{ExportData, ImportResult, Observation, Session};
use crate::utils::now_utc;
use serde::{Deserialize, Serialize};
use std::path::Path;

const CHUNK_SIZE: usize = 100;

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub exported_at: String,
    pub machine_id: String,
    pub observation_count: usize,
    pub session_count: usize,
    pub chunk_size: usize,
    pub chunk_count: usize,
}

/// Export the database to a directory of chunked JSON files.
pub fn export_to_dir(db: &Database, dir: &Path) -> Result<Manifest, IgrisError> {
    let data = db.export_all()?;

    // Create directory structure
    std::fs::create_dir_all(dir.join("observations"))
        .map_err(|e| IgrisError::database(format!("Failed to create sync dir: {e}")))?;

    // Write sessions
    let sessions_json = serde_json::to_string_pretty(&data.sessions)
        .map_err(|e| IgrisError::database(format!("Failed to serialize sessions: {e}")))?;
    std::fs::write(dir.join("sessions.json"), sessions_json)
        .map_err(|e| IgrisError::database(format!("Failed to write sessions: {e}")))?;

    // Write observation chunks
    let chunks: Vec<&[Observation]> = data.observations.chunks(CHUNK_SIZE).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        let filename = format!("chunk_{i:04}.json");
        let json = serde_json::to_string_pretty(chunk)
            .map_err(|e| IgrisError::database(format!("Failed to serialize chunk: {e}")))?;
        std::fs::write(dir.join("observations").join(&filename), json)
            .map_err(|e| IgrisError::database(format!("Failed to write chunk: {e}")))?;
    }

    // Write entity-graph collections (Fase 0b-port).
    let entities_json = serde_json::to_string_pretty(&data.entities)
        .map_err(|e| IgrisError::database(format!("Failed to serialize entities: {e}")))?;
    std::fs::write(dir.join("entities.json"), entities_json)
        .map_err(|e| IgrisError::database(format!("Failed to write entities: {e}")))?;

    let aliases_json = serde_json::to_string_pretty(&data.entity_aliases)
        .map_err(|e| IgrisError::database(format!("Failed to serialize aliases: {e}")))?;
    std::fs::write(dir.join("entity_aliases.json"), aliases_json)
        .map_err(|e| IgrisError::database(format!("Failed to write aliases: {e}")))?;

    let edges_json = serde_json::to_string_pretty(&data.edges)
        .map_err(|e| IgrisError::database(format!("Failed to serialize edges: {e}")))?;
    std::fs::write(dir.join("edges.json"), edges_json)
        .map_err(|e| IgrisError::database(format!("Failed to write edges: {e}")))?;

    let mentions_json = serde_json::to_string_pretty(&data.mentions)
        .map_err(|e| IgrisError::database(format!("Failed to serialize mentions: {e}")))?;
    std::fs::write(dir.join("mentions.json"), mentions_json)
        .map_err(|e| IgrisError::database(format!("Failed to write mentions: {e}")))?;

    // Write manifest
    let machine_id = hostname().unwrap_or_else(|| "unknown".to_string());
    let manifest = Manifest {
        version: data.version,
        exported_at: now_utc(),
        machine_id,
        observation_count: data.observations.len(),
        session_count: data.sessions.len(),
        chunk_size: CHUNK_SIZE,
        chunk_count: chunks.len(),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| IgrisError::database(format!("Failed to serialize manifest: {e}")))?;
    std::fs::write(dir.join("manifest.json"), manifest_json)
        .map_err(|e| IgrisError::database(format!("Failed to write manifest: {e}")))?;

    Ok(manifest)
}

/// Import data from a sync directory into the database.
pub fn import_from_dir(db: &Database, dir: &Path) -> Result<ImportResult, IgrisError> {
    // Read manifest
    let manifest_path = dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(IgrisError::validation(
            "Sync directory has no manifest.json",
        ));
    }
    let manifest_str = std::fs::read_to_string(&manifest_path)
        .map_err(|e| IgrisError::database(format!("Failed to read manifest: {e}")))?;
    let manifest: Manifest = serde_json::from_str(&manifest_str)
        .map_err(|e| IgrisError::validation(format!("Invalid manifest: {e}")))?;

    // Read sessions
    let sessions: Vec<Session> = {
        let path = dir.join("sessions.json");
        if path.exists() {
            let s = std::fs::read_to_string(&path)
                .map_err(|e| IgrisError::database(format!("Failed to read sessions: {e}")))?;
            serde_json::from_str(&s)
                .map_err(|e| IgrisError::validation(format!("Invalid sessions: {e}")))?
        } else {
            Vec::new()
        }
    };

    // Read observation chunks
    let mut observations: Vec<Observation> = Vec::new();
    for i in 0..manifest.chunk_count {
        let filename = format!("chunk_{i:04}.json");
        let path = dir.join("observations").join(&filename);
        let s = std::fs::read_to_string(&path)
            .map_err(|e| IgrisError::database(format!("Failed to read {filename}: {e}")))?;
        let chunk: Vec<Observation> = serde_json::from_str(&s)
            .map_err(|e| IgrisError::validation(format!("Invalid chunk {filename}: {e}")))?;
        observations.extend(chunk);
    }

    // Read entity-graph collections (absent in pre-0b-port sync dirs → empty).
    fn read_json_vec<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Vec<T>, IgrisError> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let s = std::fs::read_to_string(path)
            .map_err(|e| IgrisError::database(format!("Failed to read {path:?}: {e}")))?;
        serde_json::from_str(&s)
            .map_err(|e| IgrisError::validation(format!("Invalid {path:?}: {e}")))
    }

    let entities = read_json_vec(&dir.join("entities.json"))?;
    let entity_aliases = read_json_vec(&dir.join("entity_aliases.json"))?;
    let edges = read_json_vec(&dir.join("edges.json"))?;
    let mentions = read_json_vec(&dir.join("mentions.json"))?;

    // Reconstruct ExportData and use existing import logic
    let data = ExportData {
        version: manifest.version,
        exported_at: manifest.exported_at,
        observations,
        sessions,
        entities,
        entity_aliases,
        edges,
        mentions,
    };

    db.import_data(&data)
}

fn hostname() -> Option<String> {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
}

#[cfg(test)]
#[path = "tests/sync_test.rs"]
mod tests;
