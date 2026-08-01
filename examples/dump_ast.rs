//! Debug helper: parses a source file with a given language's tree-sitter
//! grammar and prints its S-expression parse tree. Used while writing each
//! language's `.scm` query (see src/codegraph/extractors/) to find the real
//! AST node-kind names instead of guessing them.
//!
//! Usage: cargo run --example dump_ast -- rust path/to/file.rs

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(language_name) = args.get(1) else {
        eprintln!("usage: dump_ast <language> <file>");
        std::process::exit(1);
    };
    let Some(path) = args.get(2) else {
        eprintln!("usage: dump_ast <language> <file>");
        std::process::exit(1);
    };

    let source = std::fs::read_to_string(path).expect("failed to read file");

    let mut parser = tree_sitter::Parser::new();
    let language = match language_name.as_str() {
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "javascript" => tree_sitter_javascript::LANGUAGE.into(),
        other => {
            eprintln!(
                "unsupported language for dump_ast: {other} (add it to this match as you add its grammar dependency)"
            );
            std::process::exit(1);
        }
    };
    parser
        .set_language(&language)
        .expect("grammar failed to load — check the grammar crate's loading convention on docs.rs, it may differ from `LANGUAGE.into()` depending on the resolved version");

    let tree = parser.parse(&source, None).expect("parse failed");
    println!("{}", tree.root_node().to_sexp());
}
