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

#[test]
fn edge_and_neighbor_models_serialize() {
    use crate::models::{Edge, Entity, EntityNeighbor};
    let edge = Edge {
        id: 1,
        src_entity_id: 2,
        dst_entity_id: 3,
        edge_type: "co_mentioned".to_string(),
        evidence_count: 4,
        confidence: 1.0,
        first_seen: "2026-07-01T00:00:00Z".to_string(),
        last_seen: "2026-07-01T00:00:00Z".to_string(),
        deleted_at: None,
    };
    let entity = Entity {
        id: 3,
        kind: "company".to_string(),
        canonical_name: "Acme".to_string(),
        slug: "acme".to_string(),
        tier: 3,
        salience: 0.0,
        compiled_truth: None,
        compiled_at: None,
        project: None,
        scope: "project".to_string(),
        created_at: "2026-07-01T00:00:00Z".to_string(),
        updated_at: "2026-07-01T00:00:00Z".to_string(),
        deleted_at: None,
    };
    let n = EntityNeighbor { edge, entity };
    let json = serde_json::to_string(&n).unwrap();
    assert!(json.contains("\"edge_type\":\"co_mentioned\""));
    assert!(json.contains("\"evidence_count\":4"));
    assert!(json.contains("\"canonical_name\":\"Acme\""));
}

#[test]
fn upsert_entity_strips_private_tags_from_name() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let e = db
        .upsert_entity(
            "person",
            "Jane <private>secret</private>",
            &["<private>also-secret</private> JD".to_string()],
            None,
            "project",
        )
        .unwrap();
    assert!(!e.canonical_name.contains("secret"));
    assert!(e.canonical_name.contains("[REDACTED]"));
    let alias_leak: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM entity_aliases WHERE alias_normalized LIKE '%secret%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        alias_leak, 0,
        "no raw private value should be stored in aliases"
    );
}

#[test]
fn record_mentions_stubs_unknown_and_links() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let obs = db
        .save_observation("t", "c", "manual", None, "project", None, None, None)
        .unwrap();
    let resolved = db
        .record_mentions(
            obs.id,
            &["Acme Corp".to_string(), "Jane Doe".to_string()],
            None,
            "project",
        )
        .unwrap();
    assert_eq!(resolved.len(), 2);
    // both stubbed as kind "other", tier 3
    assert!(resolved.iter().all(|e| e.kind == "other" && e.tier == 3));
    // two mention rows
    let mentions: i64 = db
        .conn
        .query_row("SELECT count(*) FROM mentions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mentions, 2);
    // one co_mentioned edge, stored src<dst
    let (src, dst, etype, ev): (i64, i64, String, i64) = db
        .conn
        .query_row(
            "SELECT src_entity_id, dst_entity_id, edge_type, evidence_count FROM edges",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert!(src < dst);
    assert_eq!(etype, "co_mentioned");
    assert_eq!(ev, 1);
}

#[test]
fn record_mentions_resolves_existing_alias_and_strengthens_edge() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    // Pre-declare Acme with an alias
    let acme = db
        .upsert_entity(
            "company",
            "Acme Corp",
            &["Acme".to_string()],
            None,
            "project",
        )
        .unwrap();
    let o1 = db
        .save_observation("t1", "c1", "manual", None, "project", None, None, None)
        .unwrap();
    let r1 = db
        .record_mentions(
            o1.id,
            &["Acme".to_string(), "Bob".to_string()],
            None,
            "project",
        )
        .unwrap();
    // "Acme" resolves to the existing Acme Corp entity, not a new stub
    assert!(r1.iter().any(|e| e.id == acme.id));
    let entity_count: i64 = db
        .conn
        .query_row("SELECT count(*) FROM entities", [], |r| r.get(0))
        .unwrap();
    assert_eq!(entity_count, 2, "Acme reused + Bob stub");

    // Second observation mentions the same pair → edge evidence_count increments
    let o2 = db
        .save_observation("t2", "c2", "manual", None, "project", None, None, None)
        .unwrap();
    db.record_mentions(
        o2.id,
        &["Acme Corp".to_string(), "Bob".to_string()],
        None,
        "project",
    )
    .unwrap();
    let edge_rows: i64 = db
        .conn
        .query_row("SELECT count(*) FROM edges", [], |r| r.get(0))
        .unwrap();
    assert_eq!(edge_rows, 1, "same pair → single edge, not duplicated");
    let ev: i64 = db
        .conn
        .query_row("SELECT evidence_count FROM edges", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ev, 2);
}

#[test]
fn entity_neighbors_returns_other_end_strongest_first() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let o = db
        .save_observation("t", "c", "manual", None, "project", None, None, None)
        .unwrap();
    let resolved = db
        .record_mentions(
            o.id,
            &["Acme".to_string(), "Bob".to_string(), "Carol".to_string()],
            None,
            "project",
        )
        .unwrap();
    let acme = resolved
        .iter()
        .find(|e| e.canonical_name == "Acme")
        .unwrap();
    let neighbors = db.entity_neighbors(acme.id, 10).unwrap();
    // Acme co-mentioned with Bob and Carol → 2 neighbors, none of them Acme itself
    assert_eq!(neighbors.len(), 2);
    assert!(neighbors.iter().all(|n| n.entity.id != acme.id));
    assert!(neighbors.iter().all(|n| n.edge.edge_type == "co_mentioned"));
}

#[test]
fn add_mention_is_idempotent() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let o = db
        .save_observation("t", "c", "manual", None, "project", None, None, None)
        .unwrap();
    let e = db
        .upsert_entity("other", "X", &[], None, "project")
        .unwrap();
    db.add_mention(o.id, e.id).unwrap();
    db.add_mention(o.id, e.id).unwrap();
    let count: i64 = db
        .conn
        .query_row("SELECT count(*) FROM mentions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
