use crate::codegraph::extractor::{ExtractionResult, LanguageExtractor};
use crate::codegraph::language::Language;
use crate::codegraph::query_runner::{QuerySpec, run_query};

const QUERY_SRC: &str = include_str!("../queries/c.scm");

pub struct CExtractor;

impl LanguageExtractor for CExtractor {
    fn language(&self) -> Language {
        Language::C
    }

    fn extract(&self, source: &str) -> ExtractionResult {
        run_query(
            source,
            &QuerySpec {
                ts_language: tree_sitter_c::LANGUAGE.into(),
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
    fn extracts_functions_calls_and_includes() {
        let source = "#include <stdio.h>\n\nvoid helper() {}\n\nint main() {\n    helper();\n    return 0;\n}\n";
        let result = CExtractor.extract(source);

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
        assert!(imports.iter().any(|i| i.contains("stdio.h")));
    }
}
