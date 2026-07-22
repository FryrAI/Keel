// Tests for schema migration and versioning (Spec 000 - Graph Schema)

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeDirection, EdgeKind, GraphEdge};

/// The `edges` table as schema v5 declared it: same columns, but a CHECK
/// constraint that predates `uses`.
const V5_EDGES_DDL: &str = "
    CREATE TABLE edges_v5 (
        id INTEGER PRIMARY KEY,
        source_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
        target_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
        kind TEXT NOT NULL CHECK (kind IN ('calls', 'imports', 'inherits', 'contains')),
        confidence REAL NOT NULL DEFAULT 1.0,
        file_path TEXT NOT NULL,
        line INTEGER NOT NULL,
        UNIQUE(source_id, target_id, kind, file_path, line)
    );
";

#[test]
/// A v5 database with existing edges must migrate to v6 (which rebuilds the
/// table — SQLite cannot ALTER a CHECK constraint) without losing rows, and
/// must accept `uses` edges afterwards.
fn test_v5_to_v6_migration_preserves_edges_and_allows_uses() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("v5.db");
    let db_str = db_path.to_str().unwrap();

    // GIVEN a v5 database: current schema, but the edges table carries the
    // pre-v6 CHECK and holds a call edge between two real nodes.
    {
        drop(SqliteGraphStore::open(db_str).unwrap());
        let conn = rusqlite::Connection::open(db_str).unwrap();
        conn.execute_batch(&format!(
            "PRAGMA foreign_keys = OFF;
             {V5_EDGES_DDL}
             DROP TABLE edges;
             ALTER TABLE edges_v5 RENAME TO edges;
             INSERT INTO nodes (id, hash, kind, name, file_path, line_start, line_end) VALUES
                 (1, 'caller_hash', 'function', 'caller', 'src/a.rs', 1, 5),
                 (2, 'callee_hash', 'function', 'callee', 'src/b.rs', 1, 5);
             INSERT INTO edges (id, source_id, target_id, kind, confidence, file_path, line)
                 VALUES (3, 1, 2, 'calls', 0.95, 'src/a.rs', 2);
             UPDATE keel_meta SET value = '5' WHERE key = 'schema_version';"
        ))
        .unwrap();
        // Sanity: at v5 a 'uses' edge is rejected by the CHECK constraint.
        assert!(
            conn.execute(
                "INSERT INTO edges (id, source_id, target_id, kind, confidence, file_path, line)
                 VALUES (4, 1, 2, 'uses', 0.9, 'src/a.rs', 3)",
                [],
            )
            .is_err(),
            "v5 CHECK must reject 'uses'"
        );
    }

    // WHEN reopened with v6 code
    let mut store = SqliteGraphStore::open(db_str).unwrap();

    // THEN the version advanced and the pre-existing edge survived the rebuild
    assert_eq!(store.schema_version().unwrap(), 6, "v5 db should reach v6");
    let incoming = store.get_edges(2, EdgeDirection::Incoming);
    assert_eq!(incoming.len(), 1, "existing edge must survive the rebuild");
    assert_eq!(incoming[0].kind, EdgeKind::Calls);
    assert_eq!(incoming[0].id, 3, "edge ids are preserved");
    assert!((incoming[0].confidence - 0.95).abs() < f64::EPSILON);

    // AND a `uses` edge is now accepted and round-trips
    store
        .update_edges(vec![EdgeChange::Add(GraphEdge {
            id: 4,
            source_id: 1,
            target_id: 2,
            kind: EdgeKind::Uses,
            confidence: 0.9,
            file_path: "src/a.rs".into(),
            line: 3,
        })])
        .expect("uses edge must be storable after migration");
    let incoming = store.get_edges(2, EdgeDirection::Incoming);
    assert_eq!(incoming.len(), 2);
    assert!(incoming.iter().any(|e| e.kind == EdgeKind::Uses));

    // AND the edge indexes were recreated (they are dropped with the table)
    drop(store);
    let conn = rusqlite::Connection::open(db_str).unwrap();
    let index_count: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'index' AND tbl_name = 'edges' AND name LIKE 'idx_edges%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(index_count, 3, "edge indexes must be recreated");
}

#[test]
/// Opening an existing database should report its current schema version.
fn test_schema_version_tracking() {
    // GIVEN a fresh in-memory SQLite database (auto-creates the current schema)
    let store = SqliteGraphStore::in_memory().expect("in-memory store");

    // WHEN schema_version is queried
    let version = store
        .schema_version()
        .expect("schema_version should succeed");

    // THEN it reports the current version
    assert_eq!(version, 6, "initial schema version should be 6");
}

#[test]
/// Opening a v1 database with v2 code should trigger automatic migration.
fn test_v1_to_v2_migration() {
    // GIVEN a database created with v1 schema
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("v1.db");
    let db_str = db_path.to_str().unwrap();

    // Create a v1 database manually (without the v2 columns)
    {
        let conn = rusqlite::Connection::open(db_str).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE keel_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO keel_meta (key, value) VALUES ('schema_version', '1');

            CREATE TABLE nodes (
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
                module_id INTEGER,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE edges (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                target_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                kind TEXT NOT NULL CHECK (kind IN ('calls', 'imports', 'inherits', 'contains')),
                file_path TEXT NOT NULL,
                line INTEGER NOT NULL,
                UNIQUE(source_id, target_id, kind, file_path, line)
            );
            ",
        )
        .unwrap();
    }

    // WHEN re-opened with v2 code
    let store = SqliteGraphStore::open(db_str).unwrap();

    // THEN schema version is now 2
    let version = store.schema_version().unwrap();
    assert_eq!(version, 6, "v1 database should be migrated to v6");
}

#[test]
/// Migrated data should be queryable using v2 APIs.
fn test_migrated_data_accessible() {
    // GIVEN a v1 database with some existing data
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("v1_with_data.db");
    let db_str = db_path.to_str().unwrap();

    {
        let conn = rusqlite::Connection::open(db_str).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE keel_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO keel_meta (key, value) VALUES ('schema_version', '1');

            CREATE TABLE nodes (
                id INTEGER PRIMARY KEY,
                hash TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                signature TEXT NOT NULL DEFAULT '',
                file_path TEXT NOT NULL,
                line_start INTEGER NOT NULL,
                line_end INTEGER NOT NULL,
                docstring TEXT,
                is_public INTEGER NOT NULL DEFAULT 0,
                type_hints_present INTEGER NOT NULL DEFAULT 0,
                has_docstring INTEGER NOT NULL DEFAULT 0,
                module_id INTEGER,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE edges (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL,
                target_id INTEGER NOT NULL,
                kind TEXT NOT NULL,
                file_path TEXT NOT NULL,
                line INTEGER NOT NULL,
                UNIQUE(source_id, target_id, kind, file_path, line)
            );

            INSERT INTO nodes (hash, kind, name, file_path, line_start, line_end) VALUES
                ('abc123', 'function', 'hello', 'main.rs', 1, 5);
            ",
        )
        .unwrap();
    }

    // WHEN re-opened with v2 code
    let store = SqliteGraphStore::open(db_str).unwrap();

    // THEN the migrated data is queryable
    assert_eq!(store.schema_version().unwrap(), 6);

    // Drop store so we can open raw connection
    drop(store);

    // AND v2 columns have default values (verify via raw rusqlite)
    let conn = rusqlite::Connection::open(db_str).unwrap();
    let tier: String = conn
        .query_row(
            "SELECT resolution_tier FROM nodes WHERE hash = 'abc123'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tier, "", "resolution_tier should default to empty string");

    // AND original data is preserved
    let name: String = conn
        .query_row("SELECT name FROM nodes WHERE hash = 'abc123'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        name, "hello",
        "original data should be preserved after migration"
    );
}

#[test]
/// Opening a database with a future schema version should be handled.
/// Currently, SqliteGraphStore does NOT validate schema version on open.
/// The INSERT OR IGNORE preserves the existing version, so a future
/// version survives the open call. This documents the current behavior.
fn test_future_schema_version_not_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("future.db");
    let db_str = db_path.to_str().unwrap();

    // First create a valid database
    {
        let store = SqliteGraphStore::open(db_str).unwrap();
        assert_eq!(store.schema_version().unwrap(), 6);
    }

    // Manually set schema_version to 99 via raw SQL
    {
        let conn = rusqlite::Connection::open(db_str).unwrap();
        conn.execute(
            "UPDATE keel_meta SET value = '99' WHERE key = 'schema_version'",
            [],
        )
        .unwrap();
    }

    // Re-open with SqliteGraphStore — currently does NOT reject future versions
    let store = SqliteGraphStore::open(db_str).unwrap();
    let version = store.schema_version().unwrap();
    assert_eq!(
        version, 99,
        "future schema version should be preserved (not rejected or downgraded)"
    );
}

#[test]
/// Migration should be idempotent (opening store twice at same path keeps v2).
fn test_migration_idempotency() {
    // GIVEN a temporary database file
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let db_path = dir.path().join("test.db");
    let db_path_str = db_path.to_str().expect("valid path");

    // WHEN the store is opened for the first time
    {
        let store = SqliteGraphStore::open(db_path_str).expect("first open");
        let v = store.schema_version().expect("version check");
        assert_eq!(v, 6, "first open should be v6");
    }

    // AND the store is opened again at the same path
    {
        let store = SqliteGraphStore::open(db_path_str).expect("second open");
        let v = store.schema_version().expect("version check");

        // THEN the schema version is unchanged (no corruption or double-migration)
        assert_eq!(v, 6, "second open should still be v6");
    }
}
