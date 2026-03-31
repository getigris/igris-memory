use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use std::fmt;

/// Error codes returned in structured JSON error responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    ValidationError,
    NotFound,
    DatabaseError,
    LockError,
}

/// Structured error for Igris tool responses.
#[derive(Debug, Clone, Serialize)]
pub struct IgrisError {
    pub error: String,
    pub code: ErrorCode,
}

impl IgrisError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self { error: msg.into(), code: ErrorCode::ValidationError }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self { error: msg.into(), code: ErrorCode::NotFound }
    }

    pub fn database(msg: impl Into<String>) -> Self {
        Self { error: msg.into(), code: ErrorCode::DatabaseError }
    }

    pub fn lock(msg: impl Into<String>) -> Self {
        Self { error: msg.into(), code: ErrorCode::LockError }
    }

    /// Serialize this error to a JSON string for MCP tool responses.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| format!("{{\"error\":\"{}\"}}", self.error))
    }
}

impl fmt::Display for IgrisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl std::error::Error for IgrisError {}

impl From<rusqlite::Error> for IgrisError {
    fn from(e: rusqlite::Error) -> Self {
        Self::database(e.to_string())
    }
}

impl From<String> for IgrisError {
    fn from(msg: String) -> Self {
        Self::validation(msg)
    }
}

impl IntoResponse for IgrisError {
    fn into_response(self) -> Response {
        let status = match self.code {
            ErrorCode::ValidationError => StatusCode::BAD_REQUEST,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::DatabaseError => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::LockError => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, axum::Json(self)).into_response()
    }
}

#[cfg(test)]
#[path = "tests/errors_test.rs"]
mod tests;
