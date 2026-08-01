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
    assert_eq!(version, 5);
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
fn save_args_accepts_optional_mentions() {
    let json = r#"{"title":"t","content":"c","mentions":["Acme","Jane"]}"#;
    let args: crate::server::args::SaveArgs = serde_json::from_str(json).unwrap();
    assert_eq!(
        args.mentions.as_deref(),
        Some(&["Acme".to_string(), "Jane".to_string()][..])
    );

    // Absent mentions deserialize to None (backward compatible)
    let json2 = r#"{"title":"t","content":"c"}"#;
    let args2: crate::server::args::SaveArgs = serde_json::from_str(json2).unwrap();
    assert!(args2.mentions.is_none());
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

#[test]
fn entity_link_and_neighbors_args_defaults() {
    let link: crate::server::args::EntityLinkArgs =
        serde_json::from_str(r#"{"src_id":1,"dst_id":2,"relation":"works_at"}"#).unwrap();
    assert_eq!(link.src_id, 1);
    assert_eq!(link.dst_id, 2);
    assert_eq!(link.relation, "works_at");

    let n: crate::server::args::EntityNeighborsArgs =
        serde_json::from_str(r#"{"entity_id":5}"#).unwrap();
    assert_eq!(n.entity_id, 5);
    assert!(n.limit.is_none());
}

#[test]
fn export_data_new_fields_default_when_absent() {
    use crate::models::ExportData;
    // Old Fase-0a export JSON has no entity fields — must still deserialize.
    let json = r#"{"version":2,"exported_at":"t","observations":[],"sessions":[]}"#;
    let data: ExportData = serde_json::from_str(json).unwrap();
    assert!(data.entities.is_empty());
    assert!(data.entity_aliases.is_empty());
    assert!(data.edges.is_empty());
    assert!(data.mentions.is_empty());
}

#[test]
fn export_all_includes_entity_graph() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let o = db
        .save_observation("t", "c", "manual", None, "project", None, None, None)
        .unwrap();
    db.record_mentions(
        o.id,
        &["Acme".to_string(), "Bob".to_string()],
        None,
        "project",
    )
    .unwrap();

    let data = db.export_all().unwrap();
    assert_eq!(data.entities.len(), 2);
    assert!(data.entity_aliases.len() >= 2); // canonical alias per stub
    assert_eq!(data.edges.len(), 1); // co_mentioned Acme<->Bob
    assert_eq!(data.mentions.len(), 2);
}

#[test]
fn entity_alias_and_mention_models_serialize() {
    use crate::models::{EntityAlias, Mention};
    let a = EntityAlias {
        entity_id: 1,
        alias_normalized: "acme".to_string(),
        source: Some("provided".to_string()),
    };
    let m = Mention {
        observation_id: 5,
        entity_id: 1,
    };
    assert!(
        serde_json::to_string(&a)
            .unwrap()
            .contains("\"alias_normalized\":\"acme\"")
    );
    assert!(
        serde_json::to_string(&m)
            .unwrap()
            .contains("\"observation_id\":5")
    );
}

#[test]
fn import_roundtrips_entity_graph_with_remapped_ids() {
    use crate::store::BrainStore;
    // Source DB with an observation + two co-mentioned entities.
    let src = Database::open_in_memory().unwrap();
    let o = src
        .save_observation(
            "meeting", "notes", "manual", None, "project", None, None, None,
        )
        .unwrap();
    src.record_mentions(
        o.id,
        &["Acme".to_string(), "Bob".to_string()],
        None,
        "project",
    )
    .unwrap();
    let data = src.export_all().unwrap();

    // Destination DB already has ONE unrelated entity, so autoincrement ids differ.
    let dst = Database::open_in_memory().unwrap();
    dst.upsert_entity("person", "Zara", &[], None, "project")
        .unwrap();

    let result = dst.import_data(&data).unwrap();
    assert_eq!(result.entities_imported, 2);
    assert_eq!(result.mentions_imported, 2);
    assert_eq!(result.edges_imported, 1);

    // The imported Acme entity resolves and has Bob as a neighbor — proving
    // edge/mention ids were remapped to the destination's entity ids.
    let acme = dst.get_entity_by_slug("acme", None, "project").unwrap();
    let neighbors = dst.entity_neighbors(acme.id, 10).unwrap();
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].entity.canonical_name, "Bob");

    // Importing the same data again is idempotent (dedup): no duplicates.
    let again = dst.import_data(&data).unwrap();
    assert_eq!(again.entities_imported, 0);
    assert_eq!(again.edges_imported, 0);
    assert_eq!(again.mentions_imported, 0);
}

#[test]
fn import_old_export_without_entities_still_works() {
    let db = Database::open_in_memory().unwrap();
    let data: crate::models::ExportData =
        serde_json::from_str(r#"{"version":2,"exported_at":"t","observations":[],"sessions":[]}"#)
            .unwrap();
    let r = db.import_data(&data).unwrap();
    assert_eq!(r.entities_imported, 0);
}

#[test]
fn sync_roundtrip_preserves_entity_graph() {
    use crate::store::BrainStore;
    let tmp = std::env::temp_dir().join("igmem_sync_entity_test_0bport");
    let _ = std::fs::remove_dir_all(&tmp);

    let src = Database::open_in_memory().unwrap();
    let o = src
        .save_observation("t", "c", "manual", None, "project", None, None, None)
        .unwrap();
    src.record_mentions(
        o.id,
        &["Acme".to_string(), "Bob".to_string()],
        None,
        "project",
    )
    .unwrap();
    crate::sync::export_to_dir(&src, &tmp).unwrap();

    let dst = Database::open_in_memory().unwrap();
    crate::sync::import_from_dir(&dst, &tmp).unwrap();

    let acme = dst.get_entity_by_slug("acme", None, "project").unwrap();
    let neighbors = dst.entity_neighbors(acme.id, 10).unwrap();
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].entity.canonical_name, "Bob");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn entity_brief_model_serializes() {
    use crate::models::{Entity, EntityBrief};
    let entity = Entity {
        id: 1,
        kind: "company".to_string(),
        canonical_name: "Acme".to_string(),
        slug: "acme".to_string(),
        tier: 3,
        salience: 0.0,
        compiled_truth: Some("# Acme".to_string()),
        compiled_at: Some("2026-07-01T00:00:00Z".to_string()),
        project: None,
        scope: "project".to_string(),
        created_at: "2026-07-01T00:00:00Z".to_string(),
        updated_at: "2026-07-01T00:00:00Z".to_string(),
        deleted_at: None,
    };
    let brief = EntityBrief {
        entity,
        neighbors: vec![],
        recent: vec![],
    };
    let json = serde_json::to_string(&brief).unwrap();
    assert!(json.contains("\"entity\""));
    assert!(json.contains("\"neighbors\""));
    assert!(json.contains("\"recent\""));
}

#[test]
fn v1_database_upgrades_to_v2_preserving_data() {
    use crate::schema::SCHEMA_V1;
    use rusqlite::Connection;

    let path = std::env::temp_dir().join("igmem_v1_upgrade_test_0bport.db");
    let _ = std::fs::remove_file(&path);

    // Simulate a Fase-0a (v1) database: apply ONLY SCHEMA_V1, set user_version = 1,
    // and insert one observation.
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.execute_batch("PRAGMA user_version = 1;").unwrap();
        conn.execute(
            "INSERT INTO observations (type, title, content, scope) VALUES ('manual','t','c','project')",
            [],
        )
        .unwrap();
    }

    // Open through Database → migration must add v2 tables and bump user_version.
    {
        let db = crate::db::Database::open(&path, None).unwrap();
        let version: u32 = db
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 5);
        for table in ["entities", "entity_aliases", "edges", "mentions"] {
            let n: i64 = db
                .conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "v2 table {table} should exist after upgrade");
        }
        // Pre-existing v1 data survived.
        let obs_count: i64 = db
            .conn
            .query_row("SELECT count(*) FROM observations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(obs_count, 1);
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn entity_timeline_returns_mentioning_observations_recent_first() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let o1 = db
        .save_observation("first", "c1", "manual", None, "project", None, None, None)
        .unwrap();
    db.record_mentions(o1.id, &["Acme".to_string()], None, "project")
        .unwrap();
    let o2 = db
        .save_observation("second", "c2", "manual", None, "project", None, None, None)
        .unwrap();
    db.record_mentions(o2.id, &["Acme".to_string()], None, "project")
        .unwrap();

    let acme = db.get_entity_by_slug("acme", None, "project").unwrap();
    let tl = db.entity_timeline(acme.id, 10).unwrap();
    assert_eq!(tl.len(), 2);
    // most recent first
    assert_eq!(tl[0].id, o2.id);
    assert_eq!(tl[1].id, o1.id);
}

#[test]
fn compile_entity_truth_is_deterministic_and_persists() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let o = db
        .save_observation("kickoff", "c", "manual", None, "project", None, None, None)
        .unwrap();
    db.record_mentions(
        o.id,
        &["Acme".to_string(), "Bob".to_string()],
        None,
        "project",
    )
    .unwrap();
    let acme = db.get_entity_by_slug("acme", None, "project").unwrap();

    let truth1 = db.compile_entity_truth(acme.id).unwrap();
    assert!(truth1.contains("# Acme"));
    assert!(truth1.contains("Bob")); // co-mentioned neighbor listed
    assert!(truth1.contains("kickoff")); // recent mention title listed

    // Persisted into the row.
    let reloaded = db.get_entity(acme.id).unwrap();
    assert_eq!(reloaded.compiled_truth.as_deref(), Some(truth1.as_str()));
    assert!(reloaded.compiled_at.is_some());

    // Deterministic: same inputs → same output.
    let truth2 = db.compile_entity_truth(acme.id).unwrap();
    assert_eq!(truth1, truth2);
}

#[test]
fn entity_brief_bundles_truth_neighbors_and_recent() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let o = db
        .save_observation("meeting", "c", "manual", None, "project", None, None, None)
        .unwrap();
    db.record_mentions(
        o.id,
        &["Acme".to_string(), "Bob".to_string()],
        None,
        "project",
    )
    .unwrap();
    let acme = db.get_entity_by_slug("acme", None, "project").unwrap();

    let brief = db.entity_brief(acme.id).unwrap();
    assert_eq!(brief.entity.id, acme.id);
    assert!(brief.entity.compiled_truth.is_some()); // freshly compiled
    assert_eq!(brief.neighbors.len(), 1);
    assert_eq!(brief.neighbors[0].entity.canonical_name, "Bob");
    assert_eq!(brief.recent.len(), 1);
    assert_eq!(brief.recent[0].id, o.id);
}

#[test]
fn purge_succeeds_when_deleted_observation_has_mentions() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let o = db
        .save_observation("t", "c", "manual", None, "project", None, None, None)
        .unwrap();
    db.record_mentions(o.id, &["Acme".to_string()], None, "project")
        .unwrap();

    let mention_count_before: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM mentions WHERE observation_id = ?1",
            [o.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(mention_count_before, 1);

    assert!(db.delete_observation(o.id).unwrap());

    let result = db.purge(0);
    assert!(
        result.is_ok(),
        "purge must succeed even when a purged observation has mentions rows: {result:?}"
    );
    assert_eq!(result.unwrap().observations_purged, 1);

    let obs_count: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM observations WHERE id = ?1",
            [o.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(obs_count, 0, "observation should be hard-deleted");

    let mention_count_after: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM mentions WHERE observation_id = ?1",
            [o.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        mention_count_after, 0,
        "mentions rows should cascade-delete with the observation"
    );
}

#[test]
fn timeline_and_brief_args_defaults() {
    let tl: crate::server::args::EntityTimelineArgs =
        serde_json::from_str(r#"{"entity_id":7}"#).unwrap();
    assert_eq!(tl.entity_id, 7);
    assert!(tl.limit.is_none());

    let b: crate::server::args::BriefArgs = serde_json::from_str(r#"{"slug":"acme"}"#).unwrap();
    assert_eq!(b.slug.as_deref(), Some("acme"));
    assert!(b.id.is_none());
    assert_eq!(b.scope, "project");
}

#[test]
fn search_entities_matches_by_alias_and_filters_kind() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    db.upsert_entity(
        "company",
        "Acme Corp",
        &["ACME".to_string()],
        None,
        "project",
    )
    .unwrap();
    db.upsert_entity("person", "Jane Acme", &[], None, "project")
        .unwrap();

    // "acme" matches both (Acme Corp via alias, Jane Acme via canonical alias)
    let all = db.search_entities("acme", None, None, None, 20).unwrap();
    assert_eq!(all.len(), 2);
    // kind filter narrows to the company
    let companies = db
        .search_entities("acme", Some("company"), None, None, 20)
        .unwrap();
    assert_eq!(companies.len(), 1);
    assert_eq!(companies[0].canonical_name, "Acme Corp");
    // no match
    assert!(
        db.search_entities("zzz", None, None, None, 20)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn list_entities_returns_recent_and_filters_kind() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    db.upsert_entity("company", "Acme", &[], None, "project")
        .unwrap();
    db.upsert_entity("person", "Bob", &[], None, "project")
        .unwrap();
    db.upsert_entity("person", "Carol", &[], None, "project")
        .unwrap();

    let all = db.list_entities(None, None, None, 20).unwrap();
    assert_eq!(all.len(), 3);
    let people = db.list_entities(Some("person"), None, None, 20).unwrap();
    assert_eq!(people.len(), 2);
    assert!(people.iter().all(|e| e.kind == "person"));
}

#[test]
fn entity_search_and_list_args_defaults() {
    let s: crate::server::args::EntitySearchArgs =
        serde_json::from_str(r#"{"query":"acme"}"#).unwrap();
    assert_eq!(s.query, "acme");
    assert!(s.limit.is_none());
    let l: crate::server::args::EntityListArgs = serde_json::from_str(r#"{}"#).unwrap();
    assert!(l.kind.is_none());
    assert!(l.limit.is_none());
}

#[test]
fn delete_entity_soft_deletes_and_hides_it() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let e = db
        .upsert_entity("person", "Zoe", &[], None, "project")
        .unwrap();
    assert!(db.delete_entity(e.id).unwrap());
    // hidden from get and list
    assert!(db.get_entity(e.id).is_err());
    assert!(db.list_entities(None, None, None, 20).unwrap().is_empty());
    // deleting again returns false
    assert!(!db.delete_entity(e.id).unwrap());
}

#[test]
fn unlink_entities_removes_edge_either_direction() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let o = db
        .save_observation("t", "c", "manual", None, "project", None, None, None)
        .unwrap();
    let resolved = db
        .record_mentions(
            o.id,
            &["Acme".to_string(), "Bob".to_string()],
            None,
            "project",
        )
        .unwrap();
    let acme = resolved
        .iter()
        .find(|e| e.canonical_name == "Acme")
        .unwrap();
    let bob = resolved.iter().find(|e| e.canonical_name == "Bob").unwrap();
    assert_eq!(db.entity_neighbors(acme.id, 10).unwrap().len(), 1);

    // unlink using the reversed direction still matches the stored min<max edge
    let removed = db.unlink_entities(bob.id, acme.id, "co_mentioned").unwrap();
    assert_eq!(removed, 1);
    assert_eq!(db.entity_neighbors(acme.id, 10).unwrap().len(), 0);
}

#[test]
fn update_entity_sets_kind_tier_salience_and_adds_alias() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let e = db
        .upsert_entity("other", "Acme", &[], None, "project")
        .unwrap();
    let original_slug = e.slug.clone();

    let updated = db
        .update_entity(
            e.id,
            Some("company"),
            Some(1),
            Some(0.75),
            &["Acme Corp".to_string()],
        )
        .unwrap();

    assert_eq!(updated.id, e.id);
    assert_eq!(updated.kind, "company");
    assert_eq!(updated.tier, 1);
    assert_eq!(updated.salience, 0.75);
    assert_eq!(updated.slug, original_slug, "slug must not change");

    // Reload independently to make sure the changes were persisted, not just returned.
    let reloaded = db.get_entity(e.id).unwrap();
    assert_eq!(reloaded.kind, "company");
    assert_eq!(reloaded.tier, 1);
    assert_eq!(reloaded.salience, 0.75);

    // The added alias resolves back to the same entity.
    let resolved = db
        .resolve_or_stub_entity("Acme Corp", None, "project")
        .unwrap();
    assert_eq!(resolved.id, e.id);
}

#[test]
fn update_entity_requires_at_least_one_field() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let e = db
        .upsert_entity("other", "Zed", &[], None, "project")
        .unwrap();
    let err = db.update_entity(e.id, None, None, None, &[]).unwrap_err();
    assert_eq!(err.code, ErrorCode::ValidationError);
}

#[test]
fn update_entity_returns_not_found_for_missing_id() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let err = db
        .update_entity(9999, Some("company"), None, None, &[])
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn stats_counts_entities_and_edges() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let o = db
        .save_observation("t", "c", "manual", None, "project", None, None, None)
        .unwrap();
    db.record_mentions(
        o.id,
        &["Acme".to_string(), "Bob".to_string()],
        None,
        "project",
    )
    .unwrap();
    let s = db.stats().unwrap();
    assert_eq!(s.total_entities, 2);
    assert_eq!(s.total_edges, 1);
}

#[test]
fn merge_entities_moves_aliases_mentions_and_edges_then_hides_source() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let target = db
        .upsert_entity("company", "Acme Corp", &[], None, "project")
        .unwrap();
    let source = db
        .upsert_entity("company", "Acme Inc", &[], None, "project")
        .unwrap();
    let carol = db
        .upsert_entity("person", "Carol", &[], None, "project")
        .unwrap();

    db.update_entity(source.id, None, None, None, &["Acme Legacy".to_string()])
        .unwrap();

    let o = db
        .save_observation(
            "kickoff", "notes", "manual", None, "project", None, None, None,
        )
        .unwrap();
    db.add_mention(o.id, source.id).unwrap();
    db.upsert_edge(source.id, carol.id, "co_mentioned").unwrap();

    let merged = db.merge_entities(source.id, target.id).unwrap();
    assert_eq!(merged.id, target.id);
    assert_eq!(merged.canonical_name, "Acme Corp");

    // Source's own name and its alias now resolve to target.
    let by_source_name = db
        .resolve_or_stub_entity("Acme Inc", None, "project")
        .unwrap();
    assert_eq!(by_source_name.id, target.id);
    let by_alias = db
        .resolve_or_stub_entity("Acme Legacy", None, "project")
        .unwrap();
    assert_eq!(by_alias.id, target.id);

    // Target's timeline now includes source's observation.
    let timeline = db.entity_timeline(target.id, 10).unwrap();
    assert!(timeline.iter().any(|obs| obs.id == o.id));

    // Target now has Carol as a neighbor (edge moved, not dropped).
    let neighbors = db.entity_neighbors(target.id, 10).unwrap();
    assert!(neighbors.iter().any(|n| n.entity.id == carol.id));

    // Source is hidden everywhere.
    assert!(db.get_entity(source.id).is_err());
    let listed = db.list_entities(None, None, None, 100).unwrap();
    assert!(!listed.iter().any(|e| e.id == source.id));
}

#[test]
fn merge_entities_dedupes_mention_shared_with_target() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let target = db
        .upsert_entity("company", "Acme Corp", &[], None, "project")
        .unwrap();
    let source = db
        .upsert_entity("company", "Acme Inc", &[], None, "project")
        .unwrap();
    let o = db
        .save_observation("t", "c", "manual", None, "project", None, None, None)
        .unwrap();
    // Same observation mentions BOTH source and target already.
    db.add_mention(o.id, source.id).unwrap();
    db.add_mention(o.id, target.id).unwrap();

    // Must not panic/error with a UNIQUE constraint violation.
    db.merge_entities(source.id, target.id).unwrap();

    let count: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM mentions WHERE observation_id = ?1 AND entity_id = ?2",
            rusqlite::params![o.id, target.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 1,
        "exactly one mention row for (observation, target)"
    );
}

#[test]
fn merge_entities_drops_would_be_self_loop_edge() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let target = db
        .upsert_entity("company", "Acme Corp", &[], None, "project")
        .unwrap();
    let source = db
        .upsert_entity("company", "Acme Inc", &[], None, "project")
        .unwrap();
    // Source and target are directly connected before the merge.
    db.upsert_edge(source.id, target.id, "co_mentioned")
        .unwrap();

    db.merge_entities(source.id, target.id).unwrap();

    let self_loops: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM edges
             WHERE src_entity_id = ?1 AND dst_entity_id = ?1 AND deleted_at IS NULL",
            rusqlite::params![target.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(self_loops, 0, "must not create a target-target self-loop");

    let neighbors = db.entity_neighbors(target.id, 10).unwrap();
    assert!(
        !neighbors.iter().any(|n| n.entity.id == target.id),
        "target must never appear as its own neighbor"
    );
}

#[test]
fn merge_entities_preserves_direction_of_directed_edges() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let target = db
        .upsert_entity("company", "Acme Corp", &[], None, "project")
        .unwrap();
    let source = db
        .upsert_entity("company", "Acme Inc", &[], None, "project")
        .unwrap();
    let dana = db
        .upsert_entity("person", "Dana", &[], None, "project")
        .unwrap();

    // source -> dana ("employs"), directed.
    db.upsert_edge(source.id, dana.id, "employs").unwrap();

    db.merge_entities(source.id, target.id).unwrap();

    let forward: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM edges
             WHERE src_entity_id = ?1 AND dst_entity_id = ?2 AND edge_type = 'employs'
               AND deleted_at IS NULL",
            rusqlite::params![target.id, dana.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(forward, 1, "direction target->dana must be preserved");

    let reversed: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM edges
             WHERE src_entity_id = ?1 AND dst_entity_id = ?2 AND edge_type = 'employs'
               AND deleted_at IS NULL",
            rusqlite::params![dana.id, target.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reversed, 0, "must not flip direction");
}

#[test]
fn merge_entities_into_self_is_rejected() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let e = db
        .upsert_entity("company", "Acme Corp", &[], None, "project")
        .unwrap();
    let err = db.merge_entities(e.id, e.id).unwrap_err();
    assert_eq!(err.code, ErrorCode::ValidationError);
}

#[test]
fn merge_entities_with_missing_ids_returns_not_found() {
    use crate::store::BrainStore;
    let db = Database::open_in_memory().unwrap();
    let e = db
        .upsert_entity("company", "Acme Corp", &[], None, "project")
        .unwrap();

    let err_missing_source = db.merge_entities(9999, e.id).unwrap_err();
    assert_eq!(err_missing_source.code, ErrorCode::NotFound);

    let err_missing_target = db.merge_entities(e.id, 9999).unwrap_err();
    assert_eq!(err_missing_target.code, ErrorCode::NotFound);
}
