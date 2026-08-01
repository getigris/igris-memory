use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

use super::extractor::{ExtractedEdge, ExtractedSymbol, ExtractionResult};

/// Everything a language extractor needs to hand to `run_query`. The grammar
/// and query text are the only genuinely per-language inputs;
/// `function_symbol_kind` exists because some languages have no free
/// functions (Java, C#) and should record definitions as `"method"` instead.
pub struct QuerySpec<'a> {
    pub ts_language: tree_sitter::Language,
    pub query_src: &'a str,
    pub function_symbol_kind: &'a str,
}

/// Parses `source` with `spec.ts_language`, runs `spec.query_src` against the
/// tree, and folds every `@name.function`/`@name.call`/`@name.import`
/// capture into an `ExtractionResult`. Never panics on malformed input or a
/// query that fails to compile — both degrade to an empty result, since a
/// syntax error in one file should never take down the rest of an index run.
pub fn run_query(source: &str, spec: &QuerySpec) -> ExtractionResult {
    let mut parser = Parser::new();
    if parser.set_language(&spec.ts_language).is_err() {
        return ExtractionResult::default();
    }
    let Some(tree) = parser.parse(source, None) else {
        return ExtractionResult::default();
    };
    let Ok(query) = Query::new(&spec.ts_language, spec.query_src) else {
        return ExtractionResult::default();
    };

    let mut result = ExtractionResult::default();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    while let Some(m) = matches.next() {
        for capture in m.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            let text = capture
                .node
                .utf8_text(source.as_bytes())
                .unwrap_or_default()
                .to_string();
            let start_line = capture.node.start_position().row as i64 + 1;
            let end_line = capture.node.end_position().row as i64 + 1;

            match capture_name {
                "name.function" => result.symbols.push(ExtractedSymbol {
                    kind: spec.function_symbol_kind.to_string(),
                    name: text.clone(),
                    qualified_name: text,
                    start_line,
                    end_line,
                }),
                // `resolution: "heuristic"` — this runner does no scope
                // resolution (it can't tell a local call from a call to a
                // same-named symbol in another file). True "static"
                // resolution for unambiguous same-scope calls is follow-up
                // work, not built in this plan (see design spec's edge
                // confidence model). The DB layer (Task 7) still does
                // cross-file *name* resolution — turning `dst_name` into a
                // real `dst_id` where exactly one candidate exists.
                "name.call" => result.edges.push(ExtractedEdge {
                    relation: "calls".to_string(),
                    src_qualified_name: None,
                    dst_name: text,
                    resolution: "heuristic".to_string(),
                    external_boundary: false,
                }),
                "name.import" => result.edges.push(ExtractedEdge {
                    relation: "imports".to_string(),
                    src_qualified_name: None,
                    dst_name: text,
                    resolution: "static".to_string(),
                    external_boundary: false,
                }),
                _ => {}
            }
        }
    }
    result
}
