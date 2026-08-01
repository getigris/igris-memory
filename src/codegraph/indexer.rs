use std::path::Path;

use crate::db::Database;
use crate::models::IndexSummary;

use super::extractor::LanguageExtractorRegistry;
use super::extractors;
use super::walker::discover_files;

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

    let mut summary = IndexSummary::default();
    let mut seen_paths = Vec::with_capacity(discovered.len());

    for file in discovered {
        seen_paths.push(file.relative_path.clone());

        let existing_hash = db
            .get_code_file_hash(project, &root_path, &file.relative_path)
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

        let code_file = match db.upsert_code_file(
            project,
            &root_path,
            &file.relative_path,
            file.language.as_str(),
            &file.content_hash,
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

        summary.files_indexed += 1;
    }

    summary.files_deleted = db
        .soft_delete_missing_code_files(project, &root_path, &seen_paths)
        .unwrap_or(0);

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
