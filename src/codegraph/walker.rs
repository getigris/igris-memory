use std::path::{Path, PathBuf};

use super::language::Language;

/// One file found by `discover_files`, ready to be diffed against
/// `code_files.content_hash` by the indexer (Task 8).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DiscoveredFile {
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub language: Language,
    pub content_hash: String,
}

/// Walks `root`, respecting `.gitignore` (and `.ignore`/hidden-file rules via
/// the `ignore` crate's defaults — the same walker `ripgrep` uses), and
/// returns every file whose extension maps to a supported `Language`.
/// Unreadable files (binary, permission errors) are silently skipped — they
/// were never going to parse anyway.
#[allow(dead_code)]
pub fn discover_files(root: &Path) -> Vec<DiscoveredFile> {
    let mut out = Vec::new();
    for entry in ignore::WalkBuilder::new(root).build() {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let Some(language) = Language::from_extension(ext) else {
            continue;
        };
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push(DiscoveredFile {
            relative_path,
            absolute_path: path.to_path_buf(),
            language,
            content_hash: crate::utils::hash_content(&content),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn discover_files_finds_supported_files_and_skips_unsupported() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("README.md"), "# hi").unwrap();

        let files = discover_files(dir.path());

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "main.rs");
        assert_eq!(files[0].language, Language::Rust);
        assert!(!files[0].content_hash.is_empty());
    }

    #[test]
    fn discover_files_respects_gitignore() {
        let dir = tempdir().unwrap();
        // The ignore crate requires a .git directory to respect .gitignore files.
        // GIT_DIR/GIT_WORK_TREE (etc.) are set by git itself around hook
        // subprocesses (e.g. this test running under `git commit`'s
        // pre-commit hook) and leak into this nested `git init`, pointing it
        // at the outer repo instead of `dir` — clear them so init is scoped
        // to the fresh tempdir regardless of the calling process's env.
        std::process::Command::new("git")
            .args(&["init", "--quiet"])
            .current_dir(dir.path())
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .output()
            .ok();
        fs::write(dir.path().join(".gitignore"), "ignored.rs\n").unwrap();
        fs::write(dir.path().join("ignored.rs"), "fn a() {}").unwrap();
        fs::write(dir.path().join("kept.rs"), "fn b() {}").unwrap();

        let files = discover_files(dir.path());

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "kept.rs");
    }
}
