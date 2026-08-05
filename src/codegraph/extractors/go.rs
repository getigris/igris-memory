use crate::codegraph::extractor::{ExtractionResult, LanguageExtractor};
use crate::codegraph::language::Language;
use crate::codegraph::query_runner::{QuerySpec, run_query};

const QUERY_SRC: &str = include_str!("../queries/go.scm");

pub struct GoExtractor;

impl LanguageExtractor for GoExtractor {
    fn language(&self) -> Language {
        Language::Go
    }

    fn extract(&self, source: &str) -> ExtractionResult {
        run_query(
            source,
            &QuerySpec {
                ts_language: tree_sitter_go::LANGUAGE.into(),
                query_src: QUERY_SRC,
                function_symbol_kind: "function",
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_functions_calls_and_imports() {
        let source = "package main\n\nimport \"fmt\"\n\nfunc helper() {}\n\nfunc main() {\n    helper()\n    fmt.Println(\"hi\")\n}\n";
        let result = GoExtractor.extract(source);

        let fn_names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(fn_names.contains(&"helper"));
        assert!(fn_names.contains(&"main"));

        let calls: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.dst_name.as_str())
            .collect();
        assert!(
            calls.contains(&"helper"),
            "plain identifier calls like helper() must resolve; fmt.Println is a selector expression and is not expected to match this simple query"
        );

        let imports: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "imports")
            .map(|e| e.dst_name.as_str())
            .collect();
        assert!(imports.iter().any(|i| i.contains("fmt")));
    }
}
