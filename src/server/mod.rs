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
    db.lock()
        .map_err(|e| IgrisError::lock(format!("Mutex poisoned: {e}")))
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
        description = "Save a memory. Call this proactively when the user makes a decision, discovers something, fixes a bug, creates a plan, or asks you to remember something. Use topic_key for evolving knowledge — same key updates in place instead of creating duplicates. Wrap secrets in <private>...</private> to auto-redact."
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
        tracing::info!(
            tool = "igris_save",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_search",
        description = "Search memories by keyword or natural language. Returns ranked results with snippets. Use this to find specific past decisions, patterns, or context before making recommendations."
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
        tracing::info!(
            tool = "igris_search",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_get",
        description = "Get the full content of a memory by ID. Use after search or context when you need the complete details of a specific observation."
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
        tracing::info!(
            tool = "igris_get",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_update",
        description = "Update specific fields of an existing memory. Use for corrections. For evolving knowledge, prefer saving with the same topic_key instead."
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
        tracing::info!(
            tool = "igris_update",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_delete",
        description = "Soft-delete a memory. Use for completed plans, outdated info, or memories the user wants removed. Data is kept but excluded from search and context. Use igris_purge later to permanently clean up."
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
                tracing::warn!(
                    tool = "igris_delete",
                    id = args.id,
                    "not found or already deleted"
                );
                err_json(IgrisError::not_found(format!(
                    "Observation {} not found or already deleted",
                    args.id
                )))
            }
            Err(e) => {
                tracing::warn!(tool = "igris_delete", error = %e, "db error");
                err_json(e)
            }
        };
        tracing::info!(
            tool = "igris_delete",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_context",
        description = "Load recent memories. Call this at the START of every conversation to understand what was done in previous sessions. Returns observations ordered by most recently updated."
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
        tracing::info!(
            tool = "igris_context",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_stats",
        description = "Get memory store statistics. Shows total memories, sessions, and breakdowns by type and project."
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
        tracing::info!(
            tool = "igris_stats",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_timeline",
        description = "View the chronological context around a memory. Shows what was saved before and after, useful to understand the sequence of decisions or events."
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
        tracing::info!(
            tool = "igris_timeline",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_suggest_topic_key",
        description = "Generate a consistent topic_key before saving. Ensures related memories share the same key for automatic grouping and in-place updates."
    )]
    fn igris_suggest_topic_key(&self, Parameters(args): Parameters<SuggestTopicKeyArgs>) -> String {
        let key = topic::suggest_topic_key(&args.observation_type, &args.title, &args.content);
        serde_json::json!({ "topic_key": key }).to_string()
    }

    #[tool(
        name = "igris_export",
        description = "Export all memories and sessions as JSON. Use for backup or migration between machines."
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
        tracing::info!(
            tool = "igris_export",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_import",
        description = "Import memories from a JSON export. Deduplicates by content hash — safe to run multiple times."
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
        tracing::info!(
            tool = "igris_import",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_purge",
        description = "Permanently remove old soft-deleted memories. Use to clean up completed plans and outdated entries. Specify days threshold (0 = purge all deleted). Irreversible."
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
        tracing::info!(
            tool = "igris_purge",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_session_start",
        description = "Register a new working session. Sessions group memories by time period and provide continuity between conversations."
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
        tracing::info!(
            tool = "igris_session_start",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_session_end",
        description = "Mark a session as completed with an optional summary."
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
        tracing::info!(
            tool = "igris_session_end",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_session_summary",
        description = "Save a structured summary of what was accomplished. This is the most important memory for continuity — the next session loads it via igris_context. Call this before ending the conversation."
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
        tracing::info!(
            tool = "igris_session_summary",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }
}

#[tool_handler]
impl ServerHandler for IgrisServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "You are connected to Igris Memory, a persistent memory store that survives across sessions \
                 and works across different AI providers (Claude, ChatGPT, Cursor, etc.).\n\n\
                 ## Session Lifecycle\n\
                 1. START: Call igris_context to load recent memories. Review them to understand prior work.\n\
                 2. DURING: Save observations proactively as important things happen.\n\
                 3. END: Call igris_session_summary before the conversation ends.\n\n\
                 ## When to Save\n\
                 - User makes a decision → type: decision\n\
                 - Architecture is designed or changed → type: architecture\n\
                 - A bug is found and fixed → type: bugfix\n\
                 - A reusable pattern emerges → type: pattern\n\
                 - Configuration is set up or changed → type: config\n\
                 - Something unexpected is discovered → type: discovery\n\
                 - A concept is explained or understood → type: learning\n\
                 - An execution plan is created → type: plan (delete when completed)\n\
                 - User explicitly asks to remember something → type: manual\n\n\
                 ## Plans\n\
                 Save execution plans as type 'plan' with a topic_key like 'plan/feature-name'. \
                 Update the plan via topic_key as it evolves. When complete, delete it with igris_delete.\n\n\
                 ## Topic Keys\n\
                 Use topic_key for knowledge that evolves. Same topic_key updates in place.\n\
                 Example: 'architecture/auth' — first: 'JWT', later: 'OAuth2 + PKCE'.\n\
                 Call igris_suggest_topic_key to generate consistent keys.\n\n\
                 ## Privacy\n\
                 Wrap sensitive values in <private>...</private> tags — auto-redacted before storage.\n\n\
                 ## Search vs Context\n\
                 - igris_search: find specific memories by keyword\n\
                 - igris_context: load recent memories chronologically (use at session start)\n\
                 - igris_timeline: see what happened before/after a specific memory",
            )
    }
}
