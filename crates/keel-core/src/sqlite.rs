use rusqlite::{params, Connection, Result as SqlResult};

use crate::store::GraphStore;
use crate::types::{ExternalEndpoint, GraphError, GraphNode, NodeKind};

const SCHEMA_VERSION: u32 = 1;

/// SQLite-backed implementation of the GraphStore trait.
pub struct SqliteGraphStore {
    pub(crate) conn: Connection,
}

impl SqliteGraphStore {
    /// Open or create a graph database at the given path.
    pub fn open(path: &str) -> Result<Self, GraphError> {
        let conn = Connection::open(path)?;
        let store = SqliteGraphStore { conn };
        store.initialize_schema()?;
        Ok(store)
    }

    /// Create an in-memory graph database (for testing).
    pub fn in_memory() -> Result<Self, GraphError> {
        let conn = Connection::open_in_memory()?;
        let store = SqliteGraphStore { conn };
        store.initialize_schema()?;
        Ok(store)
    }

    /// Temporarily disable foreign key enforcement (for bulk re-map operations).
    pub fn set_foreign_keys(&self, enabled: bool) -> Result<(), GraphError> {
        let val = if enabled { "ON" } else { "OFF" };
        self.conn
            .execute_batch(&format!("PRAGMA foreign_keys = {};", val))?;
        Ok(())
    }

    fn initialize_schema(&self) -> Result<(), GraphError> {
        self.conn.execute_batch(
            "
            -- Schema version tracking
            CREATE TABLE IF NOT EXISTS keel_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            -- Nodes
            CREATE TABLE IF NOT EXISTS nodes (
                id INTEGER PRIMARY KEY,
                hash TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL CHECK (kind IN ('module', 'class', 'function')),
                name TEXT NOT NULL,
                signature TEXT NOT NULL DEFAULT '',
                file_path TEXT NOT NULL,
                line_start INTEGER NOT NULL,
                line_end INTEGER NOT NULL,
                docstring TEXT,
                is_public INTEGER NOT NULL DEFAULT 0,
                type_hints_present INTEGER NOT NULL DEFAULT 0,
                has_docstring INTEGER NOT NULL DEFAULT 0,
                module_id INTEGER REFERENCES nodes(id),
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_nodes_hash ON nodes(hash);
            CREATE INDEX IF NOT EXISTS idx_nodes_file ON nodes(file_path);
            CREATE INDEX IF NOT EXISTS idx_nodes_module ON nodes(module_id);
            CREATE INDEX IF NOT EXISTS idx_nodes_kind ON nodes(kind);

            -- Previous hashes for rename tracking
            CREATE TABLE IF NOT EXISTS previous_hashes (
                node_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                hash TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (node_id, hash)
            );

            -- External endpoints
            CREATE TABLE IF NOT EXISTS external_endpoints (
                id INTEGER PRIMARY KEY,
                node_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                method TEXT NOT NULL DEFAULT '',
                path TEXT NOT NULL,
                direction TEXT NOT NULL CHECK (direction IN ('serves', 'calls'))
            );
            CREATE INDEX IF NOT EXISTS idx_endpoints_node ON external_endpoints(node_id);

            -- Edges
            CREATE TABLE IF NOT EXISTS edges (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                target_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                kind TEXT NOT NULL CHECK (kind IN ('calls', 'imports', 'inherits', 'contains')),
                file_path TEXT NOT NULL,
                line INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_id);
            CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_id);
            CREATE INDEX IF NOT EXISTS idx_edges_kind ON edges(kind);

            -- Module profiles
            CREATE TABLE IF NOT EXISTS module_profiles (
                module_id INTEGER PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
                path TEXT NOT NULL,
                function_count INTEGER NOT NULL DEFAULT 0,
                function_name_prefixes TEXT NOT NULL DEFAULT '[]',
                primary_types TEXT NOT NULL DEFAULT '[]',
                import_sources TEXT NOT NULL DEFAULT '[]',
                export_targets TEXT NOT NULL DEFAULT '[]',
                external_endpoint_count INTEGER NOT NULL DEFAULT 0,
                responsibility_keywords TEXT NOT NULL DEFAULT '[]'
            );

            -- Resolution cache
            CREATE TABLE IF NOT EXISTS resolution_cache (
                call_site_hash TEXT PRIMARY KEY,
                resolved_node_id INTEGER REFERENCES nodes(id),
                confidence REAL NOT NULL,
                resolution_tier TEXT NOT NULL,
                cached_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Circuit breaker state
            CREATE TABLE IF NOT EXISTS circuit_breaker (
                error_code TEXT NOT NULL,
                hash TEXT NOT NULL,
                consecutive_failures INTEGER NOT NULL DEFAULT 0,
                last_failure_at TEXT NOT NULL DEFAULT (datetime('now')),
                downgraded INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (error_code, hash)
            );
            ",
        )?;

        // Set schema version if not present
        self.conn.execute(
            "INSERT OR IGNORE INTO keel_meta (key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;

        Ok(())
    }

    /// Get the current schema version.
    pub fn schema_version(&self) -> Result<u32, GraphError> {
        let version: String = self.conn.query_row(
            "SELECT value FROM keel_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )?;
        version
            .parse()
            .map_err(|e| GraphError::Internal(format!("Invalid schema version: {}", e)))
    }

    /// Clear all graph data (nodes, edges, etc.) for a full re-map.
    /// Preserves schema and metadata.
    pub fn clear_all(&mut self) -> Result<(), GraphError> {
        self.conn.execute_batch(
            "
            DELETE FROM edges;
            DELETE FROM resolution_cache;
            DELETE FROM circuit_breaker;
            DELETE FROM module_profiles;
            DELETE FROM external_endpoints;
            DELETE FROM previous_hashes;
            DELETE FROM nodes;
            ",
        )?;
        Ok(())
    }

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
        })
    }

    pub(crate) fn load_endpoints(&self, node_id: u64) -> Vec<ExternalEndpoint> {
        let mut stmt = match self
            .conn
            .prepare("SELECT kind, method, path, direction FROM external_endpoints WHERE node_id = ?1")
        {
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

    pub(crate) fn load_previous_hashes(&self, node_id: u64) -> Vec<String> {
        let mut stmt = match self
            .conn
            .prepare(
                "SELECT hash FROM previous_hashes WHERE node_id = ?1 ORDER BY created_at DESC LIMIT 3",
            )
        {
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

    pub(crate) fn node_with_relations(&self, mut node: GraphNode) -> GraphNode {
        node.external_endpoints = self.load_endpoints(node.id);
        node.previous_hashes = self.load_previous_hashes(node.id);
        node
    }

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

    pub fn insert_node(&self, node: &GraphNode) -> Result<(), GraphError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO nodes (id, hash, kind, name, signature, file_path, line_start, line_end, docstring, is_public, type_hints_present, has_docstring, module_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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

        // Insert external endpoints
        for ep in &node.external_endpoints {
            self.conn.execute(
                "INSERT INTO external_endpoints (node_id, kind, method, path, direction) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![node.id, ep.kind, ep.method, ep.path, ep.direction],
            )?;
        }

        // Insert previous hashes
        for ph in &node.previous_hashes {
            self.conn.execute(
                "INSERT OR IGNORE INTO previous_hashes (node_id, hash) VALUES (?1, ?2)",
                params![node.id, ph],
            )?;
        }

        Ok(())
    }

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
        self.conn
            .execute("DELETE FROM external_endpoints WHERE node_id = ?1", params![node.id])?;
        for ep in &node.external_endpoints {
            self.conn.execute(
                "INSERT INTO external_endpoints (node_id, kind, method, path, direction) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![node.id, ep.kind, ep.method, ep.path, ep.direction],
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "sqlite_tests.rs"]
mod tests;
