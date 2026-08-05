use rmcp::RoleServer;
use rmcp::model::{LoggingLevel, LoggingMessageNotificationParam, ProgressNotificationParam};
use rmcp::service::RequestContext;
use serde_json::{Map, Value};

use super::IgrisServer;

/// A queued notification, dispatched in FIFO order by the single drain task
/// owned by an `IgrisServer` instance. Using one channel + one task (instead
/// of an independent `tokio::spawn` per notification) guarantees delivery
/// order matches call order, which the MCP spec requires for `progress`
/// values on a given token to increase monotonically.
pub(crate) enum NotifyJob {
    Log(rmcp::Peer<RoleServer>, LoggingMessageNotificationParam),
    Progress(rmcp::Peer<RoleServer>, ProgressNotificationParam),
}

/// Total ordering over `LoggingLevel` by severity. The MCP spec's `LoggingLevel`
/// doesn't derive `Ord`, so this ranks it for comparison against the configured
/// minimum level (`logging/setLevel`).
fn level_rank(level: LoggingLevel) -> u8 {
    match level {
        LoggingLevel::Debug => 0,
        LoggingLevel::Info => 1,
        LoggingLevel::Notice => 2,
        LoggingLevel::Warning => 3,
        LoggingLevel::Error => 4,
        LoggingLevel::Critical => 5,
        LoggingLevel::Alert => 6,
        LoggingLevel::Emergency => 7,
    }
}

/// Inserts `duration_ms` into an object-shaped notification payload. No-op if
/// `data` isn't an object (shouldn't happen given how callers build it).
pub(crate) fn with_duration(mut data: Value, duration_ms: u64) -> Value {
    if let Value::Object(ref mut map) = data {
        map.insert("duration_ms".to_string(), Value::from(duration_ms));
    }
    data
}

impl IgrisServer {
    /// Best-effort, non-blocking `notifications/message` send. Filtered against
    /// the client-configured minimum level (default `Info`). Never awaited by
    /// the caller — a tool's return latency is unaffected. The job is handed to
    /// the server's single drain task, which delivers it in FIFO order relative
    /// to every other queued notification; delivery failures (client gone,
    /// transport closed) are logged to stderr via `tracing::debug!` and
    /// otherwise ignored.
    pub(crate) fn notify_log(
        &self,
        ctx: &RequestContext<RoleServer>,
        level: LoggingLevel,
        tool: &str,
        phase: &str,
        data: Value,
    ) {
        let min_level = self
            .log_level
            .lock()
            .map(|l| *l)
            .unwrap_or(LoggingLevel::Info);
        if level_rank(level) < level_rank(min_level) {
            return;
        }
        let mut payload = match data {
            Value::Object(map) => map,
            other => {
                let mut m = Map::new();
                m.insert("value".to_string(), other);
                m
            }
        };
        payload.insert("phase".to_string(), Value::String(phase.to_string()));
        let params = LoggingMessageNotificationParam {
            level,
            logger: Some(tool.to_string()),
            data: Value::Object(payload),
        };
        let _ = self
            .notify_tx
            .send(NotifyJob::Log(ctx.peer.clone(), params));
    }

    /// Best-effort, non-blocking `notifications/progress` send. No-op if the
    /// incoming request didn't carry a `progressToken` (the client opted out).
    /// Like `notify_log`, this hands the job to the server's single drain task
    /// for FIFO delivery.
    pub(crate) fn notify_progress(
        &self,
        ctx: &RequestContext<RoleServer>,
        progress: f64,
        total: Option<f64>,
        message: impl Into<String>,
    ) {
        let Some(progress_token) = ctx.meta.get_progress_token() else {
            return;
        };
        let params = ProgressNotificationParam {
            progress_token,
            progress,
            total,
            message: Some(message.into()),
        };
        let _ = self
            .notify_tx
            .send(NotifyJob::Progress(ctx.peer.clone(), params));
    }
}

#[cfg(test)]
#[path = "tests/notify_test.rs"]
mod tests;
