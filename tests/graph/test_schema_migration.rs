// Tests for schema migration and versioning (Spec 000 - Graph Schema)

use keel_core::sqlite::SqliteGraphStore;

#[test]
/// Opening an existing database should report its current schema version.
fn test_schema_version_tracking() {
    // GIVEN a fresh in-memory SQLite database (auto-creates schema v2)
    let store = SqliteGraphStore::in_memory().expect("in-memory store");

    // WHEN schema_version is queried
    let version = store.schema_version().expect("schema_version should succeed");

    // THEN it reports version 2
    assert_eq!(version, 3, "initial schema version should be 3");
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
    assert_eq!(version, 3, "v1 database should be migrated to v3");
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
    assert_eq!(store.schema_version().unwrap(), 3);

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
        .query_row(
            "SELECT name FROM nodes WHERE hash = 'abc123'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(name, "hello", "original data should be preserved after migration");
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
        assert_eq!(store.schema_version().unwrap(), 3);
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
        assert_eq!(v, 3, "first open should be v2");
    }

    // AND the store is opened again at the same path
    {
        let store = SqliteGraphStore::open(db_path_str).expect("second open");
        let v = store.schema_version().expect("version check");

        // THEN the schema version is still 2 (no corruption or double-migration)
        assert_eq!(v, 3, "second open should still be v2");
    }
}
