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
/// capture into an `ExtractionResult`. A symbol's line range comes from the
/// match's `@definition.*` capture (the whole definition), falling back to the
/// `@name.*` identifier token if a query emits no wrapper capture.
///
/// Never panics on malformed input or a query that fails to compile — both
/// degrade to an empty result, since a syntax error in one file should never
/// take down the rest of an index run.
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
        // Every `.scm` file wraps its `@name.function` capture in a
        // `@definition.function` capture spanning the whole definition. The
        // name capture is only the identifier token, so its line range is
        // always a single line — the wrapper's range is the symbol's real
        // body span.
        let definition_range = m.captures.iter().find_map(|c| {
            let capture_name = query.capture_names()[c.index as usize];
            capture_name.starts_with("definition.").then(|| {
                (
                    c.node.start_position().row as i64 + 1,
                    c.node.end_position().row as i64 + 1,
                )
            })
        });

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
                "name.function" => {
                    let (start_line, end_line) = definition_range.unwrap_or((start_line, end_line));
                    result.symbols.push(ExtractedSymbol {
                        kind: spec.function_symbol_kind.to_string(),
                        name: text.clone(),
                        qualified_name: text,
                        start_line,
                        end_line,
                    })
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    const RUST_QUERY_SRC: &str = include_str!("queries/rust.scm");

    #[test]
    fn definition_range_is_1_indexed_and_exact() {
        // Line 1: "fn a() {}"      -> single-line definition, rows [0, 0]
        // Line 2: "fn b() {"       -> multi-line definition starts here
        // Line 3: "    1"
        // Line 4: "}"              -> multi-line definition ends here
        let src = "fn a() {}\nfn b() {\n    1\n}\n";
        let spec = QuerySpec {
            ts_language: tree_sitter_rust::LANGUAGE.into(),
            query_src: RUST_QUERY_SRC,
            function_symbol_kind: "function",
        };

        let result = run_query(src, &spec);

        let a = result
            .symbols
            .iter()
            .find(|s| s.name == "a")
            .expect("`a` should be extracted");
        assert_eq!(a.start_line, 1, "unexpected start line: {a:?}");
        assert_eq!(a.end_line, 1, "unexpected end line: {a:?}");

        let b = result
            .symbols
            .iter()
            .find(|s| s.name == "b")
            .expect("`b` should be extracted");
        assert_eq!(b.start_line, 2, "unexpected start line: {b:?}");
        assert_eq!(b.end_line, 4, "unexpected end line: {b:?}");
    }

    #[test]
    fn falls_back_to_name_capture_range_without_definition_wrapper() {
        // A query that captures `@name.function` without a wrapping
        // `@definition.*` capture never populates `definition_range`, so
        // `run_query` must fall back to the `@name.function` token's own
        // (1-indexed) row range — the `capture.node.start/end_position().row
        // as i64 + 1` computation this test targets directly.
        //
        // "b" (the identifier, not the whole `fn b() {...}` item) sits
        // entirely on line 2 of the source below, so both start and end
        // line must be exactly 2.
        let src = "fn a() {}\nfn b() {\n    1\n}\n";
        let spec = QuerySpec {
            ts_language: tree_sitter_rust::LANGUAGE.into(),
            query_src: "(function_item name: (identifier) @name.function)",
            function_symbol_kind: "function",
        };

        let result = run_query(src, &spec);

        let b = result
            .symbols
            .iter()
            .find(|s| s.name == "b")
            .expect("`b` should be extracted");
        assert_eq!(b.start_line, 2, "unexpected start line: {b:?}");
        assert_eq!(b.end_line, 2, "unexpected end line: {b:?}");
    }
}
