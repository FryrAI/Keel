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
#[ignore = "BUG: future schema version check not implemented"]
/// Opening a database newer than the running code should fail gracefully.
fn test_future_schema_version_rejected() {
    // SqliteGraphStore does not currently validate schema version on open.
    // When future-version detection is added, this test should:
    // 1. Create a database
    // 2. Manually set schema_version to 99
    // 3. Attempt to open with current code
    // 4. Expect an error indicating incompatible schema version
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
