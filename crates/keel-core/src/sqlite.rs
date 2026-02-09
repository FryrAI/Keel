use rusqlite::{params, Connection, Result as SqlResult};

use crate::store::GraphStore;
use crate::types::{
    EdgeChange, EdgeDirection, EdgeKind, ExternalEndpoint, GraphEdge, GraphError, GraphNode,
    ModuleProfile, NodeChange, NodeKind,
};

const SCHEMA_VERSION: u32 = 1;

/// SQLite-backed implementation of the GraphStore trait.
pub struct SqliteGraphStore {
    conn: Connection,
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

    fn row_to_node(row: &rusqlite::Row) -> SqlResult<GraphNode> {
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

    fn load_endpoints(&self, node_id: u64) -> Vec<ExternalEndpoint> {
        let mut stmt = self
            .conn
            .prepare("SELECT kind, method, path, direction FROM external_endpoints WHERE node_id = ?1")
            .unwrap_or_else(|_| panic!("Failed to prepare endpoint query"));

        stmt.query_map(params![node_id], |row| {
            Ok(ExternalEndpoint {
                kind: row.get(0)?,
                method: row.get(1)?,
                path: row.get(2)?,
                direction: row.get(3)?,
            })
        })
        .unwrap_or_else(|_| panic!("Failed to query endpoints"))
        .filter_map(|r| r.ok())
        .collect()
    }

    fn load_previous_hashes(&self, node_id: u64) -> Vec<String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT hash FROM previous_hashes WHERE node_id = ?1 ORDER BY created_at DESC LIMIT 3",
            )
            .unwrap_or_else(|_| panic!("Failed to prepare previous hashes query"));

        stmt.query_map(params![node_id], |row| row.get(0))
            .unwrap_or_else(|_| panic!("Failed to query previous hashes"))
            .filter_map(|r| r.ok())
            .collect()
    }

    fn node_with_relations(&self, mut node: GraphNode) -> GraphNode {
        node.external_endpoints = self.load_endpoints(node.id);
        node.previous_hashes = self.load_previous_hashes(node.id);
        node
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

        let mut stmt = self.conn.prepare(query).unwrap();
        stmt.query_map(params![node_id], |row| {
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
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
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
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM nodes WHERE file_path = ?1")
            .unwrap();
        stmt.query_map(params![file_path], Self::row_to_node)
            .unwrap()
            .filter_map(|r| r.ok())
            .map(|n| self.node_with_relations(n))
            .collect()
    }

    fn get_all_modules(&self) -> Vec<GraphNode> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM nodes WHERE kind = 'module'")
            .unwrap();
        stmt.query_map([], Self::row_to_node)
            .unwrap()
            .filter_map(|r| r.ok())
            .map(|n| self.node_with_relations(n))
            .collect()
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
                    // INSERT OR REPLACE to handle re-map of same nodes
                    tx.execute(
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
                }
                NodeChange::Update(node) => {
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
                        "INSERT OR REPLACE INTO edges (id, source_id, target_id, kind, file_path, line) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NodeKind;

    fn test_node(id: u64, hash: &str, name: &str) -> GraphNode {
        GraphNode {
            id,
            hash: hash.to_string(),
            kind: NodeKind::Function,
            name: name.to_string(),
            signature: format!("fn {}()", name),
            file_path: "src/test.rs".to_string(),
            line_start: 1,
            line_end: 10,
            docstring: None,
            is_public: true,
            type_hints_present: true,
            has_docstring: false,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 0,
        }
    }

    #[test]
    fn test_create_and_read_node() {
        let mut store = SqliteGraphStore::in_memory().unwrap();
        let node = test_node(1, "abc12345678", "test_fn");
        store.update_nodes(vec![NodeChange::Add(node.clone())]).unwrap();

        let retrieved = store.get_node("abc12345678").unwrap();
        assert_eq!(retrieved.name, "test_fn");
        assert_eq!(retrieved.hash, "abc12345678");
    }

    #[test]
    fn test_get_node_by_id() {
        let mut store = SqliteGraphStore::in_memory().unwrap();
        let node = test_node(42, "def12345678", "lookup_fn");
        store.update_nodes(vec![NodeChange::Add(node)]).unwrap();

        let retrieved = store.get_node_by_id(42).unwrap();
        assert_eq!(retrieved.name, "lookup_fn");
    }

    #[test]
    fn test_update_node() {
        let mut store = SqliteGraphStore::in_memory().unwrap();
        let node = test_node(1, "abc12345678", "old_name");
        store.update_nodes(vec![NodeChange::Add(node)]).unwrap();

        let mut updated = test_node(1, "xyz12345678", "new_name");
        updated.signature = "fn new_name() -> i32".to_string();
        store.update_nodes(vec![NodeChange::Update(updated)]).unwrap();

        let retrieved = store.get_node_by_id(1).unwrap();
        assert_eq!(retrieved.name, "new_name");
        assert_eq!(retrieved.hash, "xyz12345678");
    }

    #[test]
    fn test_remove_node() {
        let mut store = SqliteGraphStore::in_memory().unwrap();
        let node = test_node(1, "abc12345678", "doomed_fn");
        store.update_nodes(vec![NodeChange::Add(node)]).unwrap();
        store.update_nodes(vec![NodeChange::Remove(1)]).unwrap();

        assert!(store.get_node_by_id(1).is_none());
    }

    #[test]
    fn test_edges() {
        let mut store = SqliteGraphStore::in_memory().unwrap();
        let n1 = test_node(1, "aaa12345678", "caller");
        let n2 = test_node(2, "bbb12345678", "callee");
        store.update_nodes(vec![NodeChange::Add(n1), NodeChange::Add(n2)]).unwrap();

        let edge = GraphEdge {
            id: 1,
            source_id: 1,
            target_id: 2,
            kind: EdgeKind::Calls,
            file_path: "src/test.rs".to_string(),
            line: 5,
        };
        store.update_edges(vec![EdgeChange::Add(edge)]).unwrap();

        let outgoing = store.get_edges(1, EdgeDirection::Outgoing);
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].target_id, 2);

        let incoming = store.get_edges(2, EdgeDirection::Incoming);
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0].source_id, 1);
    }

    #[test]
    fn test_schema_version() {
        let store = SqliteGraphStore::in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn test_readd_same_node_no_unique_constraint_error() {
        let mut store = SqliteGraphStore::in_memory().unwrap();
        let node = test_node(1, "abc12345678", "test_fn");
        store.update_nodes(vec![NodeChange::Add(node.clone())]).unwrap();
        store
            .update_nodes(vec![NodeChange::Add(node)])
            .expect("Re-adding same node should not fail with UNIQUE constraint");
        let retrieved = store.get_node("abc12345678").unwrap();
        assert_eq!(retrieved.name, "test_fn");
    }

    #[test]
    fn test_readd_same_edge_no_unique_constraint_error() {
        let mut store = SqliteGraphStore::in_memory().unwrap();
        let n1 = test_node(1, "aaa12345678", "caller");
        let n2 = test_node(2, "bbb12345678", "callee");
        store.update_nodes(vec![NodeChange::Add(n1), NodeChange::Add(n2)]).unwrap();
        let edge = GraphEdge {
            id: 1, source_id: 1, target_id: 2, kind: EdgeKind::Calls,
            file_path: "src/test.rs".to_string(), line: 5,
        };
        store.update_edges(vec![EdgeChange::Add(edge.clone())]).unwrap();
        store
            .update_edges(vec![EdgeChange::Add(edge)])
            .expect("Re-adding same edge should not fail with UNIQUE constraint");
        assert_eq!(store.get_edges(1, EdgeDirection::Outgoing).len(), 1);
    }

    #[test]
    fn test_hash_collision_different_names_still_errors() {
        let mut store = SqliteGraphStore::in_memory().unwrap();
        let node1 = test_node(1, "collision_hash", "func_a");
        store.update_nodes(vec![NodeChange::Add(node1)]).unwrap();
        let node2 = test_node(2, "collision_hash", "func_b");
        assert!(
            store.update_nodes(vec![NodeChange::Add(node2)]).is_err(),
            "Hash collision between different functions should still error"
        );
    }
}
