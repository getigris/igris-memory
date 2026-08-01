use rusqlite::params;

use crate::codegraph::{ExtractedEdge, ExtractedSymbol};
use crate::errors::IgrisError;
use crate::models::{CodeEdge, CodeFile, CodeMap, CodeNeighbor, CodeNode, CodeSymbol};
use crate::utils::now_utc;

use super::{Database, DbResult, OptionalExt};

// `search_code_nodes`, `code_neighbors`, `code_path`, and `code_map` are
// wired into the `igris_code_search`/`igris_code_neighbors`/`igris_code_path`/
// `igris_code_map` MCP tools (Tasks 10-13).
impl Database {
    const CODE_FILE_COLS: &'static str =
        "id, project, root_path, relative_path, language, content_hash, indexed_at, deleted_at";
    const CODE_SYMBOL_COLS: &'static str =
        "id, file_id, kind, name, qualified_name, start_line, end_line, indexed_at, deleted_at";
    // Same columns as CODE_SYMBOL_COLS, `s.`-prefixed for the joined query in
    // `search_code_nodes` (kept as a literal rather than derived from
    // CODE_SYMBOL_COLS at runtime, matching the rest of this codebase's
    // fully-literal SQL strings).
    const CODE_SYMBOL_COLS_ALIASED: &'static str = "s.id, s.file_id, s.kind, s.name, \
         s.qualified_name, s.start_line, s.end_line, s.indexed_at, s.deleted_at";
    const CODE_EDGE_COLS: &'static str = "id, src_id, src_type, dst_id, dst_type, dst_name, relation, \
         resolution, external_boundary, evidence_count, first_seen, last_seen";

    fn row_to_code_file(row: &rusqlite::Row) -> CodeFile {
        CodeFile {
            id: row.get(0).unwrap_or_default(),
            project: row.get(1).unwrap_or_default(),
            root_path: row.get(2).unwrap_or_default(),
            relative_path: row.get(3).unwrap_or_default(),
            language: row.get(4).unwrap_or_default(),
            content_hash: row.get(5).unwrap_or_default(),
            indexed_at: row.get(6).unwrap_or_default(),
            deleted_at: row.get(7).unwrap_or(None),
        }
    }

    fn row_to_code_symbol(row: &rusqlite::Row) -> CodeSymbol {
        CodeSymbol {
            id: row.get(0).unwrap_or_default(),
            file_id: row.get(1).unwrap_or_default(),
            kind: row.get(2).unwrap_or_default(),
            name: row.get(3).unwrap_or_default(),
            qualified_name: row.get(4).unwrap_or_default(),
            start_line: row.get(5).unwrap_or_default(),
            end_line: row.get(6).unwrap_or_default(),
            indexed_at: row.get(7).unwrap_or_default(),
            deleted_at: row.get(8).unwrap_or(None),
        }
    }

    fn row_to_code_edge(row: &rusqlite::Row) -> CodeEdge {
        CodeEdge {
            id: row.get(0).unwrap_or_default(),
            src_id: row.get(1).unwrap_or_default(),
            src_type: row.get(2).unwrap_or_default(),
            dst_id: row.get(3).unwrap_or(None),
            dst_type: row.get(4).unwrap_or(None),
            dst_name: row.get(5).unwrap_or_default(),
            relation: row.get(6).unwrap_or_default(),
            resolution: row.get(7).unwrap_or_default(),
            external_boundary: row.get::<_, i64>(8).unwrap_or(0) != 0,
            evidence_count: row.get(9).unwrap_or(1),
            first_seen: row.get(10).unwrap_or_default(),
            last_seen: row.get(11).unwrap_or_default(),
        }
    }

    /// Create a `code_files` row, or update its `content_hash`/`indexed_at` in
    /// place if one already exists for (project, root_path, relative_path).
    pub fn upsert_code_file(
        &self,
        project: &str,
        root_path: &str,
        relative_path: &str,
        language: &str,
        content_hash: &str,
    ) -> DbResult<CodeFile> {
        let now = now_utc();
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM code_files WHERE project = ?1 AND root_path = ?2 AND relative_path = ?3",
                params![project, root_path, relative_path],
                |r| r.get(0),
            )
            .optional()?;

        let id = if let Some(id) = existing {
            self.conn.execute(
                "UPDATE code_files SET content_hash = ?1, indexed_at = ?2, language = ?3, deleted_at = NULL WHERE id = ?4",
                params![content_hash, now, language, id],
            )?;
            id
        } else {
            self.conn.execute(
                "INSERT INTO code_files (project, root_path, relative_path, language, content_hash, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![project, root_path, relative_path, language, content_hash, now],
            )?;
            self.conn.last_insert_rowid()
        };

        self.conn
            .query_row(
                &format!(
                    "SELECT {} FROM code_files WHERE id = ?1",
                    Self::CODE_FILE_COLS
                ),
                params![id],
                |row| Ok(Self::row_to_code_file(row)),
            )
            .map_err(IgrisError::from)
    }

    /// Returns the stored content hash for a file, or `None` if it's never
    /// been indexed or has been soft-deleted. Used by the indexer (Task 8) to
    /// decide whether a discovered file needs reparsing.
    pub fn get_code_file_hash(
        &self,
        project: &str,
        root_path: &str,
        relative_path: &str,
    ) -> DbResult<Option<String>> {
        let hash = self
            .conn
            .query_row(
                "SELECT content_hash FROM code_files
                 WHERE project = ?1 AND root_path = ?2 AND relative_path = ?3 AND deleted_at IS NULL",
                params![project, root_path, relative_path],
                |r| r.get(0),
            )
            .optional()?;
        Ok(hash)
    }

    /// Finalizes `content_hash`/`indexed_at` for `file_id`. Used by the
    /// indexer (Task 8) as the last step of a successful reindex: `upsert_code_file`
    /// is called first with a hash that can never match real file content
    /// (so an interrupted run is retried, not mistaken for unchanged), and
    /// this method commits the real hash only once `replace_symbols_and_edges_for_file`
    /// has also succeeded — keeping "stored hash matches disk" synonymous
    /// with "symbols/edges are up to date too".
    pub fn update_code_file_hash(&self, file_id: i64, content_hash: &str) -> DbResult<()> {
        let now = now_utc();
        self.conn.execute(
            "UPDATE code_files SET content_hash = ?1, indexed_at = ?2 WHERE id = ?3",
            params![content_hash, now, file_id],
        )?;
        Ok(())
    }

    /// Replaces every symbol/edge belonging to `file_id` with the freshly
    /// extracted set. Symbols/edges are a derived cache scoped to one file's
    /// current content, so a hard delete-then-reinsert (inside a transaction)
    /// is correct here — unlike entities, there's no cross-file history to
    /// preserve for a single file's own definitions.
    ///
    /// `dst_name` on each edge is resolved against symbols already indexed in
    /// the same `project` (by `qualified_name`): a match sets
    /// `dst_id`/`dst_type`; no match leaves both `None` — the edge is kept
    /// either way (as an `external_boundary` fact, or as an unresolved
    /// same-project reference that a later file's indexing pass may resolve
    /// once that symbol exists).
    pub fn replace_symbols_and_edges_for_file(
        &self,
        file_id: i64,
        symbols: &[ExtractedSymbol],
        edges: &[ExtractedEdge],
    ) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        let project: String = tx
            .query_row(
                "SELECT project FROM code_files WHERE id = ?1",
                params![file_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| IgrisError::not_found(format!("code file {file_id} not found")))?;

        tx.execute(
            "DELETE FROM code_edges WHERE src_type = 'symbol' AND src_id IN (SELECT id FROM code_symbols WHERE file_id = ?1)",
            params![file_id],
        )?;
        tx.execute(
            "DELETE FROM code_edges WHERE src_type = 'file' AND src_id = ?1",
            params![file_id],
        )?;
        tx.execute(
            "DELETE FROM code_symbols WHERE file_id = ?1",
            params![file_id],
        )?;

        let now = now_utc();
        let mut qualified_to_id = std::collections::HashMap::new();
        for symbol in symbols {
            tx.execute(
                "INSERT INTO code_symbols (file_id, kind, name, qualified_name, start_line, end_line, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    file_id,
                    symbol.kind,
                    symbol.name,
                    symbol.qualified_name,
                    symbol.start_line,
                    symbol.end_line,
                    now
                ],
            )?;
            qualified_to_id.insert(symbol.qualified_name.clone(), tx.last_insert_rowid());
        }

        for edge in edges {
            let src_id = match &edge.src_qualified_name {
                Some(qn) => *qualified_to_id.get(qn).unwrap_or(&file_id),
                None => file_id,
            };
            let src_type = if edge.src_qualified_name.is_some() {
                "symbol"
            } else {
                "file"
            };

            // Resolve dst_name against this project's known symbols (not
            // cross-project) — a symbol just inserted above in this same
            // transaction (via qualified_to_id) also satisfies this query,
            // since it's already committed to the `code_symbols` table
            // within the transaction.
            let resolved: Option<i64> = tx
                .query_row(
                    "SELECT s.id FROM code_symbols s
                     JOIN code_files f ON f.id = s.file_id
                     WHERE f.project = ?1 AND s.qualified_name = ?2 AND s.deleted_at IS NULL",
                    params![project, edge.dst_name],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?;

            let (dst_id, dst_type) = match resolved {
                Some(id) => (Some(id), Some("symbol")),
                None => (None, None),
            };

            tx.execute(
                "INSERT INTO code_edges (src_id, src_type, dst_id, dst_type, dst_name, relation, resolution, external_boundary, evidence_count, first_seen, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?9)",
                params![
                    src_id,
                    src_type,
                    dst_id,
                    dst_type,
                    edge.dst_name,
                    edge.relation,
                    edge.resolution,
                    edge.external_boundary as i64,
                    now
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Soft-deletes every `code_files` row under (project, root_path) whose
    /// `relative_path` is not in `seen_relative_paths` — i.e. files the
    /// walker no longer finds on disk. Returns how many were marked deleted.
    pub fn soft_delete_missing_code_files(
        &self,
        project: &str,
        root_path: &str,
        seen_relative_paths: &[String],
    ) -> DbResult<i64> {
        let mut stmt = self.conn.prepare(
            "SELECT id, relative_path FROM code_files WHERE project = ?1 AND root_path = ?2 AND deleted_at IS NULL",
        )?;
        let rows: Vec<(i64, String)> = stmt
            .query_map(params![project, root_path], |r| Ok((r.get(0)?, r.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let seen: std::collections::HashSet<&String> = seen_relative_paths.iter().collect();
        let now = now_utc();
        let mut deleted = 0i64;
        for (id, relative_path) in rows {
            if !seen.contains(&relative_path) {
                self.conn.execute(
                    "UPDATE code_files SET deleted_at = ?1 WHERE id = ?2",
                    params![now, id],
                )?;
                deleted += 1;
            }
        }
        Ok(deleted)
    }

    /// Finds symbol nodes by name substring (case-insensitive), optionally
    /// filtered by symbol kind, file language, and project.
    pub fn search_code_nodes(
        &self,
        query: &str,
        kind: Option<&str>,
        language: Option<&str>,
        project: Option<&str>,
        limit: i64,
    ) -> DbResult<Vec<CodeNode>> {
        let like = format!("%{}%", query.to_lowercase());
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM code_symbols s
             JOIN code_files f ON f.id = s.file_id
             WHERE lower(s.name) LIKE ?1 AND s.deleted_at IS NULL AND f.deleted_at IS NULL
               AND (?2 IS NULL OR s.kind = ?2)
               AND (?3 IS NULL OR f.language = ?3)
               AND (?4 IS NULL OR f.project = ?4)
             ORDER BY s.indexed_at DESC
             LIMIT ?5",
            Self::CODE_SYMBOL_COLS_ALIASED
        ))?;
        let symbols: Vec<CodeNode> = stmt
            .query_map(params![like, kind, language, project, limit], |row| {
                Ok(CodeNode::Symbol(Self::row_to_code_symbol(row)))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(symbols)
    }

    /// Directly connected nodes (1 hop) for `(node_type, node_id)`. `hops > 1`
    /// is not implemented in this task — Task 11 (`igris_code_neighbors`
    /// tool) calls this repeatedly to walk multiple hops.
    pub fn code_neighbors(
        &self,
        node_type: &str,
        node_id: i64,
        _hops: i64,
        direction: &str,
        relation: Option<&str>,
    ) -> DbResult<Vec<CodeNeighbor>> {
        let mut edges = Vec::new();
        if direction == "out" || direction == "both" {
            let mut stmt = self.conn.prepare(&format!(
                "SELECT {} FROM code_edges WHERE src_type = ?1 AND src_id = ?2 AND (?3 IS NULL OR relation = ?3)",
                Self::CODE_EDGE_COLS
            ))?;
            edges.extend(
                stmt.query_map(params![node_type, node_id, relation], |row| {
                    Ok(Self::row_to_code_edge(row))
                })?
                .filter_map(|r| r.ok()),
            );
        }
        if direction == "in" || direction == "both" {
            let mut stmt = self.conn.prepare(&format!(
                "SELECT {} FROM code_edges WHERE dst_type = ?1 AND dst_id = ?2 AND (?3 IS NULL OR relation = ?3)",
                Self::CODE_EDGE_COLS
            ))?;
            edges.extend(
                stmt.query_map(params![node_type, node_id, relation], |row| {
                    Ok(Self::row_to_code_edge(row))
                })?
                .filter_map(|r| r.ok()),
            );
        }

        let mut neighbors = Vec::new();
        for edge in edges {
            let other = if edge.src_type == node_type && edge.src_id == node_id {
                edge.dst_id.zip(edge.dst_type.clone())
            } else {
                Some((edge.src_id, edge.src_type.clone()))
            };
            let node = match other {
                Some((id, ref t)) if t == "symbol" => self
                    .conn
                    .query_row(
                        &format!(
                            "SELECT {} FROM code_symbols WHERE id = ?1",
                            Self::CODE_SYMBOL_COLS
                        ),
                        params![id],
                        |row| Ok(CodeNode::Symbol(Self::row_to_code_symbol(row))),
                    )
                    .ok(),
                Some((id, ref t)) if t == "file" => self
                    .conn
                    .query_row(
                        &format!(
                            "SELECT {} FROM code_files WHERE id = ?1",
                            Self::CODE_FILE_COLS
                        ),
                        params![id],
                        |row| Ok(CodeNode::File(Self::row_to_code_file(row))),
                    )
                    .ok(),
                _ => None,
            };
            neighbors.push(CodeNeighbor { edge, node });
        }
        Ok(neighbors)
    }

    /// Breadth-first shortest path between two nodes, capped at `max_hops`.
    /// Returns `None` if no path is found within the cap.
    pub fn code_path(
        &self,
        from_type: &str,
        from_id: i64,
        to_type: &str,
        to_id: i64,
        max_hops: i64,
    ) -> DbResult<Option<Vec<CodeNeighbor>>> {
        use std::collections::{HashSet, VecDeque};

        let start = (from_type.to_string(), from_id);
        let target = (to_type.to_string(), to_id);
        if start == target {
            return Ok(Some(vec![]));
        }

        let mut visited: HashSet<(String, i64)> = HashSet::from([start.clone()]);
        let mut queue: VecDeque<(Vec<CodeNeighbor>, (String, i64))> =
            VecDeque::from([(vec![], start)]);

        while let Some((path, current)) = queue.pop_front() {
            if path.len() as i64 >= max_hops {
                continue;
            }
            let neighbors = self.code_neighbors(&current.0, current.1, 1, "both", None)?;
            for neighbor in neighbors {
                let Some(ref node) = neighbor.node else {
                    continue;
                };
                let key = match node {
                    CodeNode::Symbol(s) => ("symbol".to_string(), s.id),
                    CodeNode::File(f) => ("file".to_string(), f.id),
                };
                if visited.contains(&key) {
                    continue;
                }
                let mut next_path = path.clone();
                next_path.push(neighbor);
                if key == target {
                    return Ok(Some(next_path));
                }
                visited.insert(key.clone());
                queue.push_back((next_path, key));
            }
        }
        Ok(None)
    }

    /// Summary of a file: its symbols and strongest connections (by
    /// `evidence_count`). `path` matches `code_files.relative_path` exactly
    /// in this task — directory-level summaries are a future enhancement.
    pub fn code_map(&self, project: &str, path: &str) -> DbResult<CodeMap> {
        let file_id: i64 = self
            .conn
            .query_row(
                "SELECT id FROM code_files WHERE project = ?1 AND relative_path = ?2 AND deleted_at IS NULL",
                params![project, path],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| IgrisError::not_found(format!("no indexed file at {path}")))?;

        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM code_symbols WHERE file_id = ?1 AND deleted_at IS NULL",
            Self::CODE_SYMBOL_COLS
        ))?;
        let symbols: Vec<CodeSymbol> = stmt
            .query_map(params![file_id], |row| Ok(Self::row_to_code_symbol(row)))?
            .filter_map(|r| r.ok())
            .collect();

        let mut top_connections = self.code_neighbors("file", file_id, 1, "both", None)?;
        top_connections.sort_by(|a, b| b.edge.evidence_count.cmp(&a.edge.evidence_count));

        Ok(CodeMap {
            path: path.to_string(),
            symbols,
            top_connections,
        })
    }
}
