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

#[test]
fn family_for_plan() {
    assert_eq!(type_to_family("plan"), "plan");
}

#[test]
fn suggest_plan_topic() {
    let key = suggest_topic_key("plan", "Implement HTTP API", "Add axum server...");
    assert_eq!(key, "plan/implement-http-api");
}
