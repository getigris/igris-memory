use crate::codegraph::extractor::{ExtractionResult, LanguageExtractor};
use crate::codegraph::language::Language;
use crate::codegraph::query_runner::{QuerySpec, run_query};

const QUERY_SRC: &str = include_str!("../queries/csharp.scm");

pub struct CSharpExtractor;

impl LanguageExtractor for CSharpExtractor {
    fn language(&self) -> Language {
        Language::CSharp
    }

    fn extract(&self, source: &str) -> ExtractionResult {
        run_query(
            source,
            &QuerySpec {
                ts_language: tree_sitter_c_sharp::LANGUAGE.into(),
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
    fn extracts_methods_calls_and_usings() {
        let source = "using System.Collections.Generic;\n\nclass Program {\n    static void Helper() {}\n\n    static void Main() {\n        Helper();\n    }\n}\n";
        let result = CSharpExtractor.extract(source);

        let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Helper"));
        assert!(names.contains(&"Main"));
        assert!(result.symbols.iter().all(|s| s.kind == "method"));

        let calls: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.dst_name.as_str())
            .collect();
        assert!(calls.contains(&"Helper"));

        let imports: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "imports")
            .map(|e| e.dst_name.as_str())
            .collect();
        assert!(imports.iter().any(|i| i.contains("Collections")));
    }
}
