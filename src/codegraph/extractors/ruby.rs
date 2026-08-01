use crate::codegraph::extractor::{ExtractionResult, LanguageExtractor};
use crate::codegraph::language::Language;
use crate::codegraph::query_runner::{QuerySpec, run_query};

const QUERY_SRC: &str = include_str!("../queries/ruby.scm");

pub struct RubyExtractor;

impl LanguageExtractor for RubyExtractor {
    fn language(&self) -> Language {
        Language::Ruby
    }

    fn extract(&self, source: &str) -> ExtractionResult {
        run_query(
            source,
            &QuerySpec {
                ts_language: tree_sitter_ruby::LANGUAGE.into(),
                query_src: QUERY_SRC,
                function_symbol_kind: "method",
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_methods_calls_and_requires() {
        let source = "require 'json'\n\ndef helper\nend\n\ndef main\n  helper()\nend\n";
        let result = RubyExtractor.extract(source);

        let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"helper"));
        assert!(names.contains(&"main"));

        let calls: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.dst_name.as_str())
            .collect();
        assert!(calls.contains(&"helper"));

        let imports: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "imports")
            .map(|e| e.dst_name.as_str())
            .collect();
        assert!(imports.iter().any(|i| i.contains("json")));
    }
}
