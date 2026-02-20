use rusqlite::{params, Connection, Result as SqlResult};

use crate::types::{ExternalEndpoint, GraphError, GraphNode, NodeKind};

const SCHEMA_VERSION: u32 = 4;

/// SQLite-backed implementation of the GraphStore trait.
pub struct SqliteGraphStore {
    pub(crate) conn: Connection,
}

impl SqliteGraphStore {
    /// Open or create a graph database at the given path.
    pub fn open(path: &str) -> Result<Self, GraphError> {
        let conn = Connection::open(path)?;
        Self::set_performance_pragmas(&conn)?;
        let store = SqliteGraphStore { conn };
        store.initialize_schema()?;
        Ok(store)
    }

    /// Create an in-memory graph database (for testing).
    pub fn in_memory() -> Result<Self, GraphError> {
        let conn = Connection::open_in_memory()?;
        Self::set_performance_pragmas(&conn)?;
        let store = SqliteGraphStore { conn };
        store.initialize_schema()?;
        Ok(store)
    }

    /// Apply SQLite performance pragmas for faster reads and writes.
    fn set_performance_pragmas(conn: &Connection) -> Result<(), GraphError> {
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA cache_size = -8000;
            PRAGMA temp_store = MEMORY;
            PRAGMA mmap_size = 268435456;
            PRAGMA foreign_keys = ON;
            ",
        )?;
        Ok(())
    }

    /// Temporarily disable foreign key enforcement (for bulk re-map operations).
    /// Returns the actual FK state after the change (for verification).
    pub fn set_foreign_keys(&self, enabled: bool) -> Result<bool, GraphError> {
        let val = if enabled { "ON" } else { "OFF" };
        self.conn
            .execute_batch(&format!("PRAGMA foreign_keys = {};", val))?;
        // Verify the change took effect
        let actual: i32 = self
            .conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap_or(if enabled { 1 } else { 0 });
        Ok(actual != 0)
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
                package TEXT DEFAULT NULL,
                resolution_tier TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_nodes_hash ON nodes(hash);
            CREATE INDEX IF NOT EXISTS idx_nodes_file ON nodes(file_path);
            CREATE INDEX IF NOT EXISTS idx_nodes_module ON nodes(module_id);
            CREATE INDEX IF NOT EXISTS idx_nodes_kind ON nodes(kind);
            CREATE INDEX IF NOT EXISTS idx_nodes_name_kind ON nodes(name, kind);

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
                confidence REAL NOT NULL DEFAULT 1.0,
                file_path TEXT NOT NULL,
                line INTEGER NOT NULL,
                UNIQUE(source_id, target_id, kind, file_path, line)
            );
            CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_id);
            CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_id);
            CREATE INDEX IF NOT EXISTS idx_edges_source_kind ON edges(source_id, kind);

            -- Module profiles
            CREATE TABLE IF NOT EXISTS module_profiles (
                module_id INTEGER PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
                path TEXT NOT NULL,
                function_count INTEGER NOT NULL DEFAULT 0,
                class_count INTEGER NOT NULL DEFAULT 0,
                line_count INTEGER NOT NULL DEFAULT 0,
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

        // Set schema version if not present (new databases get current version)
        self.conn.execute(
            "INSERT OR IGNORE INTO keel_meta (key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;

        // Run migrations for existing databases
        self.run_migrations()?;

        // Create indexes that depend on columns added by migrations.
        // These use IF NOT EXISTS so they're safe to run on every open.
        let _ = self
            .conn
            .execute_batch("CREATE INDEX IF NOT EXISTS idx_nodes_package ON nodes(package)");

        Ok(())
    }

    /// Run schema migrations from current version to SCHEMA_VERSION.
    fn run_migrations(&self) -> Result<(), GraphError> {
        let current = self.schema_version()?;
        if current >= SCHEMA_VERSION {
            return Ok(());
        }
        if current < 2 {
            self.migrate_v1_to_v2()?;
        }
        if current < 3 {
            self.migrate_v2_to_v3()?;
        }
        if current < 4 {
            self.migrate_v3_to_v4()?;
        }
        Ok(())
    }

    /// Migrate from schema v1 to v2: add resolution_tier to nodes, confidence to edges.
    fn migrate_v1_to_v2(&self) -> Result<(), GraphError> {
        // Add resolution_tier column to nodes (ignore if already exists)
        let _ = self
            .conn
            .execute_batch("ALTER TABLE nodes ADD COLUMN resolution_tier TEXT NOT NULL DEFAULT ''");
        // Add confidence column to edges (ignore if already exists)
        let _ = self
            .conn
            .execute_batch("ALTER TABLE edges ADD COLUMN confidence REAL NOT NULL DEFAULT 1.0");
        // Update schema version to 2
        self.conn.execute(
            "UPDATE keel_meta SET value = '2' WHERE key = 'schema_version'",
            [],
        )?;
        Ok(())
    }

    /// Migrate from schema v2 to v3: add package column to nodes.
    fn migrate_v2_to_v3(&self) -> Result<(), GraphError> {
        let _ = self
            .conn
            .execute_batch("ALTER TABLE nodes ADD COLUMN package TEXT DEFAULT NULL");
        let _ = self
            .conn
            .execute_batch("CREATE INDEX IF NOT EXISTS idx_nodes_package ON nodes(package)");
        self.conn.execute(
            "UPDATE keel_meta SET value = '3' WHERE key = 'schema_version'",
            [],
        )?;
        Ok(())
    }

    /// Migrate from schema v3 to v4: extend resolution_cache for Tier 3.
    fn migrate_v3_to_v4(&self) -> Result<(), GraphError> {
        let _ = self.conn.execute_batch(
            "ALTER TABLE resolution_cache ADD COLUMN file_content_hash TEXT DEFAULT NULL",
        );
        let _ = self
            .conn
            .execute_batch("ALTER TABLE resolution_cache ADD COLUMN target_file TEXT DEFAULT NULL");
        let _ = self
            .conn
            .execute_batch("ALTER TABLE resolution_cache ADD COLUMN target_name TEXT DEFAULT NULL");
        let _ = self
            .conn
            .execute_batch("ALTER TABLE resolution_cache ADD COLUMN provider TEXT DEFAULT NULL");
        self.conn.execute(
            "UPDATE keel_meta SET value = '4' WHERE key = 'schema_version'",
            [],
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

    /// Remove edges whose source or target node no longer exists.
    pub fn cleanup_orphaned_edges(&self) -> Result<u64, GraphError> {
        let deleted = self.conn.execute(
            "DELETE FROM edges WHERE source_id NOT IN (SELECT id FROM nodes) OR target_id NOT IN (SELECT id FROM nodes)",
            [],
        )?;
        Ok(deleted as u64)
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
            package: row.get::<_, Option<String>>("package").unwrap_or(None),
        })
    }

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

    pub(crate) fn node_with_relations(&self, mut node: GraphNode) -> GraphNode {
        node.external_endpoints = self.load_endpoints(node.id);
        node.previous_hashes = self.load_previous_hashes(node.id);
        node
    }

    /// Insert or update module profiles in bulk.
    /// Uses INSERT ... ON CONFLICT DO UPDATE for upsert semantics.
    pub fn upsert_module_profiles(
        &self,
        profiles: Vec<crate::types::ModuleProfile>,
    ) -> Result<(), GraphError> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO module_profiles (
                    module_id, path, function_count, class_count, line_count,
                    function_name_prefixes, primary_types, import_sources,
                    export_targets, external_endpoint_count, responsibility_keywords
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ON CONFLICT(module_id) DO UPDATE SET
                    path = excluded.path,
                    function_count = excluded.function_count,
                    class_count = excluded.class_count,
                    line_count = excluded.line_count,
                    function_name_prefixes = excluded.function_name_prefixes,
                    primary_types = excluded.primary_types,
                    import_sources = excluded.import_sources,
                    export_targets = excluded.export_targets,
                    external_endpoint_count = excluded.external_endpoint_count,
                    responsibility_keywords = excluded.responsibility_keywords",
            )?;
            for p in &profiles {
                let prefixes_json = serde_json::to_string(&p.function_name_prefixes)
                    .unwrap_or_else(|_| "[]".to_string());
                let types_json =
                    serde_json::to_string(&p.primary_types).unwrap_or_else(|_| "[]".to_string());
                let imports_json =
                    serde_json::to_string(&p.import_sources).unwrap_or_else(|_| "[]".to_string());
                let exports_json =
                    serde_json::to_string(&p.export_targets).unwrap_or_else(|_| "[]".to_string());
                let keywords_json = serde_json::to_string(&p.responsibility_keywords)
                    .unwrap_or_else(|_| "[]".to_string());
                stmt.execute(params![
                    p.module_id,
                    p.path,
                    p.function_count,
                    p.class_count,
                    p.line_count,
                    prefixes_json,
                    types_json,
                    imports_json,
                    exports_json,
                    p.external_endpoint_count,
                    keywords_json,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "sqlite_tests.rs"]
mod tests;
