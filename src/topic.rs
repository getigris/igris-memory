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
mod tests {
    use super::*;

    // ─── type_to_family ─────────────────────────────────────────────

    #[test]
    fn family_for_decision() {
        assert_eq!(type_to_family("decision"), "decision");
    }

    #[test]
    fn family_for_architecture() {
        assert_eq!(type_to_family("architecture"), "arch");
    }

    #[test]
    fn family_for_bugfix() {
        assert_eq!(type_to_family("bugfix"), "bug");
    }

    #[test]
    fn family_for_pattern() {
        assert_eq!(type_to_family("pattern"), "pattern");
    }

    #[test]
    fn family_for_config() {
        assert_eq!(type_to_family("config"), "config");
    }

    #[test]
    fn family_for_discovery() {
        assert_eq!(type_to_family("discovery"), "discovery");
    }

    #[test]
    fn family_for_learning() {
        assert_eq!(type_to_family("learning"), "learning");
    }

    #[test]
    fn family_for_manual() {
        assert_eq!(type_to_family("manual"), "note");
    }

    #[test]
    fn family_for_unknown_type() {
        assert_eq!(type_to_family("something_else"), "other");
    }

    // ─── slugify ────────────────────────────────────────────────────

    #[test]
    fn slugify_simple_title() {
        assert_eq!(slugify("Auth middleware"), "auth-middleware");
    }

    #[test]
    fn slugify_strips_special_chars() {
        assert_eq!(slugify("JWT (v2) — tokens!"), "jwt-v2-tokens");
    }

    #[test]
    fn slugify_collapses_dashes() {
        assert_eq!(slugify("hello---world"), "hello-world");
    }

    #[test]
    fn slugify_trims_dashes() {
        assert_eq!(slugify("  Hello World  "), "hello-world");
    }

    #[test]
    fn slugify_unicode_accents() {
        // Basic approach: drop non-ascii or keep as-is
        let s = slugify("Configuración DB");
        assert!(!s.is_empty());
        assert!(s.contains("db"));
    }

    #[test]
    fn slugify_long_title_truncates() {
        let long_title = "a]".repeat(100);
        let slug = slugify(&long_title);
        assert!(slug.len() <= 60, "slug should be truncated to max 60 chars");
    }

    // ─── suggest_topic_key (integration) ────────────────────────────

    #[test]
    fn suggest_decision_topic() {
        let key = suggest_topic_key("decision", "Use PostgreSQL over MySQL", "We compared...");
        assert_eq!(key, "decision/use-postgresql-over-mysql");
    }

    #[test]
    fn suggest_architecture_topic() {
        let key = suggest_topic_key("architecture", "Auth middleware design", "JWT based...");
        assert_eq!(key, "arch/auth-middleware-design");
    }

    #[test]
    fn suggest_bugfix_topic() {
        let key = suggest_topic_key("bugfix", "Fix null pointer in login", "The crash...");
        assert_eq!(key, "bug/fix-null-pointer-in-login");
    }

    #[test]
    fn suggest_manual_topic() {
        let key = suggest_topic_key("manual", "Quick note about deploy", "...");
        assert_eq!(key, "note/quick-note-about-deploy");
    }
}
