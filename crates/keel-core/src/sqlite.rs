use rusqlite::{params, Connection};

use crate::types::GraphError;

/// Module-profile persistence, split out to keep this file under 400 lines.
#[path = "sqlite_profiles.rs"]
mod profiles;

const SCHEMA_VERSION: u32 = 5;

/// DDL for the body-hash duplicate index (schema v5).
///
/// Shared by `initialize_schema` (fresh databases) and `migrate_v4_to_v5`
/// (existing ones) so the two can never drift apart. Lookups are by
/// `body_hash`, so that column carries the index.
///
/// The primary key is the composite `(node_hash, file_path, line)`, not
/// `node_hash` alone: hash disambiguation salts with the file path only, so
/// two byte-identical definitions in the *same* file share a node hash. A
/// bare `node_hash` key would let one silently replace the other and make
/// W006 attribution nondeterministic.
const BODY_INDEX_DDL: &str = "
    CREATE TABLE IF NOT EXISTS body_index (
        node_hash TEXT NOT NULL,
        body_hash TEXT NOT NULL,
        name TEXT NOT NULL,
        file_path TEXT NOT NULL,
        line INTEGER NOT NULL,
        PRIMARY KEY (node_hash, file_path, line)
    );
    CREATE INDEX IF NOT EXISTS idx_body_index_body_hash ON body_index(body_hash);
";

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
    ///
    /// `busy_timeout` is critical for multi-writer scenarios: many parallel
    /// Claude Code sessions (hooks + MCP server + manual `keel map`) may open
    /// the same `graph.db` concurrently. Without it, a second writer hits
    /// `SQLITE_BUSY` immediately instead of waiting for the lock to clear.
    fn set_performance_pragmas(conn: &Connection) -> Result<(), GraphError> {
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA cache_size = -8000;
            PRAGMA temp_store = MEMORY;
            PRAGMA mmap_size = 268435456;
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;
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

        // Body-hash duplicate index (schema v5) — shared DDL, see BODY_INDEX_DDL.
        self.drop_stale_body_index()?;
        self.conn.execute_batch(BODY_INDEX_DDL)?;

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
        if current < 5 {
            self.migrate_v4_to_v5()?;
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

    /// Drop `body_index` if it predates the composite primary key.
    ///
    /// v5 briefly carried a bare `node_hash` primary key, which silently drops
    /// duplicate definitions that share a node hash — precisely the case the
    /// index exists to catch. `CREATE TABLE IF NOT EXISTS` cannot fix an
    /// existing table, and a v5 database skips migrations entirely, so detect
    /// the old shape and recreate it.
    ///
    /// Cheap and safe: the index is a derived cache that `keel map` rebuilds.
    /// Removable once v5 has shipped and no stale databases remain.
    fn drop_stale_body_index(&self) -> Result<(), GraphError> {
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'body_index'",
                [],
                |row| row.get(0),
            )
            .ok();
        if let Some(sql) = existing {
            if !sql.contains("PRIMARY KEY (node_hash, file_path, line)") {
                self.conn.execute_batch("DROP TABLE body_index")?;
                // Otherwise W006 goes silent until the next map and reads
                // as a regression instead of a schema refresh.
                eprintln!(
                    "keel: body index schema updated — run `keel map` to \
                     rebuild duplicate detection"
                );
            }
        }
        Ok(())
    }

    /// Migrate from schema v4 to v5: add the body-hash duplicate index.
    ///
    /// Purely additive. `initialize_schema` has already run its
    /// `CREATE TABLE IF NOT EXISTS`, so the table and index exist by the time
    /// migrations run; they are repeated here so the step is self-contained
    /// and safe to run against a v4 database opened by any path.
    fn migrate_v4_to_v5(&self) -> Result<(), GraphError> {
        self.conn.execute_batch(BODY_INDEX_DDL)?;
        self.conn.execute(
            "UPDATE keel_meta SET value = '5' WHERE key = 'schema_version'",
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

    /// Persist the resolution tier that resolved each node's outgoing edges.
    ///
    /// Keyed by node id, writing the `nodes.resolution_tier` column that the
    /// map pipeline populates from the per-language resolver's reported tier.
    pub fn set_node_resolution_tiers(
        &mut self,
        tiers: &std::collections::HashMap<u64, String>,
    ) -> Result<(), GraphError> {
        if tiers.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        for (id, tier) in tiers {
            tx.execute(
                "UPDATE nodes SET resolution_tier = ?1 WHERE id = ?2",
                params![tier, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Highest id currently in use across nodes and edges.
    ///
    /// The incremental `keel compile` graph sync allocates new node/edge ids
    /// above this watermark so they never collide with rows written by `keel
    /// map` or a previous compile. Returns 0 for an empty graph.
    pub fn max_id(&self) -> u64 {
        let node_max: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(id), 0) FROM nodes", [], |r| r.get(0))
            .unwrap_or(0);
        let edge_max: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(id), 0) FROM edges", [], |r| r.get(0))
            .unwrap_or(0);
        node_max.max(edge_max) as u64
    }

    /// Delete every outgoing `calls` edge originating in `file_path`.
    ///
    /// Call edges are stored with the caller's file path, so this prunes a
    /// single file's outgoing call edges before the incremental compile sync
    /// re-resolves them. Returns the number of rows removed.
    pub fn prune_call_edges_from_file(&self, file_path: &str) -> Result<u64, GraphError> {
        let deleted = self.conn.execute(
            "DELETE FROM edges WHERE file_path = ?1 AND kind = 'calls'",
            params![file_path],
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
            DELETE FROM body_index;
            DELETE FROM nodes;
            ",
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "sqlite_tests.rs"]
mod tests;
