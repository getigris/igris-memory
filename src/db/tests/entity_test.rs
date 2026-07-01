use crate::db::Database;

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
