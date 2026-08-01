use crate::codegraph::{ExtractedEdge, ExtractedSymbol};
use crate::db::Database;

#[test]
fn upsert_code_file_is_idempotent_by_path() {
    let db = Database::open_in_memory().unwrap();
    let f1 = db
        .upsert_code_file("igris-memory", "/repo", "src/main.rs", "rust", "hash1")
        .unwrap();
    let f2 = db
        .upsert_code_file("igris-memory", "/repo", "src/main.rs", "rust", "hash2")
        .unwrap();
    assert_eq!(f1.id, f2.id);
    assert_eq!(f2.content_hash, "hash2");
}

#[test]
fn replace_symbols_and_edges_resolves_call_within_same_project() {
    let db = Database::open_in_memory().unwrap();
    let file = db
        .upsert_code_file("igris-memory", "/repo", "src/lib.rs", "rust", "h1")
        .unwrap();

    let symbols = vec![
        ExtractedSymbol {
            kind: "function".into(),
            name: "helper".into(),
            qualified_name: "helper".into(),
            start_line: 1,
            end_line: 1,
        },
        ExtractedSymbol {
            kind: "function".into(),
            name: "main".into(),
            qualified_name: "main".into(),
            start_line: 3,
            end_line: 5,
        },
    ];
    let edges = vec![ExtractedEdge {
        relation: "calls".into(),
        src_qualified_name: Some("main".into()),
        dst_name: "helper".into(),
        resolution: "heuristic".into(),
        external_boundary: false,
    }];
    db.replace_symbols_and_edges_for_file(file.id, &symbols, &edges)
        .unwrap();

    let nodes = db
        .search_code_nodes("helper", None, None, Some("igris-memory"), 10)
        .unwrap();
    assert_eq!(nodes.len(), 1);

    let crate::models::CodeNode::Symbol(helper_symbol) = &nodes[0] else {
        panic!("expected a symbol node");
    };
    let neighbors = db
        .code_neighbors("symbol", helper_symbol.id, 1, "in", None)
        .unwrap();
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].edge.relation, "calls");
    assert!(
        neighbors[0].node.is_some(),
        "call should resolve to the `main` symbol in the same project"
    );
}

#[test]
fn soft_delete_missing_code_files_marks_absent_files_deleted() {
    let db = Database::open_in_memory().unwrap();
    db.upsert_code_file("igris-memory", "/repo", "src/a.rs", "rust", "h1")
        .unwrap();
    db.upsert_code_file("igris-memory", "/repo", "src/b.rs", "rust", "h1")
        .unwrap();

    let deleted = db
        .soft_delete_missing_code_files("igris-memory", "/repo", &["src/a.rs".to_string()])
        .unwrap();

    assert_eq!(deleted, 1);
    let hash = db
        .get_code_file_hash("igris-memory", "/repo", "src/b.rs")
        .unwrap();
    assert!(
        hash.is_none(),
        "soft-deleted files should not resolve a hash"
    );
}
