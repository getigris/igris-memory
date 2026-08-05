use crate::codegraph::extractor::{ExtractionResult, LanguageExtractor};
use crate::codegraph::language::Language;
use crate::codegraph::query_runner::{QuerySpec, run_query};

const TS_QUERY_SRC: &str = include_str!("../queries/typescript.scm");
const TSX_QUERY_SRC: &str = include_str!("../queries/tsx.scm");

pub struct TypeScriptExtractor;

impl LanguageExtractor for TypeScriptExtractor {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn extract(&self, source: &str) -> ExtractionResult {
        run_query(
            source,
            &QuerySpec {
                ts_language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                query_src: TS_QUERY_SRC,
                function_symbol_kind: "function",
            },
        )
    }
}

pub struct TsxExtractor;

impl LanguageExtractor for TsxExtractor {
    fn language(&self) -> Language {
        Language::Tsx
    }

    fn extract(&self, source: &str) -> ExtractionResult {
        run_query(
            source,
            &QuerySpec {
                ts_language: tree_sitter_typescript::LANGUAGE_TSX.into(),
                query_src: TSX_QUERY_SRC,
                function_symbol_kind: "function",
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"
import { readFileSync } from 'fs';

function helper(): void {}

function main(): void {
  helper();
}
"#;

    #[test]
    fn typescript_extracts_functions_calls_and_imports() {
        let result = TypeScriptExtractor.extract(SOURCE);
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
        assert!(imports.iter().any(|i| i.contains("fs")));
    }

    #[test]
    fn tsx_extracts_functions_and_calls_from_the_same_syntax() {
        let result = TsxExtractor.extract(SOURCE);
        let fn_names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(fn_names.contains(&"helper"));
        assert!(fn_names.contains(&"main"));
    }
}
