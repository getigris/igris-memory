use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

static PRIVATE_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<private>.*?</private>").unwrap());

/// Strips `<private>...</private>` tags from content, replacing with `[REDACTED]`.
/// This prevents sensitive data (API keys, passwords, etc.) from being persisted.
pub fn strip_private_tags(input: &str) -> String {
    PRIVATE_TAG_RE.replace_all(input, "[REDACTED]").to_string()
}

/// Produces a SHA-256 hex digest of the content after normalizing whitespace.
/// Used for deduplication — two observations with the same hash within
/// a time window are considered duplicates.
pub fn hash_content(content: &str) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Returns the current UTC timestamp in ISO 8601 format.
pub fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
#[path = "tests/utils_test.rs"]
mod tests;
