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
fn code_neighbors_one_hop_direction_out_excludes_incoming_edges() {
    let db = Database::open_in_memory().unwrap();
    let file = db
        .upsert_code_file("dir-test", "/repo", "src/dir.rs", "rust", "h1")
        .unwrap();

    let symbols = ["caller", "origin", "callee"]
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
            src_qualified_name: Some("caller".into()),
            dst_name: "origin".into(),
            resolution: "heuristic".into(),
            external_boundary: false,
        },
        ExtractedEdge {
            relation: "calls".into(),
            src_qualified_name: Some("origin".into()),
            dst_name: "callee".into(),
            resolution: "heuristic".into(),
            external_boundary: false,
        },
    ];
    db.replace_symbols_and_edges_for_file(file.id, &symbols, &edges)
        .unwrap();

    let nodes = db
        .search_code_nodes("origin", None, None, Some("dir-test"), 10)
        .unwrap();
    let crate::models::CodeNode::Symbol(origin) = &nodes[0] else {
        panic!("expected a symbol node");
    };

    // `origin` has one outgoing edge (-> callee) and one incoming edge
    // (<- caller). direction = "out" must surface only the former.
    let out_only = db
        .code_neighbors("symbol", origin.id, 1, "out", None)
        .unwrap();
    assert_eq!(
        out_only.len(),
        1,
        "direction=out must not also pull in the incoming edge from caller, got {out_only:?}"
    );
    let crate::models::CodeNode::Symbol(callee) = out_only[0].node.as_ref().unwrap() else {
        panic!("expected a symbol node");
    };
    assert_eq!(callee.name, "callee");

    // Symmetric check: direction = "in" must surface only the incoming edge.
    let in_only = db
        .code_neighbors("symbol", origin.id, 1, "in", None)
        .unwrap();
    assert_eq!(
        in_only.len(),
        1,
        "direction=in must not also pull in the outgoing edge to callee, got {in_only:?}"
    );
    let crate::models::CodeNode::Symbol(caller) = in_only[0].node.as_ref().unwrap() else {
        panic!("expected a symbol node");
    };
    assert_eq!(caller.name, "caller");
}

#[test]
fn code_neighbors_one_hop_resolves_symbol_and_file_neighbors_of_shared_origin() {
    let db = Database::open_in_memory().unwrap();
    let file = db
        .upsert_code_file("shared-origin", "/repo", "src/shared.rs", "rust", "h1")
        .unwrap();

    let symbols = vec![
        ExtractedSymbol {
            kind: "function".into(),
            name: "target".into(),
            qualified_name: "target".into(),
            start_line: 1,
            end_line: 2,
        },
        ExtractedSymbol {
            kind: "function".into(),
            name: "caller".into(),
            qualified_name: "caller".into(),
            start_line: 4,
            end_line: 6,
        },
    ];
    let edges = vec![
        // A file-level edge (no src_qualified_name -> src_type = "file")
        // pointing at `target`.
        ExtractedEdge {
            relation: "imports".into(),
            src_qualified_name: None,
            dst_name: "target".into(),
            resolution: "static".into(),
            external_boundary: false,
        },
        // A symbol-level edge pointing at the same `target`.
        ExtractedEdge {
            relation: "calls".into(),
            src_qualified_name: Some("caller".into()),
            dst_name: "target".into(),
            resolution: "heuristic".into(),
            external_boundary: false,
        },
    ];
    db.replace_symbols_and_edges_for_file(file.id, &symbols, &edges)
        .unwrap();

    let nodes = db
        .search_code_nodes("target", None, None, Some("shared-origin"), 10)
        .unwrap();
    let crate::models::CodeNode::Symbol(target) = &nodes[0] else {
        panic!("expected a symbol node");
    };

    // `target` is the *destination* of both edges, so the returned
    // neighbor must be resolved to each edge's source (the file, and
    // `caller`), never to `target` itself.
    let neighbors = db
        .code_neighbors("symbol", target.id, 1, "in", None)
        .unwrap();
    assert_eq!(
        neighbors.len(),
        2,
        "expected exactly one file and one symbol neighbor, got {neighbors:?}"
    );

    let file_neighbor = neighbors
        .iter()
        .find(|n| n.edge.relation == "imports")
        .expect("no neighbor for the imports edge");
    match file_neighbor.node.as_ref() {
        Some(crate::models::CodeNode::File(f)) => {
            assert_eq!(f.id, file.id);
            assert_eq!(f.relative_path, "src/shared.rs");
        }
        other => panic!("expected the imports edge to resolve to a File node, got {other:?}"),
    }

    let symbol_neighbor = neighbors
        .iter()
        .find(|n| n.edge.relation == "calls")
        .expect("no neighbor for the calls edge");
    match symbol_neighbor.node.as_ref() {
        Some(crate::models::CodeNode::Symbol(s)) => {
            assert_eq!(s.name, "caller");
            assert_ne!(
                s.id, target.id,
                "must resolve to the caller, not to target itself"
            );
        }
        other => panic!("expected the calls edge to resolve to a Symbol node, got {other:?}"),
    }
}

#[test]
fn code_neighbors_one_hop_ignores_edges_whose_type_matches_neither_symbol_nor_file() {
    let db = Database::open_in_memory().unwrap();
    let file = db
        .upsert_code_file("bogus-type", "/repo", "src/bogus.rs", "rust", "h1")
        .unwrap();

    let symbols = vec![
        ExtractedSymbol {
            kind: "function".into(),
            name: "origin".into(),
            qualified_name: "origin".into(),
            start_line: 1,
            end_line: 2,
        },
        ExtractedSymbol {
            kind: "function".into(),
            name: "peer".into(),
            qualified_name: "peer".into(),
            start_line: 4,
            end_line: 5,
        },
    ];
    db.replace_symbols_and_edges_for_file(file.id, &symbols, &[])
        .unwrap();

    let nodes = db
        .search_code_nodes("origin", None, None, Some("bogus-type"), 10)
        .unwrap();
    let crate::models::CodeNode::Symbol(origin) = &nodes[0] else {
        panic!("expected a symbol node");
    };
    let peer_nodes = db
        .search_code_nodes("peer", None, None, Some("bogus-type"), 10)
        .unwrap();
    let crate::models::CodeNode::Symbol(peer) = &peer_nodes[0] else {
        panic!("expected a symbol node");
    };

    // Two edges out of `origin` with a `dst_type` that is neither "symbol"
    // nor "file" (the id/type combinations don't come from the public
    // extraction path — this exercises the fallback `_ => None` arm
    // directly). Each `dst_id` deliberately reuses a real symbol/file id so
    // a mutated guard that resolves them anyway is observable.
    db.conn
        .execute(
            "INSERT INTO code_edges (src_id, src_type, dst_id, dst_type, dst_name, relation, resolution)
             VALUES (?1, 'symbol', ?2, 'bogus', 'peer', 'calls', 'heuristic')",
            rusqlite::params![origin.id, peer.id],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO code_edges (src_id, src_type, dst_id, dst_type, dst_name, relation, resolution)
             VALUES (?1, 'symbol', ?2, 'bogus', 'src/bogus.rs', 'imports', 'static')",
            rusqlite::params![origin.id, file.id],
        )
        .unwrap();

    let neighbors = db
        .code_neighbors("symbol", origin.id, 1, "out", None)
        .unwrap();
    assert_eq!(
        neighbors.len(),
        2,
        "both edges must still be returned, got {neighbors:?}"
    );
    assert!(
        neighbors.iter().all(|n| n.node.is_none()),
        "an unrecognized dst_type must resolve to no node, got {neighbors:?}"
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
