mod args;

use args::*;

use crate::db::Database;
use crate::errors::IgrisError;
use crate::topic;
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
};
use std::sync::{Arc, Mutex};
use std::time::Instant;

// ─── Helpers ────────────────────────────────────────────────────────

fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

fn lock_db(db: &Arc<Mutex<Database>>) -> Result<std::sync::MutexGuard<'_, Database>, IgrisError> {
    db.lock().map_err(|e| IgrisError::lock(format!("Mutex poisoned: {e}")))
}

fn err_json(e: IgrisError) -> String {
    e.to_json()
}

// ─── MCP Server ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IgrisServer {
    db: Arc<Mutex<Database>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl IgrisServer {
    pub fn new(db: Database) -> Self {
        Self {
            db: Arc::new(Mutex::new(db)),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "igris_save",
        description = "Save a memory (observation). Supports deduplication and topic-key upsert for evolving knowledge. Wrap sensitive values in <private>...</private> tags to auto-redact them."
    )]
    fn igris_save(&self, Parameters(args): Parameters<SaveArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.save_observation(
            &args.title,
            &args.content,
            &args.observation_type,
            args.project.as_deref(),
            &args.scope,
            args.topic_key.as_deref(),
            args.tags.as_deref(),
            args.session_id.as_deref(),
        ) {
            Ok(obs) => to_json(&obs),
            Err(e) => {
                tracing::warn!(tool = "igris_save", error = %e, "validation/db error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_save", duration_ms = start.elapsed().as_millis() as u64);
        result
    }

    #[tool(
        name = "igris_search",
        description = "Full-text search across all memories. Returns ranked results with snippets. Use natural language or keywords."
    )]
    fn igris_search(&self, Parameters(args): Parameters<SearchArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.search(
            &args.query,
            args.observation_type.as_deref(),
            args.project.as_deref(),
            args.limit,
        ) {
            Ok(results) => to_json(&results),
            Err(e) => {
                tracing::warn!(tool = "igris_search", error = %e, "validation/db error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_search", duration_ms = start.elapsed().as_millis() as u64);
        result
    }

    #[tool(
        name = "igris_get",
        description = "Retrieve a single memory by ID. Returns the full untruncated content."
    )]
    fn igris_get(&self, Parameters(args): Parameters<GetArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.get_observation(args.id) {
            Ok(obs) => to_json(&obs),
            Err(e) => {
                tracing::warn!(tool = "igris_get", error = %e, "not found or db error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_get", duration_ms = start.elapsed().as_millis() as u64);
        result
    }

    #[tool(
        name = "igris_update",
        description = "Update an existing memory. Only provided fields are changed."
    )]
    fn igris_update(&self, Parameters(args): Parameters<UpdateArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.update_observation(
            args.id,
            args.title.as_deref(),
            args.content.as_deref(),
            args.observation_type.as_deref(),
            args.tags.as_deref(),
            args.topic_key.as_deref(),
        ) {
            Ok(obs) => to_json(&obs),
            Err(e) => {
                tracing::warn!(tool = "igris_update", error = %e, "validation/db error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_update", duration_ms = start.elapsed().as_millis() as u64);
        result
    }

    #[tool(
        name = "igris_delete",
        description = "Soft-delete a memory. The data is kept but excluded from searches and context."
    )]
    fn igris_delete(&self, Parameters(args): Parameters<DeleteArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.delete_observation(args.id) {
            Ok(true) => r#"{"deleted": true}"#.to_string(),
            Ok(false) => {
                tracing::warn!(tool = "igris_delete", id = args.id, "not found or already deleted");
                err_json(IgrisError::not_found(format!("Observation {} not found or already deleted", args.id)))
            }
            Err(e) => {
                tracing::warn!(tool = "igris_delete", error = %e, "db error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_delete", duration_ms = start.elapsed().as_millis() as u64);
        result
    }

    #[tool(
        name = "igris_context",
        description = "Load recent memories for context. Call this at the start of each session to recall what was done previously."
    )]
    fn igris_context(&self, Parameters(args): Parameters<ContextArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.recent_context(args.project.as_deref(), args.limit) {
            Ok(observations) => to_json(&observations),
            Err(e) => {
                tracing::warn!(tool = "igris_context", error = %e, "db error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_context", duration_ms = start.elapsed().as_millis() as u64);
        result
    }

    #[tool(
        name = "igris_stats",
        description = "Get memory store statistics: totals, counts by type, and counts by project."
    )]
    fn igris_stats(&self) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.stats() {
            Ok(stats) => to_json(&stats),
            Err(e) => {
                tracing::warn!(tool = "igris_stats", error = %e, "db error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_stats", duration_ms = start.elapsed().as_millis() as u64);
        result
    }

    #[tool(
        name = "igris_timeline",
        description = "Get a chronological view around a specific memory. Shows observations created before and after the anchor, useful for understanding the context in which a decision was made."
    )]
    fn igris_timeline(&self, Parameters(args): Parameters<TimelineArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.timeline(args.observation_id, args.before, args.after) {
            Ok(tl) => to_json(&tl),
            Err(e) => {
                tracing::warn!(tool = "igris_timeline", error = %e, "not found or db error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_timeline", duration_ms = start.elapsed().as_millis() as u64);
        result
    }

    #[tool(
        name = "igris_suggest_topic_key",
        description = "Suggest a stable topic_key for an observation based on its type and title. Use this to get a consistent key before saving, so related memories can be grouped and updated via topic-key upsert."
    )]
    fn igris_suggest_topic_key(&self, Parameters(args): Parameters<SuggestTopicKeyArgs>) -> String {
        let key = topic::suggest_topic_key(&args.observation_type, &args.title, &args.content);
        serde_json::json!({ "topic_key": key }).to_string()
    }

    #[tool(
        name = "igris_export",
        description = "Export all memories and sessions as a JSON string. Useful for backup, migration between machines, or version control."
    )]
    fn igris_export(&self) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.export_all() {
            Ok(data) => to_json(&data),
            Err(e) => {
                tracing::warn!(tool = "igris_export", error = %e, "db error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_export", duration_ms = start.elapsed().as_millis() as u64);
        result
    }

    #[tool(
        name = "igris_import",
        description = "Import memories and sessions from a JSON export. Deduplicates by content hash — safe to run multiple times. Soft-deleted entries are skipped."
    )]
    fn igris_import(&self, Parameters(args): Parameters<ImportArgs>) -> String {
        let start = Instant::now();
        let data: crate::models::ExportData = match serde_json::from_str(&args.data) {
            Ok(d) => d,
            Err(e) => return err_json(IgrisError::validation(format!("Invalid JSON: {e}"))),
        };
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.import_data(&data) {
            Ok(r) => to_json(&r),
            Err(e) => {
                tracing::warn!(tool = "igris_import", error = %e, "import error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_import", duration_ms = start.elapsed().as_millis() as u64);
        result
    }

    #[tool(
        name = "igris_purge",
        description = "Permanently delete observations that were soft-deleted more than N days ago. Runs VACUUM to reclaim disk space. This action is irreversible."
    )]
    fn igris_purge(&self, Parameters(args): Parameters<PurgeArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.purge(args.older_than_days) {
            Ok(r) => to_json(&r),
            Err(e) => {
                tracing::warn!(tool = "igris_purge", error = %e, "purge error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_purge", duration_ms = start.elapsed().as_millis() as u64);
        result
    }

    #[tool(
        name = "igris_session_start",
        description = "Register the start of a working session. Sessions group memories by time period."
    )]
    fn igris_session_start(&self, Parameters(args): Parameters<SessionStartArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.start_session(&args.id, &args.project, args.directory.as_deref()) {
            Ok(session) => to_json(&session),
            Err(e) => {
                tracing::warn!(tool = "igris_session_start", error = %e, "validation/db error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_session_start", duration_ms = start.elapsed().as_millis() as u64);
        result
    }

    #[tool(
        name = "igris_session_end",
        description = "Mark a session as completed. Optionally include a summary of what was accomplished."
    )]
    fn igris_session_end(&self, Parameters(args): Parameters<SessionEndArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.end_session(&args.id, args.summary.as_deref()) {
            Ok(session) => to_json(&session),
            Err(e) => {
                tracing::warn!(tool = "igris_session_end", error = %e, "validation/db error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_session_end", duration_ms = start.elapsed().as_millis() as u64);
        result
    }

    #[tool(
        name = "igris_session_summary",
        description = "Save a structured summary of the current session. This is the most valuable memory for continuity — the next session loads it via igris_context to know exactly what was done before."
    )]
    fn igris_session_summary(&self, Parameters(args): Parameters<SessionSummaryArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.save_session_summary(&args.content, &args.project) {
            Ok(session) => to_json(&session),
            Err(e) => {
                tracing::warn!(tool = "igris_session_summary", error = %e, "validation/db error");
                err_json(e)
            }
        };
        tracing::info!(tool = "igris_session_summary", duration_ms = start.elapsed().as_millis() as u64);
        result
    }
}

#[tool_handler]
impl ServerHandler for IgrisServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Igris Memory — persistent memory for AI coding agents. \
                 Use igris_save to store decisions, patterns, and learnings. \
                 Use igris_search to find relevant memories. \
                 Use igris_context at session start to load recent context.",
            )
    }
}
