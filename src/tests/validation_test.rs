use super::*;

// ─── require_non_empty ──────────────────────────────────────────

#[test]
fn valid_title_ok() {
    assert!(require_non_empty("Auth setup", "title").is_ok());
}

#[test]
fn empty_title_err() {
    let err = require_non_empty("", "title").unwrap_err();
    assert!(err.contains("title"), "error should mention the field name");
}

#[test]
fn whitespace_title_err() {
    assert!(require_non_empty("   ", "title").is_err());
}

// ─── validate_observation_type ──────────────────────────────────

#[test]
fn valid_type_ok() {
    for t in VALID_TYPES {
        assert!(validate_observation_type(t).is_ok(), "type '{t}' should be valid");
    }
}

#[test]
fn invalid_type_err() {
    let err = validate_observation_type("invalid_type").unwrap_err();
    assert!(err.contains("invalid_type"));
}

// ─── validate_scope ─────────────────────────────────────────────

#[test]
fn valid_scope_ok() {
    assert!(validate_scope("project").is_ok());
    assert!(validate_scope("personal").is_ok());
}

#[test]
fn invalid_scope_err() {
    assert!(validate_scope("global").is_err());
}

// ─── validate_search_query ──────────────────────────────────────

#[test]
fn valid_query_ok() {
    assert!(validate_search_query("JWT auth").is_ok());
}

#[test]
fn empty_query_err() {
    assert!(validate_search_query("").is_err());
    assert!(validate_search_query("   ").is_err());
}

// ─── validate_limit ─────────────────────────────────────────────

#[test]
fn valid_limit_ok() {
    assert!(validate_limit(Some(10)).is_ok());
    assert!(validate_limit(None).is_ok());
}

#[test]
fn zero_limit_err() {
    assert!(validate_limit(Some(0)).is_err());
}

#[test]
fn negative_limit_err() {
    assert!(validate_limit(Some(-5)).is_err());
}

// ─── validate_update_has_fields ─────────────────────────────────

#[test]
fn update_with_title_ok() {
    assert!(validate_update_has_fields(Some("new"), None, None, None, None).is_ok());
}

#[test]
fn empty_update_err() {
    assert!(validate_update_has_fields(None, None, None, None, None).is_err());
}

// ─── validate_save (composite) ──────────────────────────────────

#[test]
fn valid_save_ok() {
    assert!(validate_save("Title", "Content", "decision", "project").is_ok());
}

#[test]
fn save_empty_title_err() {
    assert!(validate_save("", "Content", "decision", "project").is_err());
}

#[test]
fn save_invalid_type_err() {
    assert!(validate_save("Title", "Content", "nope", "project").is_err());
}

#[test]
fn save_invalid_scope_err() {
    assert!(validate_save("Title", "Content", "decision", "global").is_err());
}

// ─── validate_session ───────────────────────────────────────────

#[test]
fn valid_session_ok() {
    assert!(validate_session("sess-1", "myproj").is_ok());
}

#[test]
fn empty_session_id_err() {
    assert!(validate_session("", "myproj").is_err());
}

#[test]
fn empty_session_project_err() {
    assert!(validate_session("sess-1", "").is_err());
}
