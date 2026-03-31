use crate::db::Database;
use super::*;

fn test_db_with_data() -> Database {
    let db = Database::open_in_memory().unwrap();
    for i in 0..5 {
        db.save_observation(
            &format!("Obs {i}"), &format!("Content {i}"),
            "manual", Some("proj"), "project", None, None, None,
        ).unwrap();
    }
    db.start_session("s1", "proj", Some("/code")).unwrap();
    db
}

#[test]
fn export_creates_manifest() {
    let db = test_db_with_data();
    let dir = tempfile::tempdir().unwrap();
    let manifest = export_to_dir(&db, dir.path()).unwrap();
    assert_eq!(manifest.observation_count, 5);
    assert_eq!(manifest.session_count, 1);
    assert!(dir.path().join("manifest.json").exists());
}

#[test]
fn export_creates_chunks() {
    let db = test_db_with_data();
    let dir = tempfile::tempdir().unwrap();
    export_to_dir(&db, dir.path()).unwrap();
    assert!(dir.path().join("observations/chunk_0000.json").exists());
}

#[test]
fn export_creates_sessions_file() {
    let db = test_db_with_data();
    let dir = tempfile::tempdir().unwrap();
    export_to_dir(&db, dir.path()).unwrap();
    assert!(dir.path().join("sessions.json").exists());
}

#[test]
fn import_reads_chunks() {
    let db1 = test_db_with_data();
    let dir = tempfile::tempdir().unwrap();
    export_to_dir(&db1, dir.path()).unwrap();

    let db2 = Database::open_in_memory().unwrap();
    let result = import_from_dir(&db2, dir.path()).unwrap();
    assert_eq!(result.observations_imported, 5);
    assert_eq!(result.sessions_imported, 1);
}

#[test]
fn sync_roundtrip() {
    let db1 = Database::open_in_memory().unwrap();
    db1.save_observation("Auth", "JWT tokens", "decision", Some("web"), "project", Some("arch/auth"), Some(&["rust".to_string()]), None).unwrap();
    db1.start_session("s1", "web", None).unwrap();

    let dir = tempfile::tempdir().unwrap();
    export_to_dir(&db1, dir.path()).unwrap();

    let db2 = Database::open_in_memory().unwrap();
    import_from_dir(&db2, dir.path()).unwrap();

    let obs = db2.recent_context(Some("web"), Some(1)).unwrap();
    assert_eq!(obs.len(), 1);
    assert_eq!(obs[0].title, "Auth");
    assert_eq!(obs[0].topic_key.as_deref(), Some("arch/auth"));
}

#[test]
fn import_empty_dir_errors() {
    let db = Database::open_in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let result = import_from_dir(&db, dir.path());
    assert!(result.is_err());
}
