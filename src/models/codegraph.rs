use serde::{Deserialize, Serialize};

/// A single indexed source file — one row per (project, root_path, relative_path).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct CodeFile {
    pub id: i64,
    pub project: String,
    pub root_path: String,
    pub relative_path: String,
    pub language: String,
    pub content_hash: String,
    pub indexed_at: String,
    pub deleted_at: Option<String>,
}

/// A function/method/class/etc. extracted from a `CodeFile`.
///
/// `relative_path`/`language` are denormalized from the owning `CodeFile` on
/// every read so a symbol returned by a query is enough to open the right file
/// on disk (`relative_path` + `start_line`) without a second lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct CodeSymbol {
    pub id: i64,
    pub file_id: i64,
    pub kind: String,
    pub name: String,
    pub qualified_name: String,
    pub start_line: i64,
    pub end_line: i64,
    pub indexed_at: String,
    pub deleted_at: Option<String>,
    pub relative_path: String,
    pub language: String,
}

/// A typed relation between two code nodes (file or symbol). `dst_id`/`dst_type`
/// are `None` when the target couldn't be resolved within the index — the raw
/// `dst_name` is kept so the fact isn't silently dropped.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct CodeEdge {
    pub id: i64,
    pub src_id: i64,
    pub src_type: String,
    pub dst_id: Option<i64>,
    pub dst_type: Option<String>,
    pub dst_name: String,
    pub relation: String,
    pub resolution: String,
    pub external_boundary: bool,
    pub evidence_count: i64,
    pub first_seen: String,
    pub last_seen: String,
}

/// Either a file or a symbol — the two node kinds in the code graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "node_type", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum CodeNode {
    File(CodeFile),
    Symbol(CodeSymbol),
}

/// A neighbor in the code graph: the connecting edge plus the node on the other end.
/// `node` is `None` when the edge's target is unresolved (see `CodeEdge::dst_id`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct CodeNeighbor {
    pub edge: CodeEdge,
    pub node: Option<CodeNode>,
}

/// Outcome of one `index_project` run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct IndexSummary {
    pub files_indexed: i64,
    pub files_unchanged: i64,
    pub files_skipped_unsupported: i64,
    pub files_failed_parse: i64,
    pub files_deleted: i64,
}

/// A one-call summary of a file or directory: its symbols and strongest connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeMap {
    pub path: String,
    pub symbols: Vec<CodeSymbol>,
    pub top_connections: Vec<CodeNeighbor>,
}
