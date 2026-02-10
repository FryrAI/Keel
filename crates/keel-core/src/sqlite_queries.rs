use rusqlite::params;

use crate::sqlite::SqliteGraphStore;
use crate::store::GraphStore;
use crate::types::{
    EdgeChange, EdgeDirection, EdgeKind, GraphEdge, GraphError, GraphNode, ModuleProfile,
    NodeChange,
};

impl GraphStore for SqliteGraphStore {
    fn get_node(&self, hash: &str) -> Option<GraphNode> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM nodes WHERE hash = ?1")
            .ok()?;
        let node = stmt
            .query_row(params![hash], Self::row_to_node)
            .ok()?;
        Some(self.node_with_relations(node))
    }

    fn get_node_by_id(&self, id: u64) -> Option<GraphNode> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM nodes WHERE id = ?1")
            .ok()?;
        let node = stmt
            .query_row(params![id], Self::row_to_node)
            .ok()?;
        Some(self.node_with_relations(node))
    }

    fn get_edges(&self, node_id: u64, direction: EdgeDirection) -> Vec<GraphEdge> {
        let query = match direction {
            EdgeDirection::Incoming => "SELECT * FROM edges WHERE target_id = ?1",
            EdgeDirection::Outgoing => "SELECT * FROM edges WHERE source_id = ?1",
            EdgeDirection::Both => {
                "SELECT * FROM edges WHERE source_id = ?1 OR target_id = ?1"
            }
        };

        let mut stmt = match self.conn.prepare(query) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[keel] get_edges: prepare failed: {e}");
                return Vec::new();
            }
        };
        let result = match stmt.query_map(params![node_id], |row| {
            let kind_str: String = row.get("kind")?;
            let kind = match kind_str.as_str() {
                "calls" => EdgeKind::Calls,
                "imports" => EdgeKind::Imports,
                "inherits" => EdgeKind::Inherits,
                "contains" => EdgeKind::Contains,
                _ => EdgeKind::Calls,
            };
            Ok(GraphEdge {
                id: row.get("id")?,
                source_id: row.get("source_id")?,
                target_id: row.get("target_id")?,
                kind,
                file_path: row.get("file_path")?,
                line: row.get("line")?,
            })
        }) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                eprintln!("[keel] get_edges: query failed: {e}");
                Vec::new()
            }
        };
        result
    }

    fn get_module_profile(&self, module_id: u64) -> Option<ModuleProfile> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM module_profiles WHERE module_id = ?1")
            .ok()?;
        stmt.query_row(params![module_id], |row| {
            let prefixes: String = row.get("function_name_prefixes")?;
            let types: String = row.get("primary_types")?;
            let imports: String = row.get("import_sources")?;
            let exports: String = row.get("export_targets")?;
            let keywords: String = row.get("responsibility_keywords")?;
            Ok(ModuleProfile {
                module_id: row.get("module_id")?,
                path: row.get("path")?,
                function_count: row.get("function_count")?,
                function_name_prefixes: serde_json::from_str(&prefixes).unwrap_or_default(),
                primary_types: serde_json::from_str(&types).unwrap_or_default(),
                import_sources: serde_json::from_str(&imports).unwrap_or_default(),
                export_targets: serde_json::from_str(&exports).unwrap_or_default(),
                external_endpoint_count: row.get("external_endpoint_count")?,
                responsibility_keywords: serde_json::from_str(&keywords).unwrap_or_default(),
            })
        })
        .ok()
    }

    fn get_nodes_in_file(&self, file_path: &str) -> Vec<GraphNode> {
        let mut stmt = match self
            .conn
            .prepare("SELECT * FROM nodes WHERE file_path = ?1")
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[keel] get_nodes_in_file: prepare failed: {e}");
                return Vec::new();
            }
        };
        let nodes: Vec<GraphNode> = match stmt.query_map(params![file_path], Self::row_to_node) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                eprintln!("[keel] get_nodes_in_file: query failed: {e}");
                return Vec::new();
            }
        };
        // Batch-load relations: 2 queries total instead of 2*N
        self.nodes_with_relations_batch(nodes)
    }

    fn get_all_modules(&self) -> Vec<GraphNode> {
        let mut stmt = match self
            .conn
            .prepare("SELECT * FROM nodes WHERE kind = 'module'")
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[keel] get_all_modules: prepare failed: {e}");
                return Vec::new();
            }
        };
        let nodes: Vec<GraphNode> = match stmt.query_map([], Self::row_to_node) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                eprintln!("[keel] get_all_modules: query failed: {e}");
                return Vec::new();
            }
        };
        // Batch-load relations: 2 queries total instead of 2*N
        self.nodes_with_relations_batch(nodes)
    }

    fn update_nodes(&mut self, changes: Vec<NodeChange>) -> Result<(), GraphError> {
        let tx = self.conn.transaction()?;
        for change in changes {
            match change {
                NodeChange::Add(node) => {
                    // Check for hash collision (different function, same hash)
                    let existing: Option<String> = tx
                        .query_row(
                            "SELECT name FROM nodes WHERE hash = ?1",
                            params![node.hash],
                            |row| row.get(0),
                        )
                        .ok();
                    if let Some(existing_name) = existing {
                        if existing_name != node.name {
                            return Err(GraphError::HashCollision {
                                hash: node.hash.clone(),
                                existing: existing_name,
                                new_fn: node.name.clone(),
                            });
                        }
                    }
                    // UPSERT to handle re-map without cascade-deleting related rows
                    tx.execute(
                        "INSERT INTO nodes (id, hash, kind, name, signature, file_path, line_start, line_end, docstring, is_public, type_hints_present, has_docstring, module_id)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                         ON CONFLICT(hash) DO UPDATE SET
                            kind = excluded.kind,
                            name = excluded.name,
                            signature = excluded.signature,
                            file_path = excluded.file_path,
                            line_start = excluded.line_start,
                            line_end = excluded.line_end,
                            docstring = excluded.docstring,
                            is_public = excluded.is_public,
                            type_hints_present = excluded.type_hints_present,
                            has_docstring = excluded.has_docstring,
                            module_id = excluded.module_id,
                            updated_at = datetime('now')",
                        params![
                            node.id,
                            node.hash,
                            node.kind.as_str(),
                            node.name,
                            node.signature,
                            node.file_path,
                            node.line_start,
                            node.line_end,
                            node.docstring,
                            node.is_public as i32,
                            node.type_hints_present as i32,
                            node.has_docstring as i32,
                            if node.module_id == 0 { None } else { Some(node.module_id) },
                        ],
                    )?;
                }
                NodeChange::Update(node) => {
                    // Check for hash collision (different node, same hash)
                    let existing: Option<(u64, String)> = tx
                        .query_row(
                            "SELECT id, name FROM nodes WHERE hash = ?1",
                            params![node.hash],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .ok();
                    if let Some((existing_id, existing_name)) = existing {
                        if existing_id != node.id && existing_name != node.name {
                            return Err(GraphError::HashCollision {
                                hash: node.hash.clone(),
                                existing: existing_name,
                                new_fn: node.name.clone(),
                            });
                        }
                    }
                    tx.execute(
                        "UPDATE nodes SET hash = ?1, kind = ?2, name = ?3, signature = ?4, file_path = ?5, line_start = ?6, line_end = ?7, docstring = ?8, is_public = ?9, type_hints_present = ?10, has_docstring = ?11, module_id = ?12, updated_at = datetime('now') WHERE id = ?13",
                        params![
                            node.hash,
                            node.kind.as_str(),
                            node.name,
                            node.signature,
                            node.file_path,
                            node.line_start,
                            node.line_end,
                            node.docstring,
                            node.is_public as i32,
                            node.type_hints_present as i32,
                            node.has_docstring as i32,
                            if node.module_id == 0 { None } else { Some(node.module_id) },
                            node.id,
                        ],
                    )?;
                }
                NodeChange::Remove(id) => {
                    tx.execute("DELETE FROM nodes WHERE id = ?1", params![id])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn update_edges(&mut self, changes: Vec<EdgeChange>) -> Result<(), GraphError> {
        let tx = self.conn.transaction()?;
        for change in changes {
            match change {
                EdgeChange::Add(edge) => {
                    tx.execute(
                        "INSERT INTO edges (id, source_id, target_id, kind, file_path, line) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                         ON CONFLICT(source_id, target_id, kind, file_path, line) DO NOTHING",
                        params![
                            edge.id,
                            edge.source_id,
                            edge.target_id,
                            edge.kind.as_str(),
                            edge.file_path,
                            edge.line,
                        ],
                    )?;
                }
                EdgeChange::Remove(id) => {
                    tx.execute("DELETE FROM edges WHERE id = ?1", params![id])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn get_previous_hashes(&self, node_id: u64) -> Vec<String> {
        self.load_previous_hashes(node_id)
    }
}
