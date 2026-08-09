use rmcp::model::{
    CallToolRequestParams, LoggingLevel, LoggingMessageNotificationParam,
    ProgressNotificationParam, SetLevelRequestParams,
};
use rmcp::service::NotificationContext;
use rmcp::{ClientHandler, Peer, RoleClient, ServerHandler, ServiceExt};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

use crate::db::Database;
use crate::server::IgrisServer;
use crate::store::BrainStore;

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

/// Notifications are delivered by the server's single drain task (best-effort,
/// never awaited by the tool that enqueued them), so poll against `signal` up
/// to `timeout` instead of racing the tool's return value.
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
async fn entity_merge_progress_bracket() -> anyhow::Result<()> {
    let db = Database::open_in_memory()?;
    db.upsert_entity("person", "Source Person", &[], None, "project")?;
    db.upsert_entity("person", "Target Person", &[], None, "project")?;
    let source = db.get_entity_by_slug("source-person", None, "project")?;
    let target = db.get_entity_by_slug("target-person", None, "project")?;
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

    let response = client
        .call_tool(
            CallToolRequestParams::new("igris_entity_merge").with_arguments(obj(
                serde_json::json!({
                    "source_id": source.id,
                    "target_id": target.id,
                }),
            )),
        )
        .await?;
    assert_ne!(response.is_error, Some(true));
    wait_until(&signal, Duration::from_secs(2), || {
        collected.progress.lock().unwrap().len() >= 2
    })
    .await;
    let progress = collected.progress.lock().unwrap();
    assert_eq!(
        progress.len(),
        2,
        "expected a 2-step progress bracket, got {progress:?}"
    );
    assert_eq!(progress[0].progress, 1.0);
    assert_eq!(progress[0].total, Some(2.0));
    assert_eq!(progress[1].progress, 2.0);
    assert_eq!(progress[1].total, Some(2.0));

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

    // Wait for igris_save's own start/end notifications to land before
    // clearing — otherwise a slow delivery can leak into the igris_get
    // assertions below and make this test racy.
    wait_until(&signal, Duration::from_secs(2), || {
        collected.logs.lock().unwrap().len() >= 2
    })
    .await;

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
async fn code_search_returns_indexed_symbol() -> anyhow::Result<()> {
    let db = Database::open_in_memory()?;

    // Index a fixture file synchronously and directly through the indexer,
    // bypassing the nondeterministic background-spawn path wired up by
    // igris_session_start (Task 9) — this test only needs to prove the tool's
    // query/response shape against known data, not the background timing.
    let dir = tempfile::tempdir()?;
    std::fs::write(
        dir.path().join("fixture.rs"),
        "pub fn distinctive_fixture_symbol() {}\n",
    )?;
    let summary = crate::codegraph::indexer::index_project(&db, "code-search-test", dir.path());
    assert_eq!(summary.files_indexed, 1, "fixture file should be indexed");

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

    let response = client
        .call_tool(
            CallToolRequestParams::new("igris_code_search").with_arguments(obj(
                serde_json::json!({
                    "query": "distinctive_fixture_symbol",
                    "project": "code-search-test",
                }),
            )),
        )
        .await?;
    assert_ne!(response.is_error, Some(true));

    let response_json = serde_json::to_value(&response.content)?;
    let text_content = response_json
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|o| o["text"].as_str())
        .ok_or_else(|| anyhow::anyhow!("no text field in response"))?;
    assert!(
        text_content.contains("\"name\": \"distinctive_fixture_symbol\""),
        "expected fixture symbol name in response, got {text_content}"
    );
    assert!(
        text_content.contains("\"kind\": \"function\""),
        "expected symbol kind in response, got {text_content}"
    );
    // A search hit is only actionable if it says which file to open.
    assert!(
        text_content.contains("\"relative_path\": \"fixture.rs\""),
        "expected the owning file's path in response, got {text_content}"
    );
    assert!(
        text_content.contains("\"language\": \"rust\""),
        "expected the owning file's language in response, got {text_content}"
    );

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

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn code_neighbors_returns_connected_symbol_and_edge() -> anyhow::Result<()> {
    use crate::codegraph::{ExtractedEdge, ExtractedSymbol};

    let db = Database::open_in_memory()?;

    // Seed via the DB layer directly (same fixture shape as Task 7's
    // `replace_symbols_and_edges_resolves_call_within_same_project`), rather
    // than through the real tree-sitter indexer: the extractor never sets
    // `src_qualified_name`, so indexer-produced `calls` edges always source
    // from the file node, not a symbol — this test needs a genuine
    // symbol-to-symbol edge to exercise `code_neighbors` meaningfully.
    let file = db.upsert_code_file("code-neighbors-test", "/repo", "src/lib.rs", "rust", "h1")?;
    let symbols = vec![
        ExtractedSymbol {
            kind: "function".into(),
            name: "helper".into(),
            qualified_name: "helper".into(),
            start_line: 1,
            end_line: 1,
        },
        ExtractedSymbol {
            kind: "function".into(),
            name: "main".into(),
            qualified_name: "main".into(),
            start_line: 3,
            end_line: 5,
        },
    ];
    let edges = vec![ExtractedEdge {
        relation: "calls".into(),
        src_qualified_name: Some("main".into()),
        dst_name: "helper".into(),
        resolution: "heuristic".into(),
        external_boundary: false,
    }];
    db.replace_symbols_and_edges_for_file(file.id, &symbols, &edges)?;

    let nodes = db.search_code_nodes("helper", None, None, Some("code-neighbors-test"), 10)?;
    assert_eq!(nodes.len(), 1, "expected exactly one `helper` symbol");
    let crate::models::CodeNode::Symbol(helper_symbol) = &nodes[0] else {
        anyhow::bail!("expected a symbol node");
    };
    let helper_id = helper_symbol.id;

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

    let response = client
        .call_tool(
            CallToolRequestParams::new("igris_code_neighbors").with_arguments(obj(
                serde_json::json!({
                    "node_id": helper_id,
                    "node_type": "symbol",
                    "direction": "in",
                }),
            )),
        )
        .await?;
    assert_ne!(response.is_error, Some(true));

    let response_json = serde_json::to_value(&response.content)?;
    let text_content = response_json
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|o| o["text"].as_str())
        .ok_or_else(|| anyhow::anyhow!("no text field in response"))?;
    assert!(
        text_content.contains("\"name\": \"main\""),
        "expected the calling `main` symbol in response, got {text_content}"
    );
    assert!(
        text_content.contains("\"relation\": \"calls\""),
        "expected the `calls` relation in response, got {text_content}"
    );
    assert!(
        text_content.contains("\"resolution\": \"heuristic\""),
        "expected the edge's resolution field in response, got {text_content}"
    );

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

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn code_path_finds_two_hop_chain_and_reports_not_found_when_disconnected()
-> anyhow::Result<()> {
    use crate::codegraph::{ExtractedEdge, ExtractedSymbol};

    let db = Database::open_in_memory()?;

    // Same rationale as `code_neighbors_returns_connected_symbol_and_edge`:
    // the real extractor never sets `src_qualified_name`, so this test seeds
    // genuine symbol-to-symbol edges directly via `replace_symbols_and_edges_for_file`
    // to build a real multi-hop chain: chain_a -> chain_b -> chain_c, spanning
    // two files. `chain_b`/`chain_c` are inserted first so that `chain_a`'s
    // edge (added in the second call) resolves against an already-committed
    // `chain_b` symbol.
    let file_bc = db.upsert_code_file("code-path-test", "/repo", "src/bc.rs", "rust", "h-bc")?;
    let bc_symbols = vec![
        ExtractedSymbol {
            kind: "function".into(),
            name: "chain_b".into(),
            qualified_name: "chain_b".into(),
            start_line: 1,
            end_line: 1,
        },
        ExtractedSymbol {
            kind: "function".into(),
            name: "chain_c".into(),
            qualified_name: "chain_c".into(),
            start_line: 3,
            end_line: 3,
        },
    ];
    let bc_edges = vec![ExtractedEdge {
        relation: "calls".into(),
        src_qualified_name: Some("chain_b".into()),
        dst_name: "chain_c".into(),
        resolution: "heuristic".into(),
        external_boundary: false,
    }];
    db.replace_symbols_and_edges_for_file(file_bc.id, &bc_symbols, &bc_edges)?;

    let file_a = db.upsert_code_file("code-path-test", "/repo", "src/a.rs", "rust", "h-a")?;
    let a_symbols = vec![ExtractedSymbol {
        kind: "function".into(),
        name: "chain_a".into(),
        qualified_name: "chain_a".into(),
        start_line: 1,
        end_line: 1,
    }];
    let a_edges = vec![ExtractedEdge {
        relation: "calls".into(),
        src_qualified_name: Some("chain_a".into()),
        dst_name: "chain_b".into(),
        resolution: "heuristic".into(),
        external_boundary: false,
    }];
    db.replace_symbols_and_edges_for_file(file_a.id, &a_symbols, &a_edges)?;

    // An unrelated, disconnected symbol to exercise the "no path found" case.
    let file_iso = db.upsert_code_file("code-path-test", "/repo", "src/iso.rs", "rust", "h-iso")?;
    let iso_symbols = vec![ExtractedSymbol {
        kind: "function".into(),
        name: "isolated_node".into(),
        qualified_name: "isolated_node".into(),
        start_line: 1,
        end_line: 1,
    }];
    db.replace_symbols_and_edges_for_file(file_iso.id, &iso_symbols, &[])?;

    let find_id = |name: &str| -> anyhow::Result<i64> {
        let nodes = db.search_code_nodes(name, None, None, Some("code-path-test"), 10)?;
        anyhow::ensure!(nodes.len() == 1, "expected exactly one `{name}` symbol");
        let crate::models::CodeNode::Symbol(sym) = &nodes[0] else {
            anyhow::bail!("expected a symbol node for `{name}`");
        };
        Ok(sym.id)
    };
    let chain_a_id = find_id("chain_a")?;
    let chain_c_id = find_id("chain_c")?;
    let isolated_id = find_id("isolated_node")?;

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

    // chain_a -> chain_c should resolve as a real 2-hop path through chain_b.
    let response = client
        .call_tool(
            CallToolRequestParams::new("igris_code_path").with_arguments(obj(serde_json::json!({
                "from_id": chain_a_id,
                "from_type": "symbol",
                "to_id": chain_c_id,
                "to_type": "symbol",
            }))),
        )
        .await?;
    assert_ne!(response.is_error, Some(true));

    let response_json = serde_json::to_value(&response.content)?;
    let text_content = response_json
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|o| o["text"].as_str())
        .ok_or_else(|| anyhow::anyhow!("no text field in response"))?;
    let path: Vec<serde_json::Value> = serde_json::from_str(text_content)?;
    assert_eq!(
        path.len(),
        2,
        "expected a 2-edge path (chain_a->chain_b->chain_c), got {text_content}"
    );
    assert!(path.iter().all(|hop| hop["edge"]["relation"] == "calls"));
    assert_eq!(path[0]["node"]["name"], "chain_b");
    assert_eq!(path[1]["node"]["name"], "chain_c");
    assert_eq!(path[1]["node"]["node_type"], "symbol");

    // chain_a -> isolated_node has no connecting edges at all: this must
    // come back as a clean "not found" result (JSON null), not an error.
    let response = client
        .call_tool(
            CallToolRequestParams::new("igris_code_path").with_arguments(obj(serde_json::json!({
                "from_id": chain_a_id,
                "from_type": "symbol",
                "to_id": isolated_id,
                "to_type": "symbol",
            }))),
        )
        .await?;
    assert_ne!(response.is_error, Some(true));

    let response_json = serde_json::to_value(&response.content)?;
    let text_content = response_json
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|o| o["text"].as_str())
        .ok_or_else(|| anyhow::anyhow!("no text field in response"))?;
    assert_eq!(
        text_content.trim(),
        "null",
        "expected a clean JSON null for the disconnected case, got {text_content}"
    );

    wait_until(&signal, Duration::from_secs(2), || {
        collected.logs.lock().unwrap().len() >= 4
    })
    .await;
    {
        let logs = collected.logs.lock().unwrap();
        assert_eq!(
            logs.len(),
            4,
            "expected two start+end pairs (one per call), got {logs:?}"
        );
        assert!(
            logs.iter().all(|m| m.level == LoggingLevel::Info),
            "no error-level logs expected for the not-found case: {logs:?}"
        );
        assert_eq!(collected.progress.lock().unwrap().len(), 0);
    }

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn code_map_returns_symbols_and_connections_and_reports_not_found_for_unindexed_path()
-> anyhow::Result<()> {
    use crate::codegraph::{ExtractedEdge, ExtractedSymbol};

    let db = Database::open_in_memory()?;

    // Seed via the DB layer directly, same rationale as the code_neighbors/
    // code_path tests: the real extractor never sets `src_qualified_name`,
    // so a *file-attributed* edge (src_qualified_name: None) is exactly what
    // `index_project` would also produce — this is the shape `code_map`'s
    // `top_connections` actually surfaces (edges rooted at the file node),
    // so it's seeded directly here for a deterministic, reviewable fixture.
    let file = db.upsert_code_file("code-map-test", "/repo", "src/lib.rs", "rust", "h1")?;
    let symbols = vec![
        ExtractedSymbol {
            kind: "function".into(),
            name: "helper".into(),
            qualified_name: "helper".into(),
            start_line: 1,
            end_line: 1,
        },
        ExtractedSymbol {
            kind: "function".into(),
            name: "main".into(),
            qualified_name: "main".into(),
            start_line: 3,
            end_line: 5,
        },
    ];
    let edges = vec![ExtractedEdge {
        relation: "calls".into(),
        src_qualified_name: None,
        dst_name: "helper".into(),
        resolution: "heuristic".into(),
        external_boundary: false,
    }];
    db.replace_symbols_and_edges_for_file(file.id, &symbols, &edges)?;

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

    let response = client
        .call_tool(
            CallToolRequestParams::new("igris_code_map").with_arguments(obj(serde_json::json!({
                "project": "code-map-test",
                "path": "src/lib.rs",
            }))),
        )
        .await?;
    assert_ne!(response.is_error, Some(true));

    let response_json = serde_json::to_value(&response.content)?;
    let text_content = response_json
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|o| o["text"].as_str())
        .ok_or_else(|| anyhow::anyhow!("no text field in response"))?;
    assert!(
        text_content.contains("\"name\": \"helper\""),
        "expected `helper` symbol in response, got {text_content}"
    );
    assert!(
        text_content.contains("\"name\": \"main\""),
        "expected `main` symbol in response, got {text_content}"
    );
    assert!(
        text_content.contains("\"relation\": \"calls\""),
        "expected the file's `calls` connection in response, got {text_content}"
    );

    // A path that was never indexed must come back as a clean not-found
    // error, not a panic.
    let response = client
        .call_tool(
            CallToolRequestParams::new("igris_code_map").with_arguments(obj(serde_json::json!({
                "project": "code-map-test",
                "path": "src/does_not_exist.rs",
            }))),
        )
        .await?;

    let response_json = serde_json::to_value(&response.content)?;
    let text_content = response_json
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|o| o["text"].as_str())
        .ok_or_else(|| anyhow::anyhow!("no text field in response"))?;
    assert!(
        text_content.contains("\"code\":\"NOT_FOUND\""),
        "expected a structured not-found error, got {text_content}"
    );

    // First call logs start+end (2); the not-found call additionally logs a
    // warning-level error entry (3) — see `igris_code_map`'s error branch.
    wait_until(&signal, Duration::from_secs(2), || {
        collected.logs.lock().unwrap().len() >= 5
    })
    .await;
    {
        let logs = collected.logs.lock().unwrap();
        assert_eq!(
            logs.len(),
            5,
            "expected start+end (success) and start+error+end (not-found), got {logs:?}"
        );
        assert_eq!(
            logs.iter()
                .filter(|m| m.level == LoggingLevel::Warning)
                .count(),
            1,
            "expected exactly one warning-level log for the not-found error, got {logs:?}"
        );
        assert_eq!(collected.progress.lock().unwrap().len(), 0);
    }

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

    // `HashEmbedder::embed` runs inside the spawned server task on save/search.
    // If it panics (e.g. a mutation-testing mutant makes it index out of
    // bounds), the server task dies mid-request and this `.await` would
    // otherwise hang forever instead of failing — bound it with a timeout so
    // a broken embedder fails the test instead of hanging the whole suite.
    tokio::time::timeout(
        Duration::from_secs(5),
        client.call_tool(CallToolRequestParams::new("igris_save").with_arguments(obj(
            serde_json::json!({
                "title": "test",
                "content": "hello world",
            }),
        ))),
    )
    .await
    .expect("igris_save timed out — server task likely panicked (e.g. in HashEmbedder::embed)")?;
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

    tokio::time::timeout(
        Duration::from_secs(5),
        client.call_tool(
            CallToolRequestParams::new("igris_search")
                .with_arguments(obj(serde_json::json!({ "query": "hello" }))),
        ),
    )
    .await
    .expect("igris_search timed out — server task likely panicked (e.g. in HashEmbedder::embed)")?;
    wait_until(&signal, Duration::from_secs(2), || {
        collected.progress.lock().unwrap().len() >= 2
    })
    .await;
    assert_eq!(collected.progress.lock().unwrap().len(), 2);

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn purge_reports_two_phase_progress() -> anyhow::Result<()> {
    let db = Database::open_in_memory()?;
    let obs = db.save_observation("t", "c", "manual", None, "project", None, None, None)?;
    db.delete_observation(obs.id)?;
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

    let response = client
        .call_tool(
            CallToolRequestParams::new("igris_purge")
                .with_arguments(obj(serde_json::json!({ "older_than_days": 0 }))),
        )
        .await?;
    assert_ne!(response.is_error, Some(true));
    wait_until(&signal, Duration::from_secs(2), || {
        collected.progress.lock().unwrap().len() >= 2
    })
    .await;
    let progress = collected.progress.lock().unwrap();
    assert_eq!(
        progress.len(),
        2,
        "expected hard_delete + vacuum progress, got {progress:?}"
    );
    assert_eq!(progress[0].progress, 1.0);
    assert_eq!(progress[0].total, Some(2.0));
    assert_eq!(progress[1].progress, 2.0);
    assert_eq!(progress[1].total, Some(2.0));

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn export_reports_six_section_progress() -> anyhow::Result<()> {
    let db = Database::open_in_memory()?;
    db.save_observation("t", "c", "manual", None, "project", None, None, None)?;
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

    let response = client
        .call_tool(CallToolRequestParams::new("igris_export"))
        .await?;
    assert_ne!(response.is_error, Some(true));
    wait_until(&signal, Duration::from_secs(2), || {
        collected.progress.lock().unwrap().len() >= 6
    })
    .await;
    {
        let progress = collected.progress.lock().unwrap();
        assert!(
            progress.len() >= 6,
            "expected at least 6 progress notifications (one per section), got {progress:?}"
        );
        let last = progress.last().expect("at least one progress notification");
        assert_eq!(last.progress, 6.0);
        assert_eq!(last.total, Some(6.0));
    }

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

#[tokio::test]
async fn import_reports_six_section_progress() -> anyhow::Result<()> {
    let src_db = Database::open_in_memory()?;
    src_db.save_observation("t", "c", "manual", None, "project", None, None, None)?;
    let export_data = src_db.export_all()?;
    let payload = serde_json::to_string(&export_data)?;

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

    let response = client
        .call_tool(
            CallToolRequestParams::new("igris_import")
                .with_arguments(obj(serde_json::json!({ "data": payload }))),
        )
        .await?;
    assert_ne!(response.is_error, Some(true));
    wait_until(&signal, Duration::from_secs(2), || {
        collected.progress.lock().unwrap().len() >= 6
    })
    .await;
    {
        let progress = collected.progress.lock().unwrap();
        assert!(
            progress.len() >= 6,
            "expected at least 6 progress notifications (one per section), got {progress:?}"
        );
        let last = progress.last().expect("at least one progress notification");
        assert_eq!(last.progress, 6.0);
        assert_eq!(last.total, Some(6.0));
    }

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn import_invalid_json_closes_notification_bracket() -> anyhow::Result<()> {
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

    let response = client
        .call_tool(
            CallToolRequestParams::new("igris_import")
                .with_arguments(obj(serde_json::json!({ "data": "not valid json" }))),
        )
        .await?;
    assert_ne!(response.is_error, Some(true));
    wait_until(&signal, Duration::from_secs(2), || {
        collected.logs.lock().unwrap().len() >= 3
    })
    .await;
    {
        let logs = collected.logs.lock().unwrap();
        assert_eq!(
            logs.len(),
            3,
            "expected start + Warning error + end (no dangling start), got {logs:?}"
        );
        assert_eq!(logs[0].level, LoggingLevel::Info);
        assert_eq!(logs[1].level, LoggingLevel::Warning);
        assert_eq!(logs[2].level, LoggingLevel::Info);
        assert!(
            logs.iter()
                .all(|m| m.logger.as_deref() == Some("igris_import")),
            "logger field mismatch: {logs:?}"
        );
    }

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn entity_merge_stops_progress_on_failure() -> anyhow::Result<()> {
    let db = Database::open_in_memory()?;
    let entity = db.upsert_entity("person", "Solo Entity", &[], None, "project")?;
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

    // source_id == target_id is rejected by merge_entities before any progress
    // past the initial "merging..." step — the failed merge must not report
    // the terminal 2/2 "done" progress.
    let response = client
        .call_tool(
            CallToolRequestParams::new("igris_entity_merge").with_arguments(obj(
                serde_json::json!({ "source_id": entity.id, "target_id": entity.id }),
            )),
        )
        .await?;
    assert_ne!(response.is_error, Some(true));
    wait_until(&signal, Duration::from_secs(2), || {
        collected.logs.lock().unwrap().len() >= 3
    })
    .await;
    {
        let logs = collected.logs.lock().unwrap();
        assert_eq!(
            logs.len(),
            3,
            "expected start + Warning error + end, got {logs:?}"
        );
        assert_eq!(logs[1].level, LoggingLevel::Warning);
    }
    {
        let progress = collected.progress.lock().unwrap();
        assert_eq!(
            progress.len(),
            1,
            "a failed merge must not emit the terminal 2/2 progress step, got {progress:?}"
        );
        assert_eq!(progress[0].progress, 1.0);
        assert_eq!(progress[0].total, Some(2.0));
    }

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn notification_payloads_never_leak_full_content() -> anyhow::Result<()> {
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

    let marker_title = "MARKER_TITLE_9f3a7c2e";
    let marker_content =
        "MARKER_CONTENT_this_is_a_long_secret_body_that_must_never_appear_in_a_notification";

    client
        .call_tool(CallToolRequestParams::new("igris_save").with_arguments(obj(
            serde_json::json!({ "title": marker_title, "content": marker_content }),
        )))
        .await?;
    wait_until(&signal, Duration::from_secs(2), || {
        collected.logs.lock().unwrap().len() >= 2
    })
    .await;

    let logs = collected.logs.lock().unwrap();
    assert!(!logs.is_empty(), "expected at least one log message");
    for msg in logs.iter() {
        let serialized = serde_json::to_string(&msg.data)?;
        assert!(
            !serialized.contains(marker_title),
            "notification leaked the raw title: {serialized}"
        );
        assert!(
            !serialized.contains(marker_content),
            "notification leaked the raw content: {serialized}"
        );
    }

    client.cancel().await?;
    Ok(())
}

/// Calls `tool` with `args`, waits for exactly `expected_count` log messages to
/// arrive, and asserts every one of them is Info-level and tagged with `tool`'s
/// own name (no cross-tool contamination) before clearing the collected logs
/// for the next call in a sequential coverage test.
async fn call_and_assert_uniform(
    peer: &Peer<RoleClient>,
    signal: &Notify,
    collected: &Collected,
    tool: &str,
    args: serde_json::Value,
    expected_count: usize,
) -> anyhow::Result<()> {
    let response = peer
        .call_tool(CallToolRequestParams::new(tool.to_string()).with_arguments(obj(args)))
        .await?;
    assert_ne!(
        response.is_error,
        Some(true),
        "{tool} returned a protocol-level error"
    );
    wait_until(signal, Duration::from_secs(2), || {
        collected.logs.lock().unwrap().len() >= expected_count
    })
    .await;
    {
        let logs = collected.logs.lock().unwrap();
        assert_eq!(
            logs.len(),
            expected_count,
            "{tool}: expected {expected_count} log messages, got {logs:?}"
        );
        assert!(
            logs.iter().all(|m| m.logger.as_deref() == Some(tool)),
            "{tool}: logger field mismatch in {logs:?}"
        );
        assert!(
            logs.iter().all(|m| m.level == LoggingLevel::Info),
            "{tool}: expected only Info-level messages on the success path, got {logs:?}"
        );
    }
    // None of these 18 tools emit progress notifications — catches a future
    // regression where one starts leaking progress (or another tool's token)
    // without a dedicated test noticing. Safe to check without an extra wait:
    // the drain task delivers strictly FIFO, and every tool enqueues any
    // progress calls before its final "end" log, so observing `expected_count`
    // logs above already guarantees any earlier-enqueued progress notification
    // has been delivered too.
    assert!(
        collected.progress.lock().unwrap().is_empty(),
        "{tool}: unexpected progress notification for a tool that shouldn't emit one"
    );
    collected.logs.lock().unwrap().clear();
    Ok(())
}

/// The other protocol tests each exercise one tool's distinguishing behavior
/// (progress brackets, level filtering, error paths). This test closes the
/// remaining coverage gap: every one of the 18 tools not otherwise covered by
/// name in this file gets at least one real end-to-end call, proving its
/// `ctx`/`notify_log` wiring actually fires at runtime (not just verified by
/// code review) and that no tool's notifications bleed into another's.
#[tokio::test]
async fn remaining_tools_emit_consistent_notifications() -> anyhow::Result<()> {
    let db = Database::open_in_memory()?;
    let entity_a = db.upsert_entity("person", "Alice Seed", &[], None, "project")?;
    let entity_b = db.upsert_entity("person", "Bob Seed", &[], None, "project")?;
    let entity_c = db.upsert_entity("person", "Dave ToDelete", &[], None, "project")?;
    db.upsert_edge(entity_a.id, entity_b.id, "seed_link")?;
    let obs1 = db.save_observation(
        "Obs One",
        "content one",
        "manual",
        None,
        "project",
        None,
        None,
        None,
    )?;
    let obs2 = db.save_observation(
        "Obs Two",
        "content two",
        "manual",
        None,
        "project",
        None,
        None,
        None,
    )?;

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
    let peer = client.peer();

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_session_summary",
        serde_json::json!({ "content": "summary text", "project": "project" }),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_update",
        serde_json::json!({ "id": obs1.id, "title": "Updated title" }),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_context",
        serde_json::json!({}),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_stats",
        serde_json::json!({}),
        1,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_timeline",
        serde_json::json!({ "observation_id": obs1.id }),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_suggest_topic_key",
        serde_json::json!({ "type": "decision", "title": "t", "content": "c" }),
        1,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_entity_upsert",
        serde_json::json!({ "kind": "person", "name": "Carol New" }),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_entity_get",
        serde_json::json!({ "id": entity_a.id }),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_entity_update",
        serde_json::json!({ "id": entity_a.id, "tier": 2 }),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_entity_link",
        serde_json::json!({ "src_id": entity_a.id, "dst_id": entity_b.id, "relation": "collaborates" }),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_entity_neighbors",
        serde_json::json!({ "entity_id": entity_a.id }),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_entity_timeline",
        serde_json::json!({ "entity_id": entity_a.id }),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_brief",
        serde_json::json!({ "id": entity_a.id }),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_entity_search",
        serde_json::json!({ "query": "Alice" }),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_entity_list",
        serde_json::json!({}),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_entity_unlink",
        serde_json::json!({ "src_id": entity_a.id, "dst_id": entity_b.id, "relation": "collaborates" }),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_entity_delete",
        serde_json::json!({ "id": entity_c.id }),
        2,
    )
    .await?;

    call_and_assert_uniform(
        peer,
        &signal,
        &collected,
        "igris_delete",
        serde_json::json!({ "id": obs2.id }),
        2,
    )
    .await?;

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn debug_fmt_includes_struct_name_and_fields() -> anyhow::Result<()> {
    let db = Database::open_in_memory()?;
    let server = IgrisServer::with_embedder(db, None);

    let debug_str = format!("{server:?}");
    // `Ok(Default::default())` writes nothing at all, so this alone already
    // kills the mutant — the extra assertions pin down real content too.
    assert!(
        !debug_str.is_empty(),
        "Debug::fmt must actually write something"
    );
    assert!(
        debug_str.contains("IgrisServer"),
        "expected the struct name in Debug output: {debug_str}"
    );
    assert!(
        debug_str.contains("db"),
        "expected the `db` field in Debug output: {debug_str}"
    );

    Ok(())
}

#[tokio::test]
async fn igris_save_wires_mentions_when_present() -> anyhow::Result<()> {
    let db = Database::open_in_memory()?;
    let server = IgrisServer::with_embedder(db, None);
    // Keep a handle to the server's underlying db so we can inspect the
    // `mentions`/`entities` tables directly after the round-trip below.
    let db_handle = server.db.clone();
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

    // `record_mentions` runs inside the spawned server task on save. If it
    // panics (e.g. a mutation-testing mutant makes it index out of bounds),
    // the server task dies mid-request and this `.await` would otherwise
    // hang forever instead of failing — bound it with a timeout so a broken
    // mentions pipeline fails the test instead of hanging the whole suite.
    let save_response = tokio::time::timeout(
        Duration::from_secs(5),
        client.call_tool(CallToolRequestParams::new("igris_save").with_arguments(obj(
            serde_json::json!({
                "title": "mentions test",
                "content": "content about someone",
                "mentions": ["Mentioned Person"],
            }),
        ))),
    )
    .await
    .expect("igris_save timed out — server task likely panicked (e.g. in record_mentions)")?;
    assert_ne!(
        save_response.is_error,
        Some(true),
        "save failed: {:?}",
        save_response.content
    );

    let save_json = serde_json::to_value(&save_response.content)?;
    let text_content = save_json
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|o| o["text"].as_str())
        .ok_or_else(|| anyhow::anyhow!("no text field in response"))?;
    let parsed: serde_json::Value = serde_json::from_str(text_content)?;
    let obs_id = parsed["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("no id in save response"))?;

    // A non-empty `mentions` list must have driven `record_mentions`, which
    // auto-creates a stub entity for the unknown name and links it to the
    // saved observation via the `mentions` table. Deleting the `!` in
    // `!mentions.is_empty()` would skip this wiring for a non-empty list
    // (it would only run when the list is empty), leaving no such entity
    // or link — this test would then fail both assertions below.
    let inner_db = db_handle.lock().unwrap();
    let entity = inner_db.get_entity_by_slug("mentioned-person", None, "project")?;
    assert_eq!(entity.canonical_name, "Mentioned Person");
    let mention_count: i64 = inner_db.conn.query_row(
        "SELECT COUNT(*) FROM mentions WHERE observation_id = ?1 AND entity_id = ?2",
        rusqlite::params![obs_id, entity.id],
        |r| r.get(0),
    )?;
    assert_eq!(
        mention_count, 1,
        "expected a mentions row linking the observation to the entity"
    );
    drop(inner_db);

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn get_info_reports_real_instructions_and_capabilities() -> anyhow::Result<()> {
    let db = Database::open_in_memory()?;
    let server = IgrisServer::with_embedder(db, None);

    let info = server.get_info();
    // `Default::default()` would produce `instructions: None` and an empty
    // `capabilities`, so either assertion alone kills the mutant.
    let instructions = info
        .instructions
        .expect("get_info must report Some(instructions)");
    assert!(
        instructions.contains("Session Lifecycle"),
        "expected the real instructions text, got: {instructions}"
    );
    assert!(
        info.capabilities.tools.is_some(),
        "expected tools capability to be enabled"
    );

    Ok(())
}

#[tokio::test]
async fn igris_backfill_candidates_lists_unmentioned_observations() -> anyhow::Result<()> {
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

    tokio::time::timeout(
        Duration::from_secs(5),
        client.call_tool(CallToolRequestParams::new("igris_save").with_arguments(obj(
            serde_json::json!({
                "title": "no mentions yet",
                "content": "plain observation, nothing linked",
            }),
        ))),
    )
    .await
    .expect("igris_save timed out")?;

    let response = tokio::time::timeout(
        Duration::from_secs(5),
        client.call_tool(
            CallToolRequestParams::new("igris_backfill_candidates")
                .with_arguments(obj(serde_json::json!({ "limit": 20 }))),
        ),
    )
    .await
    .expect("igris_backfill_candidates timed out")?;

    assert_ne!(
        response.is_error,
        Some(true),
        "call failed: {:?}",
        response.content
    );
    let json = serde_json::to_value(&response.content)?;
    let text = json
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|o| o["text"].as_str())
        .ok_or_else(|| anyhow::anyhow!("no text field in response"))?;
    let parsed: serde_json::Value = serde_json::from_str(text)?;
    let titles: Vec<&str> = parsed
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"no mentions yet"));

    client.cancel().await?;
    Ok(())
}
