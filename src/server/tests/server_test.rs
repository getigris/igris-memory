use rmcp::model::{
    CallToolRequestParams, LoggingLevel, LoggingMessageNotificationParam,
    ProgressNotificationParam, SetLevelRequestParams,
};
use rmcp::service::NotificationContext;
use rmcp::{ClientHandler, RoleClient, ServiceExt};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

use crate::db::Database;
use crate::server::IgrisServer;

#[derive(Clone, Default)]
struct Collected {
    logs: Arc<Mutex<Vec<LoggingMessageNotificationParam>>>,
    progress: Arc<Mutex<Vec<ProgressNotificationParam>>>,
}

struct TestClient {
    collected: Collected,
    signal: Arc<Notify>,
}

impl ClientHandler for TestClient {
    async fn on_logging_message(
        &self,
        params: LoggingMessageNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        self.collected.logs.lock().unwrap().push(params);
        self.signal.notify_one();
    }

    async fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        self.collected.progress.lock().unwrap().push(params);
        self.signal.notify_one();
    }
}

/// Notifications are fired via `tokio::spawn` on the server side (best-effort,
/// never awaited by the tool), so poll against `signal` up to `timeout` instead
/// of racing the tool's return value.
async fn wait_until(signal: &Notify, timeout: Duration, cond: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + timeout;
    while !cond() {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return;
        }
        let _ = tokio::time::timeout(remaining, signal.notified()).await;
    }
}

fn obj(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    value.as_object().unwrap().clone()
}

#[tokio::test]
async fn logs_and_progress_notifications() -> anyhow::Result<()> {
    let db = Database::open_in_memory()?;
    let server = IgrisServer::with_embedder(db, None);
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let running = server.serve(server_transport).await?;
        running.waiting().await?;
        anyhow::Ok(())
    });

    let signal = Arc::new(Notify::new());
    let collected = Collected::default();
    let client = TestClient {
        collected: collected.clone(),
        signal: signal.clone(),
    }
    .serve(client_transport)
    .await?;

    // --- Baseline: default level (Info) sees exactly a start+end log, no progress. ---
    let response = client
        .call_tool(
            CallToolRequestParams::new("igris_session_start").with_arguments(obj(
                serde_json::json!({ "id": "session-a", "project": "igris-memory" }),
            )),
        )
        .await?;
    assert_ne!(response.is_error, Some(true));
    wait_until(&signal, Duration::from_secs(2), || {
        collected.logs.lock().unwrap().len() >= 2
    })
    .await;
    {
        let logs = collected.logs.lock().unwrap();
        assert_eq!(
            logs.len(),
            2,
            "expected start+end log messages, got {logs:?}"
        );
        assert!(logs.iter().all(|m| m.level == LoggingLevel::Info));
        assert_eq!(collected.progress.lock().unwrap().len(), 0);
    }
    collected.logs.lock().unwrap().clear();

    // --- Level filtering: raise the bar to Warning, Info messages vanish. ---
    client
        .set_level(SetLevelRequestParams::new(LoggingLevel::Warning))
        .await?;
    client
        .call_tool(
            CallToolRequestParams::new("igris_session_start").with_arguments(obj(
                serde_json::json!({ "id": "session-b", "project": "igris-memory" }),
            )),
        )
        .await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        collected.logs.lock().unwrap().len(),
        0,
        "Info-level messages should be suppressed at Warning threshold"
    );

    // --- A genuine Warning-level event (session not found) still gets through. ---
    let response = client
        .call_tool(
            CallToolRequestParams::new("igris_session_end")
                .with_arguments(obj(serde_json::json!({ "id": "does-not-exist" }))),
        )
        .await?;
    assert_ne!(response.is_error, Some(true));
    wait_until(&signal, Duration::from_secs(2), || {
        !collected.logs.lock().unwrap().is_empty()
    })
    .await;
    {
        let logs = collected.logs.lock().unwrap();
        assert_eq!(
            logs.len(),
            1,
            "expected exactly the Warning-level error message, got {logs:?}"
        );
        assert_eq!(logs[0].level, LoggingLevel::Warning);
    }

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn get_tool_baseline_logs_no_progress() -> anyhow::Result<()> {
    let db = Database::open_in_memory()?;
    let server = IgrisServer::with_embedder(db, None);
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let running = server.serve(server_transport).await?;
        running.waiting().await?;
        anyhow::Ok(())
    });

    let signal = Arc::new(Notify::new());
    let collected = Collected::default();
    let client = TestClient {
        collected: collected.clone(),
        signal: signal.clone(),
    }
    .serve(client_transport)
    .await?;

    // First, save an observation to get a valid id
    let save_response = client
        .call_tool(CallToolRequestParams::new("igris_save").with_arguments(obj(
            serde_json::json!({
                "title": "test",
                "content": "test content",
            }),
        )))
        .await?;
    assert_ne!(save_response.is_error, Some(true));

    // Clear logs to start fresh for igris_get test
    collected.logs.lock().unwrap().clear();

    // Extract the observation id from the save response
    // The content is a Vec<Annotated<RawContent>> where the text field contains JSON
    let save_json = serde_json::to_value(&save_response.content)?;
    let text_content = save_json
        .as_array()
        .and_then(|arr| arr.get(0))
        .and_then(|obj| obj["text"].as_str())
        .ok_or_else(|| anyhow::anyhow!("no text field in response"))?;
    let parsed_json: serde_json::Value = serde_json::from_str(text_content)?;
    let obs_id = parsed_json["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("no id in parsed response"))?;

    // Now test igris_get with the valid id (success path)
    client
        .call_tool(
            CallToolRequestParams::new("igris_get")
                .with_arguments(obj(serde_json::json!({ "id": obs_id }))),
        )
        .await?;
    wait_until(&signal, Duration::from_secs(2), || {
        collected.logs.lock().unwrap().len() >= 2
    })
    .await;
    let logs = collected.logs.lock().unwrap();
    assert_eq!(
        logs.len(),
        2,
        "expected start+end log messages, got {logs:?}"
    );
    assert!(
        logs.iter().all(|m| m.level == LoggingLevel::Info),
        "all logs should be Info level for success path"
    );
    assert_eq!(collected.progress.lock().unwrap().len(), 0);

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn save_and_search_progress_only_with_embedder() -> anyhow::Result<()> {
    let db = Database::open_in_memory()?;
    let embedder = Arc::new(crate::embed::HashEmbedder::new(32));
    let server = IgrisServer::with_embedder(db, Some(embedder));
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let running = server.serve(server_transport).await?;
        running.waiting().await?;
        anyhow::Ok(())
    });

    let signal = Arc::new(Notify::new());
    let collected = Collected::default();
    let client = TestClient {
        collected: collected.clone(),
        signal: signal.clone(),
    }
    .serve(client_transport)
    .await?;

    client
        .call_tool(CallToolRequestParams::new("igris_save").with_arguments(obj(
            serde_json::json!({
                "title": "test",
                "content": "hello world",
            }),
        )))
        .await?;
    wait_until(&signal, Duration::from_secs(2), || {
        collected.progress.lock().unwrap().len() >= 2
    })
    .await;
    {
        let progress = collected.progress.lock().unwrap();
        assert_eq!(
            progress.len(),
            2,
            "expected a 2-step progress bracket, got {progress:?}"
        );
        assert_eq!(progress[0].total, Some(2.0));
    }
    collected.progress.lock().unwrap().clear();

    client
        .call_tool(
            CallToolRequestParams::new("igris_search")
                .with_arguments(obj(serde_json::json!({ "query": "hello" }))),
        )
        .await?;
    wait_until(&signal, Duration::from_secs(2), || {
        collected.progress.lock().unwrap().len() >= 2
    })
    .await;
    assert_eq!(collected.progress.lock().unwrap().len(), 2);

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn search_no_progress_without_embedder() -> anyhow::Result<()> {
    let db = Database::open_in_memory()?;
    let server = IgrisServer::with_embedder(db, None);
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let running = server.serve(server_transport).await?;
        running.waiting().await?;
        anyhow::Ok(())
    });

    let signal = Arc::new(Notify::new());
    let collected = Collected::default();
    let client = TestClient {
        collected: collected.clone(),
        signal: signal.clone(),
    }
    .serve(client_transport)
    .await?;

    client
        .call_tool(
            CallToolRequestParams::new("igris_search")
                .with_arguments(obj(serde_json::json!({ "query": "hello" }))),
        )
        .await?;
    wait_until(&signal, Duration::from_secs(2), || {
        collected.logs.lock().unwrap().len() >= 2
    })
    .await;
    assert_eq!(collected.progress.lock().unwrap().len(), 0);

    client.cancel().await?;
    Ok(())
}
