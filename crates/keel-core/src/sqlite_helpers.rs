use std::collections::HashMap;

use rusqlite::{params, Result as SqlResult};

use crate::sqlite::SqliteGraphStore;
use crate::store::GraphStore;
use crate::types::{BodyIndexEntry, ExternalEndpoint, GraphError, GraphNode, NodeKind};

impl SqliteGraphStore {
    /// Rebuild the body-hash index from `entries` in a single transaction.
    ///
    /// Backs [`GraphStore::replace_body_index`]. The delete and the inserts
    /// share one transaction so a concurrent reader never sees a partially
    /// rebuilt index.
    pub(crate) fn body_index_replace(
        &self,
        entries: Vec<BodyIndexEntry>,
    ) -> Result<(), GraphError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM body_index", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO body_index
                 (node_hash, body_hash, name, file_path, line)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for e in &entries {
                stmt.execute(params![
                    e.node_hash,
                    e.body_hash,
                    e.name,
                    e.file_path,
                    e.line
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Look up every indexed node sharing `body_hash`.
    ///
    /// Backs [`GraphStore::find_body_matches`]. Uses `idx_body_index_body_hash`.
    pub(crate) fn body_index_find(&self, body_hash: &str) -> Vec<BodyIndexEntry> {
        let mut stmt = match self.conn.prepare(
            "SELECT node_hash, body_hash, name, file_path, line
             FROM body_index WHERE body_hash = ?1
             ORDER BY file_path, line",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[keel] body_index_find: prepare failed: {e}");
                return Vec::new();
            }
        };

        // Bind before returning: the `MappedRows` temporary borrows `stmt`, so
        // using the match as the tail expression outlives `stmt` (E0597).
        let result = match stmt.query_map(params![body_hash], |row| {
            Ok(BodyIndexEntry {
                node_hash: row.get(0)?,
                body_hash: row.get(1)?,
                name: row.get(2)?,
                file_path: row.get(3)?,
                line: row.get(4)?,
            })
        }) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                eprintln!("[keel] body_index_find: query failed: {e}");
                Vec::new()
            }
        };
        result
    }

    /// Build the monorepo package index straight from the graph:
    /// `package -> (symbol name -> node id)`. Empty for repos that set no
    /// `package` column. Lets `keel compile`'s incremental sync reproduce the
    /// map's cross-package call edges instead of dropping them on prune.
    pub fn package_node_index(&self) -> HashMap<String, HashMap<String, u64>> {
        let mut stmt = match self.conn.prepare(
            "SELECT package, name, id FROM nodes WHERE package IS NOT NULL AND kind != 'module'",
        ) {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        };
        let mut index: HashMap<String, HashMap<String, u64>> = HashMap::new();
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? as u64,
            ))
        }) {
            for (pkg, name, id) in rows.flatten() {
                index.entry(pkg).or_default().insert(name, id);
            }
        }
        index
    }

    /// BAML boundary function nodes as `name -> node id`, read from the graph
    /// the last `keel map` populated. Used by the compile sync to re-resolve
    /// calls into `.baml` functions; callers gate this on `baml_src` existing
    /// so non-BAML repos never run the query.
    pub fn baml_function_nodes(&self) -> HashMap<String, u64> {
        let mut stmt = match self.conn.prepare(
            "SELECT name, id FROM nodes WHERE kind = 'function' AND file_path LIKE '%.baml'",
        ) {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        };
        let mut index: HashMap<String, u64> = HashMap::new();
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        }) {
            for (name, id) in rows.flatten() {
                index.entry(name).or_insert(id);
            }
        }
        index
    }

    /// Load the persisted batch-mode state blob (opaque JSON owned by
    /// `keel-enforce`), or `None` when no batch is active. Stored as a single
    /// `keel_meta` row so batch mode persists the same way circuit-breaker
    /// state does — one atomic SQLite write, no stray `.keel/batch.json`.
    pub fn load_batch(&self) -> Option<String> {
        self.conn
            .query_row(
                "SELECT value FROM keel_meta WHERE key = 'batch'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
    }

    /// Persist the batch-mode state blob, replacing any previous one atomically.
    pub fn save_batch(&self, json: &str) -> Result<(), GraphError> {
        self.conn.execute(
            "INSERT INTO keel_meta (key, value) VALUES ('batch', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    /// Remove any persisted batch-mode state (batch ended or expired).
    pub fn clear_batch(&self) -> Result<(), GraphError> {
        self.conn
            .execute("DELETE FROM keel_meta WHERE key = 'batch'", [])?;
        Ok(())
    }

    /// Load circuit breaker state from the database.
    /// Each tuple is (error_code, hash, consecutive_failures, downgraded,
    /// provenance_file, last_body_hash).
    pub fn load_circuit_breaker(
        &self,
    ) -> Result<Vec<crate::sqlite::CircuitBreakerEntry>, GraphError> {
        let mut stmt = self.conn.prepare(
            "SELECT error_code, hash, consecutive_failures, downgraded, provenance_file, \
             last_body_hash FROM circuit_breaker",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, i32>(3)? != 0,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Save circuit breaker state, replacing all existing rows.
    pub fn save_circuit_breaker(
        &self,
        state: &[crate::sqlite::CircuitBreakerEntry],
    ) -> Result<(), GraphError> {
        self.conn.execute("DELETE FROM circuit_breaker", [])?;
        let mut stmt = self.conn.prepare(
            "INSERT INTO circuit_breaker \
             (error_code, hash, consecutive_failures, downgraded, provenance_file, last_body_hash) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for (code, hash, consecutive, downgraded, file, last_hash) in state {
            stmt.execute(params![
                code,
                hash,
                consecutive,
                *downgraded as i32,
                file,
                last_hash
            ])?;
        }
        Ok(())
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

    /// Append rename history for a node, inside an open transaction.
    ///
    /// **Append-only by design.** Parsers routinely build `GraphNode` values
    /// with an empty `previous_hashes`, so replacing the set here would erase
    /// accumulated history on every re-map of an unchanged file. Entries are
    /// only ever added; a full `keel map` re-baselines them via `clear_all`.
    ///
    /// The `(node_id, hash)` primary key makes repeated writes idempotent.
    pub(crate) fn append_previous_hashes(
        tx: &rusqlite::Transaction<'_>,
        node_id: u64,
        hashes: &[String],
    ) -> Result<(), GraphError> {
        for hash in hashes {
            tx.execute(
                "INSERT OR IGNORE INTO previous_hashes (node_id, hash) VALUES (?1, ?2)",
                params![node_id, hash],
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

    /// Convert a SQLite row into a `GraphNode` (without relations loaded).
    pub(crate) fn row_to_node(row: &rusqlite::Row) -> SqlResult<GraphNode> {
        let kind_str: String = row.get("kind")?;
        let kind = match kind_str.as_str() {
            "module" => NodeKind::Module,
            "class" => NodeKind::Class,
            "function" => NodeKind::Function,
            _ => NodeKind::Function, // fallback
        };
        Ok(GraphNode {
            id: row.get("id")?,
            hash: row.get("hash")?,
            kind,
            name: row.get("name")?,
            signature: row.get("signature")?,
            file_path: row.get("file_path")?,
            line_start: row.get("line_start")?,
            line_end: row.get("line_end")?,
            docstring: row.get("docstring")?,
            is_public: row.get::<_, i32>("is_public")? != 0,
            type_hints_present: row.get::<_, i32>("type_hints_present")? != 0,
            has_docstring: row.get::<_, i32>("has_docstring")? != 0,
            external_endpoints: Vec::new(), // loaded separately
            previous_hashes: Vec::new(),    // loaded separately
            module_id: row.get::<_, Option<u64>>("module_id")?.unwrap_or(0),
            package: row.get::<_, Option<String>>("package").unwrap_or(None),
        })
    }

    /// Load all external endpoints associated with a given node.
    pub(crate) fn load_endpoints(&self, node_id: u64) -> Vec<ExternalEndpoint> {
        let mut stmt = match self.conn.prepare(
            "SELECT kind, method, path, direction FROM external_endpoints WHERE node_id = ?1",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[keel] load_endpoints: prepare failed: {e}");
                return Vec::new();
            }
        };

        let result = match stmt.query_map(params![node_id], |row| {
            Ok(ExternalEndpoint {
                kind: row.get(0)?,
                method: row.get(1)?,
                path: row.get(2)?,
                direction: row.get(3)?,
            })
        }) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                eprintln!("[keel] load_endpoints: query failed: {e}");
                Vec::new()
            }
        };
        result
    }

    /// Load the most recent previous hashes for a node (up to 3, newest first).
    pub(crate) fn load_previous_hashes(&self, node_id: u64) -> Vec<String> {
        let mut stmt = match self.conn.prepare(
            "SELECT hash FROM previous_hashes WHERE node_id = ?1 ORDER BY created_at DESC LIMIT 3",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[keel] load_previous_hashes: prepare failed: {e}");
                return Vec::new();
            }
        };

        let result = match stmt.query_map(params![node_id], |row| row.get(0)) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                eprintln!("[keel] load_previous_hashes: query failed: {e}");
                Vec::new()
            }
        };
        result
    }

    /// Attach endpoints and previous hashes to a node, returning the enriched node.
    pub(crate) fn node_with_relations(&self, mut node: GraphNode) -> GraphNode {
        node.external_endpoints = self.load_endpoints(node.id);
        node.previous_hashes = self.load_previous_hashes(node.id);
        node
    }
}
