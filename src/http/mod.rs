mod routes;

#[cfg(test)]
#[path = "tests/http_test.rs"]
mod tests;

use crate::db::Database;
use axum::Router;
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

/// Shared application state for HTTP handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Database>>,
}

/// Start the HTTP REST API server.
pub async fn serve(db: Database, host: &str, port: u16) -> anyhow::Result<()> {
    let state = AppState {
        db: Arc::new(Mutex::new(db)),
    };

    let app = router(state);

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("HTTP server listening on http://{addr}");

    axum::serve(listener, app).await?;
    Ok(())
}

/// Build the axum router with all routes. Exposed for testing.
pub fn router(state: AppState) -> Router {
    routes::build(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
}
