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

/// Normalize an alias for deterministic matching: trim, lowercase,
/// and collapse internal whitespace to single spaces.
#[allow(dead_code)] // TODO(fase-0a): remove once used in Task 5
pub fn normalize_alias(input: &str) -> String {
    input
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Produce a stable kebab-case slug from an entity name.
/// Keeps ASCII alphanumerics, turns runs of other chars into single dashes,
/// trims leading/trailing dashes, and caps length at 60 chars.
#[allow(dead_code)] // TODO(fase-0a): remove once used in Task 5
pub fn entity_slug(input: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in input.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !slug.is_empty() && !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug.chars().take(60).collect()
}

#[cfg(test)]
#[path = "tests/utils_test.rs"]
mod tests;
