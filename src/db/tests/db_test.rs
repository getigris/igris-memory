use super::*;
use rusqlite::params;

fn test_db() -> Database {
    Database::open_in_memory().expect("failed to create in-memory db")
}

// ─── Observations ───────────────────────────────────────────────

#[test]
fn save_and_get() {
    let db = test_db();
    let obs = db
        .save_observation(
            "Test title",
            "Test content",
            "decision",
            Some("myproj"),
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(obs.title, "Test title");
    assert_eq!(obs.observation_type, "decision");

    let fetched = db.get_observation(obs.id).unwrap();
    assert_eq!(fetched.content, "Test content");
}

#[test]
fn privacy_stripping() {
    let db = test_db();
    let obs = db
        .save_observation(
            "Keys",
            "Use <private>sk-secret</private> for auth",
            "config",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(obs.content, "Use [REDACTED] for auth");
}

#[test]
fn topic_key_upsert() {
    let db = test_db();
    let first = db
        .save_observation(
            "Auth v1",
            "JWT tokens",
            "decision",
            Some("p"),
            "project",
            Some("arch/auth"),
            None,
            None,
        )
        .unwrap();
    assert_eq!(first.revision_count, 1);

    let second = db
        .save_observation(
            "Auth v2",
            "OAuth2 + PKCE",
            "decision",
            Some("p"),
            "project",
            Some("arch/auth"),
            None,
            None,
        )
        .unwrap();
    assert_eq!(second.id, first.id, "should update same row");
    assert_eq!(second.revision_count, 2);
    assert_eq!(second.content, "OAuth2 + PKCE");
}

#[test]
fn soft_delete() {
    let db = test_db();
    let obs = db
        .save_observation(
            "Delete me",
            "content",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    assert!(db.delete_observation(obs.id).unwrap());

    let deleted = db.get_observation(obs.id).unwrap();
    assert!(deleted.deleted_at.is_some());
}

// ─── Search ─────────────────────────────────────────────────────

#[test]
fn search_finds_results() {
    let db = test_db();
    db.save_observation(
        "Auth middleware",
        "JWT validation in Express",
        "decision",
        Some("web"),
        "project",
        None,
        None,
        None,
    )
    .unwrap();
    db.save_observation(
        "Database setup",
        "PostgreSQL with Drizzle ORM",
        "config",
        Some("web"),
        "project",
        None,
        None,
        None,
    )
    .unwrap();

    let results = db.search("JWT middleware", None, None, None).unwrap();
    assert!(!results.is_empty());
    assert!(results[0].observation.title.contains("Auth"));
}

#[test]
fn context_returns_recent() {
    let db = test_db();
    for i in 0..5 {
        db.save_observation(
            &format!("Obs {i}"),
            &format!("Content {i}"),
            "manual",
            Some("proj"),
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    }
    let ctx = db.recent_context(Some("proj"), Some(3)).unwrap();
    assert_eq!(ctx.len(), 3);
}

#[test]
fn stats_counts() {
    let db = test_db();
    db.save_observation(
        "A",
        "a",
        "decision",
        Some("p1"),
        "project",
        None,
        None,
        None,
    )
    .unwrap();
    db.save_observation("B", "b", "bugfix", Some("p1"), "project", None, None, None)
        .unwrap();
    db.save_observation(
        "C",
        "c",
        "decision",
        Some("p2"),
        "project",
        None,
        None,
        None,
    )
    .unwrap();
    db.start_session("s1", "p1", None).unwrap();

    let stats = db.stats().unwrap();
    assert_eq!(stats.total_observations, 3);
    assert_eq!(stats.total_sessions, 1);
    assert_eq!(*stats.by_type.get("decision").unwrap(), 2);
    assert_eq!(*stats.by_project.get("p1").unwrap(), 2);
}

// ─── Sessions ───────────────────────────────────────────────────

#[test]
fn session_lifecycle() {
    let db = test_db();
    let s = db.start_session("s1", "myproj", Some("/code")).unwrap();
    assert!(s.ended_at.is_none());

    let s = db.end_session("s1", Some("Done with auth")).unwrap();
    assert!(s.ended_at.is_some());
    assert_eq!(s.summary.as_deref(), Some("Done with auth"));
}

// ─── Timeline ───────────────────────────────────────────────────

#[test]
fn timeline_returns_anchor() {
    let db = test_db();
    let obs = db
        .save_observation(
            "A",
            "content a",
            "decision",
            Some("p"),
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    let tl = db.timeline(obs.id, None, None).unwrap();
    assert_eq!(tl.anchor.id, obs.id);
}

#[test]
fn timeline_returns_before_and_after() {
    let db = test_db();
    let o1 = db
        .save_observation(
            "First",
            "c1",
            "manual",
            Some("p"),
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    let o2 = db
        .save_observation(
            "Second",
            "c2",
            "manual",
            Some("p"),
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    let o3 = db
        .save_observation(
            "Third",
            "c3",
            "manual",
            Some("p"),
            "project",
            None,
            None,
            None,
        )
        .unwrap();

    let tl = db.timeline(o2.id, Some(5), Some(5)).unwrap();
    assert_eq!(tl.anchor.id, o2.id);
    assert!(
        tl.before.iter().any(|o| o.id == o1.id),
        "should include o1 before anchor"
    );
    assert!(
        tl.after.iter().any(|o| o.id == o3.id),
        "should include o3 after anchor"
    );
}

#[test]
fn timeline_respects_limits() {
    let db = test_db();
    for i in 0..10 {
        db.save_observation(
            &format!("Obs {i}"),
            &format!("content {i}"),
            "manual",
            Some("p"),
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    }
    let mid = db
        .save_observation(
            "Middle",
            "mid",
            "manual",
            Some("p"),
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    for i in 11..20 {
        db.save_observation(
            &format!("Obs {i}"),
            &format!("content {i}"),
            "manual",
            Some("p"),
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    }

    let tl = db.timeline(mid.id, Some(3), Some(2)).unwrap();
    assert_eq!(tl.before.len(), 3);
    assert_eq!(tl.after.len(), 2);
}

#[test]
fn timeline_excludes_deleted() {
    let db = test_db();
    let o1 = db
        .save_observation("A", "a", "manual", Some("p"), "project", None, None, None)
        .unwrap();
    let o2 = db
        .save_observation("B", "b", "manual", Some("p"), "project", None, None, None)
        .unwrap();
    let _o3 = db
        .save_observation("C", "c", "manual", Some("p"), "project", None, None, None)
        .unwrap();
    db.delete_observation(o1.id).unwrap();

    let tl = db.timeline(o2.id, Some(5), Some(5)).unwrap();
    assert!(tl.before.is_empty(), "deleted obs should not appear");
    assert_eq!(tl.after.len(), 1);
}

#[test]
fn timeline_nonexistent_id_errors() {
    let db = test_db();
    let err = db.timeline(9999, None, None);
    assert!(err.is_err());
}

#[test]
fn timeline_defaults_without_limits() {
    let db = test_db();
    for i in 0..10 {
        db.save_observation(
            &format!("Obs {i}"),
            &format!("c{i}"),
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    }
    let tl = db.timeline(5, None, None).unwrap();
    assert!(tl.before.len() <= 5);
    assert!(tl.after.len() <= 5);
}

// ─── Export / Import ────────────────────────────────────────────

#[test]
fn export_includes_all_data() {
    let db = test_db();
    db.save_observation(
        "A",
        "a",
        "decision",
        Some("p1"),
        "project",
        None,
        None,
        None,
    )
    .unwrap();
    db.save_observation("B", "b", "bugfix", Some("p1"), "project", None, None, None)
        .unwrap();
    db.start_session("s1", "p1", None).unwrap();

    let data = db.export_all().unwrap();
    assert_eq!(data.observations.len(), 2);
    assert_eq!(data.sessions.len(), 1);
    assert_eq!(data.version, 1);
    assert!(!data.exported_at.is_empty());
}

#[test]
fn export_includes_deleted() {
    let db = test_db();
    let obs = db
        .save_observation("A", "a", "manual", None, "project", None, None, None)
        .unwrap();
    db.delete_observation(obs.id).unwrap();

    let data = db.export_all().unwrap();
    assert_eq!(
        data.observations.len(),
        1,
        "export should include soft-deleted"
    );
}

#[test]
fn import_into_empty_db() {
    let db1 = test_db();
    db1.save_observation("A", "a", "decision", Some("p"), "project", None, None, None)
        .unwrap();
    db1.start_session("s1", "p", Some("/code")).unwrap();
    let data = db1.export_all().unwrap();

    let db2 = test_db();
    let result = db2.import_data(&data).unwrap();
    assert_eq!(result.observations_imported, 1);
    assert_eq!(result.sessions_imported, 1);
}

#[test]
fn import_deduplicates_by_hash() {
    let db = test_db();
    db.save_observation(
        "A",
        "same content",
        "decision",
        Some("p"),
        "project",
        None,
        None,
        None,
    )
    .unwrap();
    let data = db.export_all().unwrap();

    let result = db.import_data(&data).unwrap();
    assert_eq!(result.observations_imported, 0);
    assert_eq!(result.observations_skipped, 1);
}

#[test]
fn import_deduplicates_sessions_by_id() {
    let db = test_db();
    db.start_session("s1", "p", None).unwrap();
    let data = db.export_all().unwrap();

    let result = db.import_data(&data).unwrap();
    assert_eq!(result.sessions_imported, 0);
    assert_eq!(result.sessions_skipped, 1);
}

#[test]
fn import_roundtrip_preserves_data() {
    let db1 = test_db();
    db1.save_observation(
        "Title",
        "Content here",
        "architecture",
        Some("proj"),
        "project",
        Some("arch/auth"),
        Some(&["rust".to_string(), "auth".to_string()]),
        None,
    )
    .unwrap();
    let data = db1.export_all().unwrap();

    let db2 = test_db();
    db2.import_data(&data).unwrap();

    let ctx = db2.recent_context(Some("proj"), Some(1)).unwrap();
    assert_eq!(ctx.len(), 1);
    assert_eq!(ctx[0].title, "Title");
    assert_eq!(ctx[0].content, "Content here");
    assert_eq!(ctx[0].observation_type, "architecture");
    assert_eq!(ctx[0].topic_key.as_deref(), Some("arch/auth"));
    assert_eq!(ctx[0].tags.as_ref().unwrap().len(), 2);
}

// ─── Purge ──────────────────────────────────────────────────────

#[test]
fn purge_removes_old_deleted() {
    let db = test_db();
    let obs = db
        .save_observation(
            "Old",
            "old content",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    db.delete_observation(obs.id).unwrap();

    db.conn
        .execute(
            "UPDATE observations SET deleted_at = datetime('now', '-60 days') WHERE id = ?1",
            params![obs.id],
        )
        .unwrap();

    let result = db.purge(30).unwrap();
    assert_eq!(result.observations_purged, 1);

    let err = db.get_observation(obs.id);
    assert!(err.is_err(), "purged observation should not exist");
}

#[test]
fn purge_keeps_recently_deleted() {
    let db = test_db();
    let obs = db
        .save_observation(
            "Recent",
            "recent content",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    db.delete_observation(obs.id).unwrap();

    let result = db.purge(30).unwrap();
    assert_eq!(result.observations_purged, 0);

    let fetched = db.get_observation(obs.id).unwrap();
    assert!(fetched.deleted_at.is_some());
}

#[test]
fn purge_keeps_non_deleted() {
    let db = test_db();
    db.save_observation(
        "Active",
        "active content",
        "manual",
        None,
        "project",
        None,
        None,
        None,
    )
    .unwrap();

    let result = db.purge(0).unwrap();
    assert_eq!(result.observations_purged, 0);
}

#[test]
fn purge_with_zero_days_purges_all_deleted() {
    let db = test_db();
    let obs = db
        .save_observation("A", "a", "manual", None, "project", None, None, None)
        .unwrap();
    db.delete_observation(obs.id).unwrap();

    let result = db.purge(0).unwrap();
    assert_eq!(result.observations_purged, 1);
}

#[test]
fn purge_rejects_negative_days() {
    let db = test_db();
    let err = db.purge(-1);
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err().code,
        crate::errors::ErrorCode::ValidationError
    );
}

// ─── Validation ─────────────────────────────────────────────────

#[test]
fn save_rejects_empty_title() {
    let db = test_db();
    let err = db.save_observation("", "content", "decision", None, "project", None, None, None);
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err().code,
        crate::errors::ErrorCode::ValidationError
    );
}

#[test]
fn save_rejects_invalid_type() {
    let db = test_db();
    let err = db.save_observation(
        "Title", "content", "invalid", None, "project", None, None, None,
    );
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err().code,
        crate::errors::ErrorCode::ValidationError
    );
}

#[test]
fn save_rejects_invalid_scope() {
    let db = test_db();
    let err = db.save_observation(
        "Title", "content", "decision", None, "global", None, None, None,
    );
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err().code,
        crate::errors::ErrorCode::ValidationError
    );
}

#[test]
fn search_rejects_empty_query() {
    let db = test_db();
    let err = db.search("", None, None, None);
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err().code,
        crate::errors::ErrorCode::ValidationError
    );
}

#[test]
fn search_rejects_zero_limit() {
    let db = test_db();
    let err = db.search("test", None, None, Some(0));
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err().code,
        crate::errors::ErrorCode::ValidationError
    );
}

#[test]
fn update_rejects_empty_fields() {
    let db = test_db();
    let obs = db
        .save_observation("T", "C", "manual", None, "project", None, None, None)
        .unwrap();
    let err = db.update_observation(obs.id, None, None, None, None, None);
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err().code,
        crate::errors::ErrorCode::ValidationError
    );
}

#[test]
fn update_rejects_invalid_type() {
    let db = test_db();
    let obs = db
        .save_observation("T", "C", "manual", None, "project", None, None, None)
        .unwrap();
    let err = db.update_observation(obs.id, None, None, Some("nope"), None, None);
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err().code,
        crate::errors::ErrorCode::ValidationError
    );
}

#[test]
fn session_rejects_empty_id() {
    let db = test_db();
    let err = db.start_session("", "proj", None);
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err().code,
        crate::errors::ErrorCode::ValidationError
    );
}

#[test]
fn session_rejects_empty_project() {
    let db = test_db();
    let err = db.start_session("s1", "", None);
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err().code,
        crate::errors::ErrorCode::ValidationError
    );
}

// ─── Plans lifecycle ────────────────────────────────────────────

#[test]
fn save_plan_and_delete_on_completion() {
    let db = test_db();
    let plan = db
        .save_observation(
            "Implement HTTP API",
            "1. Add axum\n2. Create routes\n3. Add tests",
            "plan",
            Some("igris"),
            "project",
            Some("plan/http-api"),
            None,
            None,
        )
        .unwrap();
    assert_eq!(plan.observation_type, "plan");

    // Update plan progress via topic_key
    let updated = db
        .save_observation(
            "Implement HTTP API",
            "1. ✅ Add axum\n2. ✅ Create routes\n3. Add tests",
            "plan",
            Some("igris"),
            "project",
            Some("plan/http-api"),
            None,
            None,
        )
        .unwrap();
    assert_eq!(updated.id, plan.id, "topic_key upsert");
    assert_eq!(updated.revision_count, 2);

    // Plan completed → delete
    assert!(db.delete_observation(plan.id).unwrap());

    // Plan no longer appears in context
    let ctx = db.recent_context(Some("igris"), None).unwrap();
    assert!(ctx.is_empty());
}

// ─── Edge cases ─────────────────────────────────────────────────

#[test]
fn deduplication_within_window() {
    let db = test_db();
    let first = db
        .save_observation(
            "Same",
            "identical content",
            "manual",
            Some("p"),
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    let second = db
        .save_observation(
            "Same",
            "identical content",
            "manual",
            Some("p"),
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        first.id, second.id,
        "same content within window should dedup"
    );
    assert_eq!(second.duplicate_count, 2);
}

#[test]
fn unicode_in_title_and_content() {
    let db = test_db();
    let obs = db
        .save_observation(
            "认证设计 🔐",
            "使用JWT进行API认证 — très bien",
            "decision",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    let fetched = db.get_observation(obs.id).unwrap();
    assert_eq!(fetched.title, "认证设计 🔐");
    assert!(fetched.content.contains("très bien"));
}

#[test]
fn very_long_content() {
    let db = test_db();
    let long_content = "x".repeat(50_000);
    let obs = db
        .save_observation(
            "Long",
            &long_content,
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    let fetched = db.get_observation(obs.id).unwrap();
    assert_eq!(fetched.content.len(), 50_000);
}

#[test]
fn tags_with_special_chars() {
    let db = test_db();
    let tags = vec![
        "c++".to_string(),
        "node.js".to_string(),
        "émoji🎉".to_string(),
    ];
    let obs = db
        .save_observation("T", "C", "manual", None, "project", None, Some(&tags), None)
        .unwrap();
    let fetched = db.get_observation(obs.id).unwrap();
    assert_eq!(fetched.tags.unwrap(), tags);
}

#[test]
fn topic_key_upsert_does_not_cross_projects() {
    let db = test_db();
    let a = db
        .save_observation(
            "Auth v1",
            "JWT",
            "decision",
            Some("proj-a"),
            "project",
            Some("arch/auth"),
            None,
            None,
        )
        .unwrap();
    let b = db
        .save_observation(
            "Auth v1",
            "OAuth",
            "decision",
            Some("proj-b"),
            "project",
            Some("arch/auth"),
            None,
            None,
        )
        .unwrap();
    assert_ne!(
        a.id, b.id,
        "same topic_key in different projects should NOT upsert"
    );
}

#[test]
fn search_with_max_limit() {
    let db = test_db();
    for i in 0..60 {
        db.save_observation(
            &format!("Obs {i}"),
            &format!("content {i}"),
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    }
    let results = db.search("content", None, None, Some(50)).unwrap();
    assert!(results.len() <= 50, "should cap at 50");
}

#[test]
fn search_over_limit_capped() {
    let db = test_db();
    db.save_observation(
        "A",
        "test content",
        "manual",
        None,
        "project",
        None,
        None,
        None,
    )
    .unwrap();
    let results = db.search("test", None, None, Some(999)).unwrap();
    assert!(!results.is_empty()); // works, but internally capped at 50
}

#[test]
fn double_delete_returns_false() {
    let db = test_db();
    let obs = db
        .save_observation("T", "C", "manual", None, "project", None, None, None)
        .unwrap();
    assert!(db.delete_observation(obs.id).unwrap());
    assert!(
        !db.delete_observation(obs.id).unwrap(),
        "second delete should return false"
    );
}

#[test]
fn search_excludes_deleted() {
    let db = test_db();
    let obs = db
        .save_observation(
            "Findable",
            "unique keyword xyzzy",
            "manual",
            None,
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    db.delete_observation(obs.id).unwrap();
    let results = db.search("xyzzy", None, None, None).unwrap();
    assert!(
        results.is_empty(),
        "deleted observations should not appear in search"
    );
}

#[test]
fn context_excludes_deleted() {
    let db = test_db();
    let obs = db
        .save_observation("T", "C", "manual", Some("p"), "project", None, None, None)
        .unwrap();
    db.delete_observation(obs.id).unwrap();
    let ctx = db.recent_context(Some("p"), None).unwrap();
    assert!(ctx.is_empty());
}

// ─── Encryption ─────────────────────────────────────────────────

#[test]
fn open_without_key_works() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let db = Database::open(&path, None).unwrap();
    db.save_observation("T", "C", "manual", None, "project", None, None, None)
        .unwrap();
    let obs = db.recent_context(None, Some(1)).unwrap();
    assert_eq!(obs.len(), 1);
}

#[test]
fn open_with_key_encrypts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("encrypted.db");

    // Create encrypted DB
    let db = Database::open(&path, Some("secret123")).unwrap();
    db.save_observation(
        "Secret", "data", "manual", None, "project", None, None, None,
    )
    .unwrap();
    drop(db);

    // Re-open with correct key works
    let db = Database::open(&path, Some("secret123")).unwrap();
    let obs = db.recent_context(None, Some(1)).unwrap();
    assert_eq!(obs[0].title, "Secret");
}

#[test]
fn open_with_wrong_key_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("encrypted.db");

    // Create encrypted DB
    let db = Database::open(&path, Some("correct_key")).unwrap();
    db.save_observation("T", "C", "manual", None, "project", None, None, None)
        .unwrap();
    drop(db);

    // Re-open with wrong key fails
    let result = Database::open(&path, Some("wrong_key"));
    assert!(
        result.is_err(),
        "wrong key should fail to open encrypted DB"
    );
}

#[test]
fn encrypted_db_crud_works() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crud.db");
    let db = Database::open(&path, Some("mykey")).unwrap();

    // Save
    let obs = db
        .save_observation(
            "Auth design",
            "JWT tokens",
            "decision",
            Some("web"),
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(obs.title, "Auth design");

    // Get
    let fetched = db.get_observation(obs.id).unwrap();
    assert_eq!(fetched.content, "JWT tokens");

    // Search
    let results = db.search("JWT", None, None, None).unwrap();
    assert!(!results.is_empty());

    // Delete
    assert!(db.delete_observation(obs.id).unwrap());
}

#[test]
fn encryption_key_with_special_chars() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("special.db");
    let key = "p@ss'w\"ord with spaces & émojis 🔑";
    let db = Database::open(&path, Some(key)).unwrap();
    db.save_observation("T", "C", "manual", None, "project", None, None, None)
        .unwrap();
    drop(db);

    let db = Database::open(&path, Some(key)).unwrap();
    let obs = db.recent_context(None, Some(1)).unwrap();
    assert_eq!(obs.len(), 1);
}
