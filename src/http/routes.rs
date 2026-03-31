use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::errors::IgrisError;
use crate::models::*;
use crate::topic;

use super::AppState;

type Result<T> = std::result::Result<T, IgrisError>;

fn lock_db(
    state: &AppState,
) -> std::result::Result<std::sync::MutexGuard<'_, crate::db::Database>, IgrisError> {
    state
        .db
        .lock()
        .map_err(|e| IgrisError::lock(format!("Mutex poisoned: {e}")))
}

pub fn build(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/observations", post(save_observation))
        .route("/observations/:id", get(get_observation))
        .route("/observations/:id", patch(update_observation))
        .route("/observations/:id", delete(delete_observation))
        .route("/observations/:id/timeline", get(timeline))
        .route("/search", get(search))
        .route("/context", get(context))
        .route("/stats", get(stats))
        .route("/suggest-topic-key", post(suggest_topic_key))
        .route("/export", post(export))
        .route("/import", post(import))
        .route("/purge", post(purge))
        .route("/sessions", post(session_start))
        .route("/sessions/:id", patch(session_end))
        .route("/sessions/summary", post(session_summary))
        .with_state(state)
}

// ─── Health ─────────────────────────────────────────────────────

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

// ─── Observations ───────────────────────────────────────────────

#[derive(Deserialize)]
struct SaveBody {
    title: String,
    content: String,
    #[serde(rename = "type", default = "default_type")]
    observation_type: String,
    project: Option<String>,
    #[serde(default = "default_scope")]
    scope: String,
    topic_key: Option<String>,
    tags: Option<Vec<String>>,
    session_id: Option<String>,
}

fn default_type() -> String {
    "manual".to_string()
}
fn default_scope() -> String {
    "project".to_string()
}

async fn save_observation(
    State(state): State<AppState>,
    Json(body): Json<SaveBody>,
) -> Result<Json<Observation>> {
    let db = lock_db(&state)?;
    let obs = db.save_observation(
        &body.title,
        &body.content,
        &body.observation_type,
        body.project.as_deref(),
        &body.scope,
        body.topic_key.as_deref(),
        body.tags.as_deref(),
        body.session_id.as_deref(),
    )?;
    Ok(Json(obs))
}

async fn get_observation(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Observation>> {
    let db = lock_db(&state)?;
    let obs = db.get_observation(id)?;
    Ok(Json(obs))
}

#[derive(Deserialize)]
struct UpdateBody {
    title: Option<String>,
    content: Option<String>,
    #[serde(rename = "type")]
    observation_type: Option<String>,
    tags: Option<Vec<String>>,
    topic_key: Option<String>,
}

async fn update_observation(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<Observation>> {
    let db = lock_db(&state)?;
    let obs = db.update_observation(
        id,
        body.title.as_deref(),
        body.content.as_deref(),
        body.observation_type.as_deref(),
        body.tags.as_deref(),
        body.topic_key.as_deref(),
    )?;
    Ok(Json(obs))
}

async fn delete_observation(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    let db = lock_db(&state)?;
    match db.delete_observation(id)? {
        true => Ok(Json(serde_json::json!({"deleted": true}))),
        false => Err(IgrisError::not_found(format!(
            "Observation {id} not found or already deleted"
        ))),
    }
}

// ─── Search & Context ───────────────────────────────────────────

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(rename = "type")]
    observation_type: Option<String>,
    project: Option<String>,
    limit: Option<i64>,
}

async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>> {
    let db = lock_db(&state)?;
    let results = db.search(
        &params.q,
        params.observation_type.as_deref(),
        params.project.as_deref(),
        params.limit,
    )?;
    Ok(Json(results))
}

#[derive(Deserialize)]
struct ContextQuery {
    project: Option<String>,
    limit: Option<i64>,
}

async fn context(
    State(state): State<AppState>,
    Query(params): Query<ContextQuery>,
) -> Result<Json<Vec<Observation>>> {
    let db = lock_db(&state)?;
    let obs = db.recent_context(params.project.as_deref(), params.limit)?;
    Ok(Json(obs))
}

async fn stats(State(state): State<AppState>) -> Result<Json<Stats>> {
    let db = lock_db(&state)?;
    let s = db.stats()?;
    Ok(Json(s))
}

// ─── Timeline ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct TimelineQuery {
    before: Option<i64>,
    after: Option<i64>,
}

async fn timeline(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<TimelineQuery>,
) -> Result<Json<Timeline>> {
    let db = lock_db(&state)?;
    let tl = db.timeline(id, params.before, params.after)?;
    Ok(Json(tl))
}

// ─── Topic Key ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct SuggestTopicKeyBody {
    #[serde(rename = "type")]
    observation_type: String,
    title: String,
    content: String,
}

async fn suggest_topic_key(Json(body): Json<SuggestTopicKeyBody>) -> Json<serde_json::Value> {
    let key = topic::suggest_topic_key(&body.observation_type, &body.title, &body.content);
    Json(serde_json::json!({"topic_key": key}))
}

// ─── Export / Import / Purge ────────────────────────────────────

async fn export(State(state): State<AppState>) -> Result<Json<ExportData>> {
    let db = lock_db(&state)?;
    let data = db.export_all()?;
    Ok(Json(data))
}

async fn import(
    State(state): State<AppState>,
    Json(data): Json<ExportData>,
) -> Result<Json<ImportResult>> {
    let db = lock_db(&state)?;
    let result = db.import_data(&data)?;
    Ok(Json(result))
}

#[derive(Deserialize)]
struct PurgeBody {
    older_than_days: i64,
}

async fn purge(
    State(state): State<AppState>,
    Json(body): Json<PurgeBody>,
) -> Result<Json<PurgeResult>> {
    let db = lock_db(&state)?;
    let result = db.purge(body.older_than_days)?;
    Ok(Json(result))
}

// ─── Sessions ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct SessionStartBody {
    id: String,
    project: String,
    directory: Option<String>,
}

async fn session_start(
    State(state): State<AppState>,
    Json(body): Json<SessionStartBody>,
) -> Result<Json<Session>> {
    let db = lock_db(&state)?;
    let session = db.start_session(&body.id, &body.project, body.directory.as_deref())?;
    Ok(Json(session))
}

#[derive(Deserialize)]
struct SessionEndBody {
    summary: Option<String>,
}

async fn session_end(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SessionEndBody>,
) -> Result<Json<Session>> {
    let db = lock_db(&state)?;
    let session = db.end_session(&id, body.summary.as_deref())?;
    Ok(Json(session))
}

#[derive(Deserialize)]
struct SessionSummaryBody {
    content: String,
    project: String,
}

async fn session_summary(
    State(state): State<AppState>,
    Json(body): Json<SessionSummaryBody>,
) -> Result<Json<Session>> {
    let db = lock_db(&state)?;
    let session = db.save_session_summary(&body.content, &body.project)?;
    Ok(Json(session))
}
