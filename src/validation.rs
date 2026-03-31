/// Valid observation types accepted by Igris.
pub const VALID_TYPES: &[&str] = &[
    "decision",
    "architecture",
    "bugfix",
    "pattern",
    "config",
    "discovery",
    "learning",
    "plan",
    "manual",
];

/// Valid scopes for observations.
pub const VALID_SCOPES: &[&str] = &["project", "personal"];

/// Validate that a string is not empty or whitespace-only.
pub fn require_non_empty(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    Ok(())
}

/// Validate that the observation type is one of the known types.
pub fn validate_observation_type(obs_type: &str) -> Result<(), String> {
    if !VALID_TYPES.contains(&obs_type) {
        return Err(format!(
            "Invalid type '{obs_type}'. Must be one of: {}",
            VALID_TYPES.join(", ")
        ));
    }
    Ok(())
}

/// Validate that the scope is valid.
pub fn validate_scope(scope: &str) -> Result<(), String> {
    if !VALID_SCOPES.contains(&scope) {
        return Err(format!(
            "Invalid scope '{scope}'. Must be one of: {}",
            VALID_SCOPES.join(", ")
        ));
    }
    Ok(())
}

/// Validate a search query.
pub fn validate_search_query(query: &str) -> Result<(), String> {
    require_non_empty(query, "query")
}

/// Validate a limit parameter (must be > 0).
pub fn validate_limit(limit: Option<i64>) -> Result<(), String> {
    if let Some(n) = limit
        && n <= 0
    {
        return Err(format!("limit must be greater than 0, got {n}"));
    }
    Ok(())
}

/// Validate that an update has at least one field to change.
pub fn validate_update_has_fields(
    title: Option<&str>,
    content: Option<&str>,
    obs_type: Option<&str>,
    tags: Option<&[String]>,
    topic_key: Option<&str>,
) -> Result<(), String> {
    if title.is_none() && content.is_none() && obs_type.is_none() && tags.is_none() && topic_key.is_none() {
        return Err("Update requires at least one field to change".to_string());
    }
    Ok(())
}

/// Validate save observation inputs.
pub fn validate_save(title: &str, content: &str, obs_type: &str, scope: &str) -> Result<(), String> {
    require_non_empty(title, "title")?;
    require_non_empty(content, "content")?;
    validate_observation_type(obs_type)?;
    validate_scope(scope)?;
    Ok(())
}

/// Validate session inputs.
pub fn validate_session(id: &str, project: &str) -> Result<(), String> {
    require_non_empty(id, "session id")?;
    require_non_empty(project, "project")?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/validation_test.rs"]
mod tests;
