use crate::codegraph::extractor::{ExtractionResult, LanguageExtractor};
use crate::codegraph::language::Language;
use crate::codegraph::query_runner::{QuerySpec, run_query};

const QUERY_SRC: &str = include_str!("../queries/swift.scm");

pub struct SwiftExtractor;

impl LanguageExtractor for SwiftExtractor {
    fn language(&self) -> Language {
        Language::Swift
    }

    fn extract(&self, source: &str) -> ExtractionResult {
        run_query(
            source,
            &QuerySpec {
                ts_language: tree_sitter_swift::LANGUAGE.into(),
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
    fn extracts_functions_and_calls() {
        let source = "import Foundation\n\nfunc helper() {}\n\nfunc main() {\n    helper()\n}\n";
        let result = SwiftExtractor.extract(source);

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
    }
}
