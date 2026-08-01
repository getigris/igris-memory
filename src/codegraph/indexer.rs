use std::path::Path;

use crate::db::Database;
use crate::models::IndexSummary;

use super::extractor::LanguageExtractorRegistry;
use super::extractors;
use super::walker::{DiscoveredFile, discover_files};

/// Walks `root`, reparses every file whose content hash changed since the
/// last run (or that's new), persists the result, and soft-deletes any
/// previously-indexed file no longer found on disk.
// Not yet wired into `main`/`server` — Task 9 (background indexing on
// session start) is the caller, and lands in a later task of this plan.
#[allow(dead_code)]
pub fn index_project(db: &Database, project: &str, root: &Path) -> IndexSummary {
    let mut registry = LanguageExtractorRegistry::new();
    extractors::register_all(&mut registry);

    let root_path = root.to_string_lossy().to_string();
    let discovered = discover_files(root);

    let (mut summary, seen_paths) =
        index_discovered_files(db, project, &root_path, discovered, &registry);

    summary.files_deleted =
        match db.soft_delete_missing_code_files(project, &root_path, &seen_paths) {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(
                    project = project,
                    root_path = %root_path,
                    error = %e,
                    "soft-delete of missing code files failed"
                );
                0
            }
        };

    summary
}

/// Indexes an already-discovered set of files against `registry`, returning
/// the partial summary (`files_deleted` is filled in by the caller) plus
/// every relative path seen. Split out of `index_project` so tests can drive
/// `discovered`/`registry` combinations that the real `discover_files`/
/// `extractors::register_all` wiring never produces — every `Language`
/// `discover_files` can return currently has a registered extractor, and a
/// file discover_files just read can't fail to reread a moment later — which
/// would otherwise leave the "unsupported language" and "read failed"
/// branches below unreachable.
#[allow(dead_code)]
fn index_discovered_files(
    db: &Database,
    project: &str,
    root_path: &str,
    discovered: Vec<DiscoveredFile>,
    registry: &LanguageExtractorRegistry,
) -> (IndexSummary, Vec<String>) {
    let mut summary = IndexSummary::default();
    let mut seen_paths = Vec::with_capacity(discovered.len());

    for file in discovered {
        seen_paths.push(file.relative_path.clone());

        let existing_hash = db
            .get_code_file_hash(project, root_path, &file.relative_path)
            .unwrap_or(None);
        if existing_hash.as_deref() == Some(file.content_hash.as_str()) {
            summary.files_unchanged += 1;
            continue;
        }

        let Some(extractor) = registry.get(file.language) else {
            summary.files_skipped_unsupported += 1;
            continue;
        };

        let Ok(source) = std::fs::read_to_string(&file.absolute_path) else {
            summary.files_failed_parse += 1;
            continue;
        };

        let extraction = extractor.extract(&source);

        // Persist the file row with a hash that can never equal a real
        // content hash (empty string — `hash_content` always returns a
        // non-empty SHA-256 hex digest). If this run dies before the real
        // hash is committed below, `get_code_file_hash` won't match on the
        // next run and the file gets reparsed instead of silently staying
        // stale forever. See db::codegraph::update_code_file_hash.
        let code_file = match db.upsert_code_file(
            project,
            root_path,
            &file.relative_path,
            file.language.as_str(),
            "",
        ) {
            Ok(f) => f,
            Err(_) => {
                summary.files_failed_parse += 1;
                continue;
            }
        };

        if db
            .replace_symbols_and_edges_for_file(
                code_file.id,
                &extraction.symbols,
                &extraction.edges,
            )
            .is_err()
        {
            summary.files_failed_parse += 1;
            continue;
        }

        if db
            .update_code_file_hash(code_file.id, &file.content_hash)
            .is_err()
        {
            summary.files_failed_parse += 1;
            continue;
        }

        summary.files_indexed += 1;
    }

    (summary, seen_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegraph::Language;
    use crate::db::Database;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn index_project_indexes_new_files_and_skips_unchanged_on_second_run() {
        let db = Database::open_in_memory().unwrap();
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();

        let first = index_project(&db, "igris-memory", dir.path());
        assert_eq!(first.files_indexed, 1);
        assert_eq!(first.files_unchanged, 0);

        let second = index_project(&db, "igris-memory", dir.path());
        assert_eq!(second.files_indexed, 0);
        assert_eq!(second.files_unchanged, 1);
    }

    #[test]
    fn index_project_retries_file_that_failed_symbol_persist_on_next_run() {
        let db = Database::open_in_memory().unwrap();
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();

        // Simulate `replace_symbols_and_edges_for_file` failing after the
        // file row has already been upserted: drop the table it writes to,
        // so its first statement errors and the transaction rolls back.
        db.conn.execute_batch("DROP TABLE code_symbols;").unwrap();

        let first = index_project(&db, "igris-memory", dir.path());
        assert_eq!(first.files_failed_parse, 1);
        assert_eq!(first.files_indexed, 0);

        // Restore the table so the retry on the next run can succeed.
        db.conn.execute_batch(crate::schema::SCHEMA_V5).unwrap();

        let second = index_project(&db, "igris-memory", dir.path());
        assert_eq!(
            second.files_indexed, 1,
            "a file whose symbol/edge persist failed must be reparsed, not treated as unchanged"
        );
        assert_eq!(second.files_unchanged, 0);
    }

    #[test]
    fn index_project_soft_deletes_files_removed_from_disk() {
        let db = Database::open_in_memory().unwrap();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("gone.rs");
        fs::write(&file_path, "fn a() {}\n").unwrap();
        index_project(&db, "igris-memory", dir.path());

        fs::remove_file(&file_path).unwrap();
        let summary = index_project(&db, "igris-memory", dir.path());

        assert_eq!(summary.files_deleted, 1);
    }

    // `discover_files` only ever returns a `Language` for which
    // `extractors::register_all` has registered an extractor (they're
    // derived from the same enum), so `index_project` can never hit its
    // "unsupported language" branch through the real walker/registry. Drive
    // `index_discovered_files` directly with an empty registry to exercise
    // it anyway.
    #[test]
    fn index_project_counts_skipped_unsupported_when_registry_lacks_extractor() {
        let db = Database::open_in_memory().unwrap();
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
        let registry = LanguageExtractorRegistry::new();
        let discovered = vec![DiscoveredFile {
            relative_path: "main.rs".to_string(),
            absolute_path: dir.path().join("main.rs"),
            language: Language::Rust,
            content_hash: "irrelevant".to_string(),
        }];

        let (summary, _) =
            index_discovered_files(&db, "igris-memory", "/root", discovered, &registry);

        assert_eq!(summary.files_skipped_unsupported, 1);
        assert_eq!(summary.files_indexed, 0);
    }

    // Similarly, by the time `index_project`'s loop reads a discovered
    // file's content, `discover_files` already read it successfully once
    // (to compute its hash) — so a read failure inside the loop can't
    // happen through the real walker. Feed `index_discovered_files` a
    // `DiscoveredFile` that points at a path that was never written.
    #[test]
    fn index_project_counts_failed_parse_when_discovered_file_is_unreadable() {
        let db = Database::open_in_memory().unwrap();
        let dir = tempdir().unwrap();
        let mut registry = LanguageExtractorRegistry::new();
        extractors::register_all(&mut registry);
        let discovered = vec![DiscoveredFile {
            relative_path: "ghost.rs".to_string(),
            absolute_path: dir.path().join("ghost.rs"),
            language: Language::Rust,
            content_hash: "irrelevant".to_string(),
        }];

        let (summary, _) = index_discovered_files(
            &db,
            "igris-memory",
            &dir.path().to_string_lossy(),
            discovered,
            &registry,
        );

        assert_eq!(summary.files_failed_parse, 1);
        assert_eq!(summary.files_indexed, 0);
    }

    #[test]
    fn index_project_counts_failed_parse_when_upsert_code_file_fails() {
        let db = Database::open_in_memory().unwrap();
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();

        // Drop the table `upsert_code_file` writes to. The file still
        // passes every earlier check (readable, supported language) —
        // `get_code_file_hash`'s resulting error is swallowed by
        // `unwrap_or(None)` in the indexer, same as "never indexed before" —
        // so persistence is where it fails.
        db.conn.execute_batch("DROP TABLE code_files;").unwrap();

        let summary = index_project(&db, "igris-memory", dir.path());

        assert_eq!(summary.files_failed_parse, 1);
        assert_eq!(summary.files_indexed, 0);
    }

    #[test]
    fn index_project_counts_failed_parse_when_final_hash_commit_fails() {
        let db = Database::open_in_memory().unwrap();
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();

        // `upsert_code_file` writes a placeholder empty content_hash first,
        // then `update_code_file_hash` commits the real one. Block only
        // updates that set a non-empty content_hash so the placeholder
        // write goes through and just the final commit fails.
        db.conn
            .execute_batch(
                "CREATE TRIGGER block_final_hash_commit
                 BEFORE UPDATE OF content_hash ON code_files
                 WHEN NEW.content_hash != ''
                 BEGIN
                     SELECT RAISE(ABORT, 'blocked for test');
                 END;",
            )
            .unwrap();

        let summary = index_project(&db, "igris-memory", dir.path());

        assert_eq!(summary.files_failed_parse, 1);
        assert_eq!(summary.files_indexed, 0);
    }
}
