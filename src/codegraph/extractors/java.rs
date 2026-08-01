use crate::codegraph::extractor::{ExtractionResult, LanguageExtractor};
use crate::codegraph::language::Language;
use crate::codegraph::query_runner::{QuerySpec, run_query};

const QUERY_SRC: &str = include_str!("../queries/java.scm");

pub struct JavaExtractor;

impl LanguageExtractor for JavaExtractor {
    fn language(&self) -> Language {
        Language::Java
    }

    fn extract(&self, source: &str) -> ExtractionResult {
        run_query(
            source,
            &QuerySpec {
                ts_language: tree_sitter_java::LANGUAGE.into(),
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
    fn extracts_methods_calls_and_imports() {
        let source = "import java.util.List;\n\nclass Main {\n    void helper() {}\n\n    void main() {\n        helper();\n    }\n}\n";
        let result = JavaExtractor.extract(source);

        let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"helper"));
        assert!(names.contains(&"main"));
        assert!(result.symbols.iter().all(|s| s.kind == "method"));

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
        assert!(imports.iter().any(|i| i.contains("List")));
    }
}
