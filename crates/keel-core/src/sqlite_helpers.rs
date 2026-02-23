use rusqlite::params;

use crate::sqlite::SqliteGraphStore;
use crate::store::GraphStore;
use crate::types::{GraphError, GraphNode};

impl SqliteGraphStore {
    /// Load circuit breaker state from the database.
    pub fn load_circuit_breaker(&self) -> Result<Vec<(String, String, u32, bool)>, GraphError> {
        let mut stmt = self.conn.prepare(
            "SELECT error_code, hash, consecutive_failures, downgraded FROM circuit_breaker",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, i32>(3)? != 0,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Save circuit breaker state, replacing all existing rows.
    pub fn save_circuit_breaker(
        &self,
        state: &[(String, String, u32, bool)],
    ) -> Result<(), GraphError> {
        self.conn.execute("DELETE FROM circuit_breaker", [])?;
        let mut stmt = self.conn.prepare(
            "INSERT INTO circuit_breaker (error_code, hash, consecutive_failures, downgraded) \
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (code, hash, consecutive, downgraded) in state {
            stmt.execute(params![code, hash, consecutive, *downgraded as i32])?;
        }
        Ok(())
    }

    /// Search for nodes whose name contains the given substring (case-insensitive).
    /// Single SQL query instead of iterating modules + per-file lookups (N+1).
    pub fn search_nodes(
        &self,
        query: &str,
        kind_filter: Option<&str>,
        limit: usize,
    ) -> Vec<GraphNode> {
        let pattern = format!("%{}%", query);
        let sql = match kind_filter {
            Some(_) => {
                "SELECT id, hash, kind, name, signature, file_path, line_start, line_end, \
                 docstring, is_public, type_hints_present, has_docstring, module_id \
                 FROM nodes WHERE LOWER(name) LIKE LOWER(?1) AND kind = ?2 \
                 ORDER BY name LIMIT ?3"
            }
            None => {
                "SELECT id, hash, kind, name, signature, file_path, line_start, line_end, \
                 docstring, is_public, type_hints_present, has_docstring, module_id \
                 FROM nodes WHERE LOWER(name) LIKE LOWER(?1) \
                 ORDER BY name LIMIT ?2"
            }
        };

        let result = match kind_filter {
            Some(kind) => {
                let mut stmt = match self.conn.prepare(sql) {
                    Ok(s) => s,
                    Err(_) => return vec![],
                };
                stmt.query_map(params![pattern, kind, limit as u32], Self::row_to_node)
                    .ok()
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default()
            }
            None => {
                let mut stmt = match self.conn.prepare(sql) {
                    Ok(s) => s,
                    Err(_) => return vec![],
                };
                stmt.query_map(params![pattern, limit as u32], Self::row_to_node)
                    .ok()
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default()
            }
        };
        result
    }

    /// Insert a node into the database, or update it on hash conflict (upsert).
    pub fn insert_node(&self, node: &GraphNode) -> Result<(), GraphError> {
        self.conn.execute(
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

        // Clear old endpoints before re-inserting (UPSERT preserves the row,
        // so CASCADE no longer cleans these up)
        self.conn.execute(
            "DELETE FROM external_endpoints WHERE node_id = ?1",
            params![node.id],
        )?;
        for ep in &node.external_endpoints {
            self.conn.execute(
                "INSERT INTO external_endpoints (node_id, kind, method, path, direction) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![node.id, ep.kind, ep.method, ep.path, ep.direction],
            )?;
        }

        // Insert previous hashes (PK constraint handles dedup)
        for ph in &node.previous_hashes {
            self.conn.execute(
                "INSERT OR IGNORE INTO previous_hashes (node_id, hash) VALUES (?1, ?2)",
                params![node.id, ph],
            )?;
        }

        Ok(())
    }

    /// Update an existing node by ID, preserving the old hash in previous_hashes.
    pub fn update_node_in_db(&self, node: &GraphNode) -> Result<(), GraphError> {
        // Store old hash as previous hash
        if let Some(old) = self.get_node_by_id(node.id) {
            if old.hash != node.hash {
                self.conn.execute(
                    "INSERT OR IGNORE INTO previous_hashes (node_id, hash) VALUES (?1, ?2)",
                    params![node.id, old.hash],
                )?;
                // Keep only last 3
                self.conn.execute(
                    "DELETE FROM previous_hashes WHERE node_id = ?1 AND hash NOT IN (SELECT hash FROM previous_hashes WHERE node_id = ?1 ORDER BY created_at DESC LIMIT 3)",
                    params![node.id],
                )?;
            }
        }

        self.conn.execute(
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

        // Re-insert endpoints
        self.conn.execute(
            "DELETE FROM external_endpoints WHERE node_id = ?1",
            params![node.id],
        )?;
        for ep in &node.external_endpoints {
            self.conn.execute(
                "INSERT INTO external_endpoints (node_id, kind, method, path, direction) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![node.id, ep.kind, ep.method, ep.path, ep.direction],
            )?;
        }

        Ok(())
    }
}
