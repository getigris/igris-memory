pub mod args;

use args::*;

use crate::db::Database;
use crate::errors::IgrisError;
use crate::store::BrainStore;
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
            Ok(obs) => {
                if let Some(mentions) = args.mentions.as_deref()
                    && !mentions.is_empty()
                    && let Err(e) =
                        db.record_mentions(obs.id, mentions, args.project.as_deref(), &args.scope)
                {
                    tracing::warn!(tool = "igris_save", error = %e, "mention wiring failed");
                }
                to_json(&obs)
            }
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
        name = "igris_entity_upsert",
        description = "Create or update a first-class entity (person, company, project, concept, ...). Idempotent by name within a project+scope: calling again with the same name updates in place and registers new aliases. Use this to declare the who/what your memories are about."
    )]
    fn igris_entity_upsert(&self, Parameters(args): Parameters<EntityUpsertArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.upsert_entity(
            &args.kind,
            &args.name,
            args.aliases.as_deref().unwrap_or(&[]),
            args.project.as_deref(),
            &args.scope,
        ) {
            Ok(entity) => to_json(&entity),
            Err(e) => {
                tracing::warn!(tool = "igris_entity_upsert", error = %e, "validation/db error");
                err_json(e)
            }
        };
        tracing::info!(
            tool = "igris_entity_upsert",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_entity_get",
        description = "Fetch a single entity by id or by slug. When using slug, pass the same project+scope the entity was created under."
    )]
    fn igris_entity_get(&self, Parameters(args): Parameters<EntityGetArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let lookup = match (args.id, args.slug.as_deref()) {
            (Some(id), _) => db.get_entity(id),
            (None, Some(slug)) => db.get_entity_by_slug(slug, args.project.as_deref(), &args.scope),
            (None, None) => Err(IgrisError::validation(
                "igris_entity_get requires either 'id' or 'slug'".to_string(),
            )),
        };
        let result = match lookup {
            Ok(entity) => to_json(&entity),
            Err(e) => {
                tracing::warn!(tool = "igris_entity_get", error = %e, "not found or db error");
                err_json(e)
            }
        };
        tracing::info!(
            tool = "igris_entity_get",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_entity_update",
        description = "Partially update an entity's mutable fields by id — kind, tier, salience, and/or additional aliases. The slug (stable identity) never changes. Requires at least one field to change. Complements igris_entity_upsert, which is keyed by name."
    )]
    fn igris_entity_update(&self, Parameters(args): Parameters<EntityUpdateArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.update_entity(
            args.id,
            args.kind.as_deref(),
            args.tier,
            args.salience,
            args.add_aliases.as_deref().unwrap_or(&[]),
        ) {
            Ok(entity) => to_json(&entity),
            Err(e) => {
                tracing::warn!(tool = "igris_entity_update", error = %e, "validation/db error");
                err_json(e)
            }
        };
        tracing::info!(
            tool = "igris_entity_update",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_entity_link",
        description = "Create or strengthen a typed relation between two entities (by id), e.g. 'works_at', 'founded'. Idempotent: repeating the same link increments its evidence."
    )]
    fn igris_entity_link(&self, Parameters(args): Parameters<EntityLinkArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.upsert_edge(args.src_id, args.dst_id, &args.relation) {
            Ok(edge) => to_json(&edge),
            Err(e) => {
                tracing::warn!(tool = "igris_entity_link", error = %e, "db error");
                err_json(e)
            }
        };
        tracing::info!(
            tool = "igris_entity_link",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_entity_neighbors",
        description = "List an entity's graph neighbors (connected entities + the relation), strongest connections first."
    )]
    fn igris_entity_neighbors(&self, Parameters(args): Parameters<EntityNeighborsArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.entity_neighbors(args.entity_id, args.limit.unwrap_or(20)) {
            Ok(neighbors) => to_json(&neighbors),
            Err(e) => {
                tracing::warn!(tool = "igris_entity_neighbors", error = %e, "db error");
                err_json(e)
            }
        };
        tracing::info!(
            tool = "igris_entity_neighbors",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_entity_timeline",
        description = "List the observations that mention an entity, most recent first — the entity's chronological history."
    )]
    fn igris_entity_timeline(&self, Parameters(args): Parameters<EntityTimelineArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.entity_timeline(args.entity_id, args.limit.unwrap_or(20)) {
            Ok(obs) => to_json(&obs),
            Err(e) => {
                tracing::warn!(tool = "igris_entity_timeline", error = %e, "db error");
                err_json(e)
            }
        };
        tracing::info!(
            tool = "igris_entity_timeline",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_brief",
        description = "Get a one-call brief for an entity (by id or slug): a freshly compiled summary, its strongest connections, and its recent mentions with citations. Use this to load everything known about a person/company/project at once."
    )]
    fn igris_brief(&self, Parameters(args): Parameters<BriefArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let entity_id = match (args.id, args.slug.as_deref()) {
            (Some(id), _) => Ok(id),
            (None, Some(slug)) => db
                .get_entity_by_slug(slug, args.project.as_deref(), &args.scope)
                .map(|e| e.id),
            (None, None) => Err(IgrisError::validation(
                "igris_brief requires either 'id' or 'slug'".to_string(),
            )),
        };
        let result = match entity_id.and_then(|id| db.entity_brief(id)) {
            Ok(brief) => to_json(&brief),
            Err(e) => {
                tracing::warn!(tool = "igris_brief", error = %e, "not found or db error");
                err_json(e)
            }
        };
        tracing::info!(
            tool = "igris_brief",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_entity_search",
        description = "Find entities by name or alias (substring, case-insensitive), optionally filtered by kind. Use to discover an entity's id/slug before igris_brief, igris_entity_neighbors, etc."
    )]
    fn igris_entity_search(&self, Parameters(args): Parameters<EntitySearchArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.search_entities(
            &args.query,
            args.kind.as_deref(),
            args.project.as_deref(),
            args.scope.as_deref(),
            args.limit.unwrap_or(20),
        ) {
            Ok(entities) => to_json(&entities),
            Err(e) => {
                tracing::warn!(tool = "igris_entity_search", error = %e, "db error");
                err_json(e)
            }
        };
        tracing::info!(
            tool = "igris_entity_search",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_entity_list",
        description = "Browse entities, most recently updated first, optionally filtered by kind/project/scope. Use at session start or to survey what the brain knows."
    )]
    fn igris_entity_list(&self, Parameters(args): Parameters<EntityListArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.list_entities(
            args.kind.as_deref(),
            args.project.as_deref(),
            args.scope.as_deref(),
            args.limit.unwrap_or(20),
        ) {
            Ok(entities) => to_json(&entities),
            Err(e) => {
                tracing::warn!(tool = "igris_entity_list", error = %e, "db error");
                err_json(e)
            }
        };
        tracing::info!(
            tool = "igris_entity_list",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_entity_delete",
        description = "Soft-delete an entity by id. It is hidden from search/list/get/neighbors but data is retained."
    )]
    fn igris_entity_delete(&self, Parameters(args): Parameters<EntityDeleteArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.delete_entity(args.id) {
            Ok(true) => r#"{"deleted": true}"#.to_string(),
            Ok(false) => err_json(IgrisError::not_found(format!(
                "Entity {} not found or already deleted",
                args.id
            ))),
            Err(e) => {
                tracing::warn!(tool = "igris_entity_delete", error = %e, "db error");
                err_json(e)
            }
        };
        tracing::info!(
            tool = "igris_entity_delete",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
    }

    #[tool(
        name = "igris_entity_unlink",
        description = "Remove a typed relation between two entities (either direction). Returns how many edges were removed."
    )]
    fn igris_entity_unlink(&self, Parameters(args): Parameters<EntityUnlinkArgs>) -> String {
        let start = Instant::now();
        let db = match lock_db(&self.db) {
            Ok(db) => db,
            Err(e) => return err_json(e),
        };
        let result = match db.unlink_entities(args.src_id, args.dst_id, &args.relation) {
            Ok(n) => serde_json::json!({ "removed": n }).to_string(),
            Err(e) => {
                tracing::warn!(tool = "igris_entity_unlink", error = %e, "db error");
                err_json(e)
            }
        };
        tracing::info!(
            tool = "igris_entity_unlink",
            duration_ms = start.elapsed().as_millis() as u64
        );
        result
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
                 - igris_timeline: see what happened before/after a specific memory\n\n\
                 ## Entities (the knowledge graph)\n\
                 - Declare the who/what your memories are about with igris_entity_upsert (people, companies, projects, concepts).\n\
                 - When you save, pass mentions: [\"Name\", ...] to igris_save — unknown names auto-create stub entities, and entities mentioned together get linked automatically.\n\
                 - Discover entities with igris_entity_search (by name/alias) or igris_entity_list (browse); you don't need to know ids in advance.\n\
                 - Load everything about one entity with igris_brief; see its history with igris_entity_timeline and its connections with igris_entity_neighbors.",
            )
    }
}
