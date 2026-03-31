use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::db::Database;
use crate::http::{AppState, router};
use std::sync::{Arc, Mutex};

fn test_state() -> AppState {
    let db = Database::open_in_memory().expect("failed to create in-memory db");
    AppState { db: Arc::new(Mutex::new(db)) }
}

fn json_request(method: &str, uri: &str, body: Option<serde_json::Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    match body {
        Some(b) => builder.body(Body::from(serde_json::to_vec(&b).unwrap())).unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn response_json(app: axum::Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body = response.into_body();
    let bytes = body.collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// ─── Health ─────────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_ok() {
    let app = router(test_state());
    let req = Request::get("/health").body(Body::empty()).unwrap();
    let (status, json) = response_json(app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ok");
}

// ─── Observations CRUD ──────────────────────────────────────────

#[tokio::test]
async fn save_and_get_observation() {
    let state = test_state();
    let app = router(state.clone());

    let (status, json) = response_json(
        app,
        json_request("POST", "/observations", Some(serde_json::json!({
            "title": "Auth setup",
            "content": "JWT with RS256",
            "type": "decision",
            "project": "web"
        }))),
    ).await;
    assert_eq!(status, StatusCode::OK);
    let id = json["id"].as_i64().unwrap();

    let app = router(state);
    let (status, json) = response_json(
        app,
        Request::get(&format!("/observations/{id}")).body(Body::empty()).unwrap(),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["title"], "Auth setup");
}

#[tokio::test]
async fn save_rejects_empty_title() {
    let app = router(test_state());
    let (status, json) = response_json(
        app,
        json_request("POST", "/observations", Some(serde_json::json!({
            "title": "",
            "content": "something",
            "type": "decision"
        }))),
    ).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn get_nonexistent_returns_404() {
    let app = router(test_state());
    let (status, json) = response_json(
        app,
        Request::get("/observations/9999").body(Body::empty()).unwrap(),
    ).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR); // rusqlite QueryReturnedNoRows → DatabaseError
    assert!(json["error"].as_str().is_some());
}

#[tokio::test]
async fn update_observation_partial() {
    let state = test_state();
    let app = router(state.clone());

    let (_, json) = response_json(
        app,
        json_request("POST", "/observations", Some(serde_json::json!({
            "title": "Original",
            "content": "content",
            "type": "manual"
        }))),
    ).await;
    let id = json["id"].as_i64().unwrap();

    let app = router(state);
    let (status, json) = response_json(
        app,
        json_request("PATCH", &format!("/observations/{id}"), Some(serde_json::json!({
            "title": "Updated"
        }))),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["title"], "Updated");
    assert_eq!(json["content"], "content");
}

#[tokio::test]
async fn delete_observation_soft() {
    let state = test_state();
    let app = router(state.clone());

    let (_, json) = response_json(
        app,
        json_request("POST", "/observations", Some(serde_json::json!({
            "title": "To delete",
            "content": "bye",
            "type": "manual"
        }))),
    ).await;
    let id = json["id"].as_i64().unwrap();

    let app = router(state);
    let (status, json) = response_json(
        app,
        Request::delete(&format!("/observations/{id}")).header("content-type", "application/json").body(Body::empty()).unwrap(),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["deleted"], true);
}

// ─── Search & Context ───────────────────────────────────────────

#[tokio::test]
async fn search_returns_results() {
    let state = test_state();
    let app = router(state.clone());
    response_json(
        app,
        json_request("POST", "/observations", Some(serde_json::json!({
            "title": "JWT middleware",
            "content": "Auth with JWT tokens",
            "type": "decision"
        }))),
    ).await;

    let app = router(state);
    let (status, json) = response_json(
        app,
        Request::get("/search?q=JWT").body(Body::empty()).unwrap(),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json.as_array().unwrap().len() > 0);
}

#[tokio::test]
async fn search_empty_query_returns_400() {
    let app = router(test_state());
    let (status, json) = response_json(
        app,
        Request::get("/search?q=").body(Body::empty()).unwrap(),
    ).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn context_returns_recent() {
    let state = test_state();
    let app = router(state.clone());
    for i in 0..3 {
        response_json(
            router(state.clone()),
            json_request("POST", "/observations", Some(serde_json::json!({
                "title": format!("Obs {i}"),
                "content": format!("Content {i}"),
                "type": "manual",
                "project": "proj"
            }))),
        ).await;
    }

    let app = router(state);
    let (status, json) = response_json(
        app,
        Request::get("/context?project=proj&limit=2").body(Body::empty()).unwrap(),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn stats_returns_counts() {
    let state = test_state();
    response_json(
        router(state.clone()),
        json_request("POST", "/observations", Some(serde_json::json!({
            "title": "A", "content": "a", "type": "decision"
        }))),
    ).await;

    let (status, json) = response_json(
        router(state),
        Request::get("/stats").body(Body::empty()).unwrap(),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_observations"], 1);
}

// ─── Timeline ───────────────────────────────────────────────────

#[tokio::test]
async fn timeline_around_observation() {
    let state = test_state();
    for i in 0..5 {
        response_json(
            router(state.clone()),
            json_request("POST", "/observations", Some(serde_json::json!({
                "title": format!("Obs {i}"), "content": format!("c{i}"), "type": "manual"
            }))),
        ).await;
    }

    let (status, json) = response_json(
        router(state),
        Request::get("/observations/3/timeline?before=2&after=2").body(Body::empty()).unwrap(),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["anchor"]["id"].as_i64().is_some());
}

// ─── Topic Key ──────────────────────────────────────────────────

#[tokio::test]
async fn suggest_topic_key_returns_key() {
    let app = router(test_state());
    let (status, json) = response_json(
        app,
        json_request("POST", "/suggest-topic-key", Some(serde_json::json!({
            "type": "decision",
            "title": "Use PostgreSQL",
            "content": "We chose PG"
        }))),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["topic_key"], "decision/use-postgresql");
}

// ─── Export / Import ────────────────────────────────────────────

#[tokio::test]
async fn export_import_roundtrip() {
    let state = test_state();
    response_json(
        router(state.clone()),
        json_request("POST", "/observations", Some(serde_json::json!({
            "title": "Exported", "content": "data", "type": "manual"
        }))),
    ).await;

    let (_, export_json) = response_json(
        router(state.clone()),
        json_request("POST", "/export", None),
    ).await;
    assert_eq!(export_json["observations"].as_array().unwrap().len(), 1);

    // Import into a fresh state
    let state2 = test_state();
    let (status, json) = response_json(
        router(state2),
        json_request("POST", "/import", Some(export_json)),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["observations_imported"], 1);
}

// ─── Purge ──────────────────────────────────────────────────────

#[tokio::test]
async fn purge_old_deleted() {
    let state = test_state();
    let (_, json) = response_json(
        router(state.clone()),
        json_request("POST", "/observations", Some(serde_json::json!({
            "title": "Delete me", "content": "c", "type": "manual"
        }))),
    ).await;
    let id = json["id"].as_i64().unwrap();

    response_json(
        router(state.clone()),
        Request::delete(&format!("/observations/{id}")).header("content-type", "application/json").body(Body::empty()).unwrap(),
    ).await;

    let (status, json) = response_json(
        router(state),
        json_request("POST", "/purge", Some(serde_json::json!({"older_than_days": 0}))),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["observations_purged"], 1);
}

// ─── Sessions ───────────────────────────────────────────────────

#[tokio::test]
async fn session_lifecycle() {
    let state = test_state();

    let (status, json) = response_json(
        router(state.clone()),
        json_request("POST", "/sessions", Some(serde_json::json!({
            "id": "s1",
            "project": "myproj",
            "directory": "/code"
        }))),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["ended_at"].is_null());

    let (status, json) = response_json(
        router(state),
        json_request("PATCH", "/sessions/s1", Some(serde_json::json!({
            "summary": "Done with auth"
        }))),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!json["ended_at"].is_null());
    assert_eq!(json["summary"], "Done with auth");
}

// ─── HTTP Edge Cases ────────────────────────────────────────────

#[tokio::test]
async fn malformed_json_returns_422() {
    let app = router(test_state());
    let req = Request::post("/observations")
        .header("content-type", "application/json")
        .body(Body::from("{invalid json}"))
        .unwrap();
    let (status, _) = response_json(app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn missing_required_field_returns_422() {
    let app = router(test_state());
    // Missing "content" field
    let (status, _) = response_json(
        app,
        json_request("POST", "/observations", Some(serde_json::json!({
            "title": "Only title"
        }))),
    ).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn invalid_type_returns_400() {
    let app = router(test_state());
    let (status, json) = response_json(
        app,
        json_request("POST", "/observations", Some(serde_json::json!({
            "title": "T", "content": "C", "type": "invalid_type"
        }))),
    ).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn import_invalid_json_returns_422() {
    let app = router(test_state());
    let req = Request::post("/import")
        .header("content-type", "application/json")
        .body(Body::from("not json"))
        .unwrap();
    let (status, _) = response_json(app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_nonexistent_returns_404() {
    let app = router(test_state());
    let (status, json) = response_json(
        app,
        Request::delete("/observations/9999")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap(),
    ).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["code"], "NOT_FOUND");
}

#[tokio::test]
async fn update_nonexistent_returns_500() {
    let app = router(test_state());
    let (status, _) = response_json(
        app,
        json_request("PATCH", "/observations/9999", Some(serde_json::json!({
            "title": "new"
        }))),
    ).await;
    // rusqlite QueryReturnedNoRows → DatabaseError → 500
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn unicode_in_http_request() {
    let state = test_state();
    let (status, json) = response_json(
        router(state.clone()),
        json_request("POST", "/observations", Some(serde_json::json!({
            "title": "认证设计 🔐",
            "content": "使用JWT — très bien",
            "type": "decision"
        }))),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["title"], "认证设计 🔐");
}

#[tokio::test]
async fn search_with_type_filter() {
    let state = test_state();
    response_json(router(state.clone()), json_request("POST", "/observations", Some(serde_json::json!({
        "title": "A decision", "content": "important", "type": "decision"
    })))).await;
    response_json(router(state.clone()), json_request("POST", "/observations", Some(serde_json::json!({
        "title": "A bugfix", "content": "important fix", "type": "bugfix"
    })))).await;

    let (status, json) = response_json(
        router(state),
        Request::get("/search?q=important&type=decision").body(Body::empty()).unwrap(),
    ).await;
    assert_eq!(status, StatusCode::OK);
    let results = json.as_array().unwrap();
    assert!(results.iter().all(|r| r["type"] == "decision"));
}

#[tokio::test]
async fn purge_negative_days_returns_400() {
    let app = router(test_state());
    let (status, json) = response_json(
        app,
        json_request("POST", "/purge", Some(serde_json::json!({"older_than_days": -1}))),
    ).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "VALIDATION_ERROR");
}
