//! Graph helpers shared by the `BrainStore` edge/neighbor methods.
//!
//! The `impl BrainStore for Database` block lives in `entities.rs` — Rust
//! requires a single trait impl per type per crate, and `entities.rs` already
//! owns it. This module contributes an inherent `impl Database` block (allowed
//! to be split across files) with the edge row mapping used by those methods.

use crate::models::Edge;

use super::Database;

impl Database {
    pub(crate) fn row_to_edge(row: &rusqlite::Row) -> Edge {
        Edge {
            id: row.get(0).unwrap_or_default(),
            src_entity_id: row.get(1).unwrap_or_default(),
            dst_entity_id: row.get(2).unwrap_or_default(),
            edge_type: row.get(3).unwrap_or_default(),
            evidence_count: row.get(4).unwrap_or(1),
            confidence: row.get(5).unwrap_or(1.0),
            first_seen: row.get(6).unwrap_or_default(),
            last_seen: row.get(7).unwrap_or_default(),
            deleted_at: row.get(8).unwrap_or(None),
        }
    }

    pub(crate) const EDGE_COLS: &'static str = "id, src_entity_id, dst_entity_id, edge_type, \
         evidence_count, confidence, first_seen, last_seen, deleted_at";
}
