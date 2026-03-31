/// Suggest a stable topic_key based on observation type, title, and content.
/// Topic keys follow the pattern `{family}/{slug}` where family is derived
/// from the observation type and slug from the title.
pub fn suggest_topic_key(obs_type: &str, title: &str, _content: &str) -> String {
    let family = type_to_family(obs_type);
    let slug = slugify(title);
    format!("{family}/{slug}")
}

/// Map observation type to topic family prefix.
fn type_to_family(obs_type: &str) -> &str {
    match obs_type {
        "decision" => "decision",
        "architecture" => "arch",
        "bugfix" => "bug",
        "pattern" => "pattern",
        "config" => "config",
        "discovery" => "discovery",
        "learning" => "learning",
        "plan" => "plan",
        "manual" => "note",
        _ => "other",
    }
}

const MAX_SLUG_LEN: usize = 60;

/// Convert a title to a URL-safe slug.
fn slugify(title: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    // Collapse multiple dashes and trim
    let mut result = String::with_capacity(slug.len());
    let mut prev_dash = true; // starts true to trim leading dashes
    for c in slug.chars() {
        if c == '-' {
            if !prev_dash {
                result.push('-');
            }
            prev_dash = true;
        } else {
            result.push(c);
            prev_dash = false;
        }
    }
    // Trim trailing dash
    let trimmed = result.trim_end_matches('-');
    // Truncate to max length at a dash boundary
    if trimmed.len() <= MAX_SLUG_LEN {
        trimmed.to_string()
    } else {
        let truncated = &trimmed[..MAX_SLUG_LEN];
        match truncated.rfind('-') {
            Some(pos) => truncated[..pos].to_string(),
            None => truncated.to_string(),
        }
    }
}

#[cfg(test)]
#[path = "tests/topic_test.rs"]
mod tests;
