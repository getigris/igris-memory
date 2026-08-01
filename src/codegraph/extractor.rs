use std::collections::HashMap;

use super::language::Language;

/// A function/method/class/etc. found in a source file by a `LanguageExtractor`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ExtractedSymbol {
    pub kind: String,
    pub name: String,
    pub qualified_name: String,
    pub start_line: i64,
    pub end_line: i64,
}

/// A relation found in a source file — an import, a call, a definition.
/// `dst_name` is the raw name/path as written in source; resolving it to an
/// actual `CodeFile`/`CodeSymbol` id happens later, in the DB layer (Task 7),
/// not in the extractor.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ExtractedEdge {
    pub relation: String,
    pub src_qualified_name: Option<String>,
    pub dst_name: String,
    pub resolution: String,
    pub external_boundary: bool,
}

/// Everything extracted from one source file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ExtractionResult {
    pub symbols: Vec<ExtractedSymbol>,
    pub edges: Vec<ExtractedEdge>,
}

/// Implemented once per language. `extract` must never panic on malformed
/// input — tree-sitter parsers are error-tolerant by design, so a syntax
/// error should just mean fewer/incomplete captures, not a crash.
#[allow(dead_code)]
pub trait LanguageExtractor: Send + Sync {
    fn language(&self) -> Language;
    fn extract(&self, source: &str) -> ExtractionResult;
}

/// Looks up the right `LanguageExtractor` for a given `Language`.
#[derive(Default)]
#[allow(dead_code)]
pub struct LanguageExtractorRegistry {
    extractors: HashMap<Language, Box<dyn LanguageExtractor>>,
}

impl LanguageExtractorRegistry {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            extractors: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn register(&mut self, extractor: Box<dyn LanguageExtractor>) {
        self.extractors.insert(extractor.language(), extractor);
    }

    #[allow(dead_code)]
    pub fn get(&self, language: Language) -> Option<&dyn LanguageExtractor> {
        self.extractors.get(&language).map(|b| b.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeExtractor;
    impl LanguageExtractor for FakeExtractor {
        fn language(&self) -> Language {
            Language::Rust
        }
        fn extract(&self, _source: &str) -> ExtractionResult {
            ExtractionResult {
                symbols: vec![ExtractedSymbol {
                    kind: "function".into(),
                    name: "foo".into(),
                    qualified_name: "foo".into(),
                    start_line: 1,
                    end_line: 1,
                }],
                edges: vec![],
            }
        }
    }

    #[test]
    fn registry_returns_registered_extractor_by_language() {
        let mut registry = LanguageExtractorRegistry::new();
        registry.register(Box::new(FakeExtractor));

        let extractor = registry
            .get(Language::Rust)
            .expect("rust extractor registered");
        let result = extractor.extract("fn foo() {}");
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "foo");
    }

    #[test]
    fn registry_returns_none_for_unregistered_language() {
        let registry = LanguageExtractorRegistry::new();
        assert!(registry.get(Language::Python).is_none());
    }
}
