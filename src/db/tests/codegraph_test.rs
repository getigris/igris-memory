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
fn search_results_carry_the_owning_file_path_and_language() {
    let db = Database::open_in_memory().unwrap();
    let file = db
        .upsert_code_file("igris-memory", "/repo", "src/db/codegraph.rs", "rust", "h1")
        .unwrap();
    let symbols = vec![ExtractedSymbol {
        kind: "function".into(),
        name: "search_code_nodes".into(),
        qualified_name: "search_code_nodes".into(),
        start_line: 300,
        end_line: 330,
    }];
    db.replace_symbols_and_edges_for_file(file.id, &symbols, &[])
        .unwrap();

    let nodes = db
        .search_code_nodes("search_code", None, None, Some("igris-memory"), 10)
        .unwrap();
    let crate::models::CodeNode::Symbol(symbol) = &nodes[0] else {
        panic!("expected a symbol node");
    };
    assert_eq!(
        symbol.relative_path, "src/db/codegraph.rs",
        "a search hit must be enough to open the right file on disk"
    );
    assert_eq!(symbol.language, "rust");
    assert_eq!(symbol.start_line, 300);
}

#[test]
fn code_neighbors_expands_the_frontier_for_each_hop() {
    let db = Database::open_in_memory().unwrap();
    let file = db
        .upsert_code_file("hops-test", "/repo", "src/chain.rs", "rust", "h1")
        .unwrap();

    let symbols = ["leaf", "middle", "root"]
        .iter()
        .map(|name| ExtractedSymbol {
            kind: "function".into(),
            name: (*name).into(),
            qualified_name: (*name).into(),
            start_line: 1,
            end_line: 2,
        })
        .collect::<Vec<_>>();
    let edges = vec![
        ExtractedEdge {
            relation: "calls".into(),
            src_qualified_name: Some("root".into()),
            dst_name: "middle".into(),
            resolution: "heuristic".into(),
            external_boundary: false,
        },
        ExtractedEdge {
            relation: "calls".into(),
            src_qualified_name: Some("middle".into()),
            dst_name: "leaf".into(),
            resolution: "heuristic".into(),
            external_boundary: false,
        },
    ];
    db.replace_symbols_and_edges_for_file(file.id, &symbols, &edges)
        .unwrap();

    let nodes = db
        .search_code_nodes("root", None, None, Some("hops-test"), 10)
        .unwrap();
    let crate::models::CodeNode::Symbol(root) = &nodes[0] else {
        panic!("expected a symbol node");
    };

    let one_hop = db
        .code_neighbors("symbol", root.id, 1, "out", None)
        .unwrap();
    assert_eq!(one_hop.len(), 1, "root -> middle only");

    let two_hops = db
        .code_neighbors("symbol", root.id, 2, "out", None)
        .unwrap();
    assert_eq!(
        two_hops.len(),
        2,
        "hops=2 must also reach `leaf` through `middle`, got {two_hops:?}"
    );
    let reached: Vec<String> = two_hops
        .iter()
        .filter_map(|n| match n.node.as_ref() {
            Some(crate::models::CodeNode::Symbol(s)) => Some(s.name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(reached, vec!["middle".to_string(), "leaf".to_string()]);

    // A deeper request than the graph is deep terminates instead of looping.
    let deep = db
        .code_neighbors("symbol", root.id, 10, "both", None)
        .unwrap();
    assert_eq!(deep.len(), 2, "each edge is returned at most once");

    let err = db
        .code_neighbors("symbol", root.id, 0, "out", None)
        .unwrap_err();
    assert!(err.to_string().contains("hops"), "unexpected error: {err}");
}

#[test]
fn unresolved_import_targets_are_marked_external_boundary() {
    let db = Database::open_in_memory().unwrap();
    let file = db
        .upsert_code_file("boundary-test", "/repo", "src/lib.rs", "rust", "h1")
        .unwrap();

    let symbols = vec![ExtractedSymbol {
        kind: "function".into(),
        name: "local_helper".into(),
        qualified_name: "local_helper".into(),
        start_line: 1,
        end_line: 3,
    }];
    let edges = vec![
        ExtractedEdge {
            relation: "imports".into(),
            src_qualified_name: None,
            dst_name: "std::collections::HashMap".into(),
            resolution: "static".into(),
            external_boundary: false,
        },
        ExtractedEdge {
            relation: "calls".into(),
            src_qualified_name: None,
            dst_name: "some_unresolved_call".into(),
            resolution: "heuristic".into(),
            external_boundary: false,
        },
        ExtractedEdge {
            relation: "imports".into(),
            src_qualified_name: None,
            dst_name: "local_helper".into(),
            resolution: "static".into(),
            external_boundary: false,
        },
    ];
    db.replace_symbols_and_edges_for_file(file.id, &symbols, &edges)
        .unwrap();

    let neighbors = db.code_neighbors("file", file.id, 1, "out", None).unwrap();
    let by_name = |name: &str| {
        neighbors
            .iter()
            .find(|n| n.edge.dst_name == name)
            .unwrap_or_else(|| panic!("no edge for {name}"))
            .edge
            .clone()
    };

    assert!(
        by_name("std::collections::HashMap").external_boundary,
        "an import that resolves to nothing in-project leaves the project"
    );
    assert!(
        !by_name("local_helper").external_boundary,
        "an import resolving to an in-project symbol is not a boundary"
    );
    assert!(
        !by_name("some_unresolved_call").external_boundary,
        "an unresolved call is not proof of an external target"
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
