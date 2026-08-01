/// A source language the code graph can index. One variant per Tier 1
/// language from the design spec (docs/superpowers/specs/2026-07-31-code-graph-design.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum Language {
    Rust,
    JavaScript,
    TypeScript,
    Tsx,
    Python,
    Go,
    Java,
    C,
    Cpp,
    CSharp,
    Ruby,
    Php,
    Swift,
    Kotlin,
}

impl Language {
    /// Maps a file extension (no leading dot) to a `Language`, or `None` if
    /// unsupported. Files with an unsupported extension are skipped by the
    /// walker (Task 4), not treated as an error.
    #[allow(dead_code)]
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Language::Rust),
            "js" | "jsx" | "mjs" | "cjs" => Some(Language::JavaScript),
            "ts" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            "py" => Some(Language::Python),
            "go" => Some(Language::Go),
            "java" => Some(Language::Java),
            "c" | "h" => Some(Language::C),
            "cpp" | "cc" | "cxx" | "hpp" | "hh" => Some(Language::Cpp),
            "cs" => Some(Language::CSharp),
            "rb" => Some(Language::Ruby),
            "php" => Some(Language::Php),
            "swift" => Some(Language::Swift),
            "kt" | "kts" => Some(Language::Kotlin),
            _ => None,
        }
    }

    /// Stable string form stored in `code_files.language`.
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Tsx => "tsx",
            Language::Python => "python",
            Language::Go => "go",
            Language::Java => "java",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::CSharp => "csharp",
            Language::Ruby => "ruby",
            Language::Php => "php",
            Language::Swift => "swift",
            Language::Kotlin => "kotlin",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_extension_maps_known_extensions() {
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
        assert_eq!(Language::from_extension("tsx"), Some(Language::Tsx));
        assert_eq!(Language::from_extension("kt"), Some(Language::Kotlin));
    }

    #[test]
    fn from_extension_returns_none_for_unknown() {
        assert_eq!(Language::from_extension("md"), None);
        assert_eq!(Language::from_extension(""), None);
    }

    #[test]
    fn as_str_round_trips_to_a_stable_name() {
        assert_eq!(Language::Rust.as_str(), "rust");
        assert_eq!(Language::CSharp.as_str(), "csharp");
    }
}
