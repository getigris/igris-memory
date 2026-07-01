use crate::db::Database;
use crate::errors::ErrorCode;
use crate::store::BrainStore;

#[test]
fn database_implements_brainstore() {
    fn assert_impl<T: BrainStore>() {}
    assert_impl::<Database>();
}

#[test]
fn upsert_creates_then_returns_entity() {
    let db = Database::open_in_memory().unwrap();
    let e = db
        .upsert_entity("person", "Jane Doe", &[], Some("igris-memory"), "project")
        .unwrap();
    assert_eq!(e.kind, "person");
    assert_eq!(e.canonical_name, "Jane Doe");
    assert_eq!(e.slug, "jane-doe");
    assert_eq!(e.tier, 3);

    let fetched = db.get_entity(e.id).unwrap();
    assert_eq!(fetched.id, e.id);
    assert_eq!(fetched.slug, "jane-doe");
}

#[test]
fn upsert_same_slug_updates_in_place() {
    let db = Database::open_in_memory().unwrap();
    let a = db
        .upsert_entity("person", "Jane Doe", &[], None, "project")
        .unwrap();
    let b = db
        .upsert_entity("person", "Jane Doe", &["JD".to_string()], None, "project")
        .unwrap();
    assert_eq!(a.id, b.id, "same slug must update in place, not duplicate");

    let count: i64 = db
        .conn
        .query_row("SELECT count(*) FROM entities", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);

    // canonical_name + provided alias are both registered
    let alias_count: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM entity_aliases WHERE entity_id = ?1",
            [a.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(alias_count, 2, "expected 'jane doe' + 'jd' aliases");
}

#[test]
fn get_entity_by_slug_scopes_by_project() {
    let db = Database::open_in_memory().unwrap();
    db.upsert_entity("company", "Acme", &[], Some("proj-a"), "project")
        .unwrap();
    let got = db
        .get_entity_by_slug("acme", Some("proj-a"), "project")
        .unwrap();
    assert_eq!(got.canonical_name, "Acme");
    assert!(
        db.get_entity_by_slug("acme", Some("proj-b"), "project")
            .is_err(),
        "slug lookup must be scoped to project"
    );
}

#[test]
fn get_entity_returns_not_found_for_missing_id() {
    let db = Database::open_in_memory().unwrap();
    let err = db.get_entity(9999).unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn get_entity_by_slug_returns_not_found_for_missing_slug() {
    let db = Database::open_in_memory().unwrap();
    let err = db
        .get_entity_by_slug("does-not-exist", None, "project")
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn upsert_rejects_invalid_kind() {
    let db = Database::open_in_memory().unwrap();
    assert!(
        db.upsert_entity("alien", "X", &[], None, "project")
            .is_err()
    );
}

#[test]
fn schema_v2_creates_entity_tables() {
    let db = Database::open_in_memory().unwrap();
    for table in ["entities", "entity_aliases", "edges", "mentions"] {
        let count: i64 = db
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "table {table} should exist");
    }
    let version: u32 = db
        .conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2);
}

#[test]
fn schema_v2_preserves_observations_table() {
    let db = Database::open_in_memory().unwrap();
    let count: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='observations'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 1,
        "existing observations table must survive migration"
    );
}

#[test]
fn entity_model_serializes_kind_field() {
    use crate::models::Entity;
    let e = Entity {
        id: 1,
        kind: "person".to_string(),
        canonical_name: "Jane Doe".to_string(),
        slug: "jane-doe".to_string(),
        tier: 3,
        salience: 0.0,
        compiled_truth: None,
        compiled_at: None,
        project: Some("igris-memory".to_string()),
        scope: "project".to_string(),
        created_at: "2026-06-30T00:00:00Z".to_string(),
        updated_at: "2026-06-30T00:00:00Z".to_string(),
        deleted_at: None,
    };
    let json = serde_json::to_string(&e).unwrap();
    assert!(json.contains("\"kind\":\"person\""));
    assert!(json.contains("\"canonical_name\":\"Jane Doe\""));
}

#[test]
fn normalize_alias_lowercases_and_collapses_whitespace() {
    use crate::utils::normalize_alias;
    assert_eq!(normalize_alias("  Jane   DOE "), "jane doe");
}

#[test]
fn entity_slug_produces_kebab_case() {
    use crate::utils::entity_slug;
    assert_eq!(entity_slug("Acme Corp, Inc."), "acme-corp-inc");
    assert_eq!(entity_slug("  Hello!!  World  "), "hello-world");
}

#[test]
fn validate_entity_rejects_unknown_kind() {
    use crate::validation::validate_entity;
    assert!(validate_entity("alien", "X", "project").is_err());
    assert!(validate_entity("person", "Jane", "project").is_ok());
}

#[test]
fn validate_entity_rejects_empty_name() {
    use crate::validation::validate_entity;
    assert!(validate_entity("person", "   ", "project").is_err());
}

#[test]
fn entity_upsert_args_default_scope_is_project() {
    // Deserializing without a scope must default to "project",
    // matching the tool contract.
    let json = r#"{"kind":"person","name":"Jane Doe"}"#;
    let args: crate::server::args::EntityUpsertArgs = serde_json::from_str(json).unwrap();
    assert_eq!(args.scope, "project");
    assert_eq!(args.name, "Jane Doe");
    assert!(args.aliases.is_none());
}
