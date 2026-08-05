use crate::db::Database;
use crate::store::BrainStore;

/// Two soft-deleted entities in the import payload must both be counted
/// as skipped (kills `ent_skipped += 1` -> `-=`/`*=` mutants on the
/// deleted_at branch of `import_data_with_progress`).
#[test]
fn import_entities_counts_multiple_soft_deleted_skips() {
    let source = Database::open_in_memory().unwrap();
    source
        .upsert_entity("person", "Alice", &[], Some("proj"), "project")
        .unwrap();
    source
        .upsert_entity("person", "Bob", &[], Some("proj"), "project")
        .unwrap();
    source
        .upsert_entity("person", "Carol", &[], Some("proj"), "project")
        .unwrap();

    let mut data = source.export_all().unwrap();
    assert_eq!(data.entities.len(), 3);
    // Mark two of the three entities as soft-deleted in the payload so the
    // import loop's `deleted_at.is_some()` branch fires twice.
    data.entities[0].deleted_at = Some("2026-01-01T00:00:00Z".to_string());
    data.entities[1].deleted_at = Some("2026-01-01T00:00:00Z".to_string());

    let dest = Database::open_in_memory().unwrap();
    let result = dest.import_data(&data).unwrap();

    assert_eq!(result.entities_skipped, 2);
    assert_eq!(result.entities_imported, 1);
}

/// Two entities that already exist at the destination (same slug/project/scope)
/// must both be counted as skipped (kills `ent_skipped += 1` -> `-=`/`*=`
/// mutants on the duplicate-detection branch of `import_data_with_progress`).
#[test]
fn import_entities_counts_multiple_duplicate_skips() {
    let source = Database::open_in_memory().unwrap();
    source
        .upsert_entity("person", "Alice", &[], Some("proj"), "project")
        .unwrap();
    source
        .upsert_entity("person", "Bob", &[], Some("proj"), "project")
        .unwrap();
    let data = source.export_all().unwrap();
    assert_eq!(data.entities.len(), 2);

    let dest = Database::open_in_memory().unwrap();
    // Pre-create matching entities at the destination so both rows in `data`
    // are detected as existing (same slug + project + scope) during import.
    dest.upsert_entity("person", "Alice", &[], Some("proj"), "project")
        .unwrap();
    dest.upsert_entity("person", "Bob", &[], Some("proj"), "project")
        .unwrap();

    let result = dest.import_data(&data).unwrap();

    assert_eq!(result.entities_skipped, 2);
    assert_eq!(result.entities_imported, 0);
}
