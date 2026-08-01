use crate::codegraph::extractor::{ExtractionResult, LanguageExtractor};
use crate::codegraph::language::Language;
use crate::codegraph::query_runner::{QuerySpec, run_query};

const QUERY_SRC: &str = include_str!("../queries/rust.scm");

pub struct RustExtractor;

impl LanguageExtractor for RustExtractor {
    fn language(&self) -> Language {
        Language::Rust
    }

    fn extract(&self, source: &str) -> ExtractionResult {
        run_query(
            source,
            &QuerySpec {
                ts_language: tree_sitter_rust::LANGUAGE.into(),
                query_src: QUERY_SRC,
                function_symbol_kind: "function",
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegraph::LanguageExtractor;

    #[test]
    fn extracts_functions_calls_and_imports() {
        let source = r#"
use std::collections::HashMap;

fn helper() {}

fn main() {
    helper();
}
"#;
        let result = RustExtractor.extract(source);

        let fn_names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(fn_names.contains(&"helper"));
        assert!(fn_names.contains(&"main"));

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
        assert!(imports.iter().any(|i| i.contains("HashMap")));
    }
}
