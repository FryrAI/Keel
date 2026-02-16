// Tests for schema migration and versioning (Spec 000 - Graph Schema)

use keel_core::sqlite::SqliteGraphStore;

#[test]
/// Opening an existing database should report its current schema version.
fn test_schema_version_tracking() {
    // GIVEN a fresh in-memory SQLite database (auto-creates schema v1)
    let store = SqliteGraphStore::in_memory().expect("in-memory store");

    // WHEN schema_version is queried
    let version = store.schema_version().expect("schema_version should succeed");

    // THEN it reports version 1
    assert_eq!(version, 1, "initial schema version should be 1");
}

#[test]
#[ignore = "BUG: v2 migration not yet implemented"]
/// Opening a v1 database with v2 code should trigger automatic migration.
fn test_v1_to_v2_migration() {
    // There is only v1 currently. This test will be implemented
    // when v2 migration logic is added to SqliteGraphStore.
}

#[test]
#[ignore = "BUG: v2 migration not yet implemented"]
/// Migrated data should be queryable using v2 APIs.
fn test_migrated_data_accessible() {
    // Depends on v2 migration existing. This test will be implemented
    // when v2 schema and migration path are available.
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
        assert_eq!(store.schema_version().unwrap(), 1);
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
    // BUG: When future-version detection is implemented, this should
    // return an error instead of silently opening the database.
}

#[test]
/// Migration should be idempotent (opening store twice at same path keeps v1).
fn test_migration_idempotency() {
    // GIVEN a temporary database file
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let db_path = dir.path().join("test.db");
    let db_path_str = db_path.to_str().expect("valid path");

    // WHEN the store is opened for the first time
    {
        let store = SqliteGraphStore::open(db_path_str).expect("first open");
        let v = store.schema_version().expect("version check");
        assert_eq!(v, 1, "first open should be v1");
    }

    // AND the store is opened again at the same path
    {
        let store = SqliteGraphStore::open(db_path_str).expect("second open");
        let v = store.schema_version().expect("version check");

        // THEN the schema version is still 1 (no corruption or double-migration)
        assert_eq!(v, 1, "second open should still be v1");
    }
}
