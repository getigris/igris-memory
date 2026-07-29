use rmcp::RoleServer;
use rmcp::model::{LoggingLevel, LoggingMessageNotificationParam, ProgressNotificationParam};
use rmcp::service::RequestContext;
use serde_json::{Map, Value};

use super::IgrisServer;

/// Total ordering over `LoggingLevel` by severity. The MCP spec's `LoggingLevel`
/// doesn't derive `Ord`, so this ranks it for comparison against the configured
/// minimum level (`logging/setLevel`).
#[allow(dead_code)]
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
#[allow(dead_code)]
pub(crate) fn with_duration(mut data: Value, duration_ms: u64) -> Value {
    if let Value::Object(ref mut map) = data {
        map.insert("duration_ms".to_string(), Value::from(duration_ms));
    }
    data
}

impl IgrisServer {
    /// Best-effort, non-blocking `notifications/message` send. Filtered against
    /// the client-configured minimum level (default `Info`). Never awaited by
    /// the caller — a tool's return latency is unaffected. Delivery failures
    /// (client gone, transport closed) are logged to stderr via
    /// `tracing::debug!` and otherwise ignored.
    #[allow(dead_code)]
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
        let peer = ctx.peer.clone();
        let tool = tool.to_string();
        let params = LoggingMessageNotificationParam {
            level,
            logger: Some(tool.clone()),
            data: Value::Object(payload),
        };
        tokio::spawn(async move {
            if let Err(e) = peer.notify_logging_message(params).await {
                tracing::debug!(tool, error = %e, "log notification not delivered");
            }
        });
    }

    /// Best-effort, non-blocking `notifications/progress` send. No-op if the
    /// incoming request didn't carry a `progressToken` (the client opted out).
    #[allow(dead_code)]
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
        let peer = ctx.peer.clone();
        let params = ProgressNotificationParam {
            progress_token,
            progress,
            total,
            message: Some(message.into()),
        };
        tokio::spawn(async move {
            if let Err(e) = peer.notify_progress(params).await {
                tracing::debug!(error = %e, "progress notification not delivered");
            }
        });
    }
}

#[cfg(test)]
#[path = "tests/notify_test.rs"]
mod tests;
