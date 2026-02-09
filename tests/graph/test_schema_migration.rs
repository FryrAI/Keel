// Tests for schema migration and versioning (Spec 000 - Graph Schema)
//
// use keel_core::storage::{SqliteGraphStore, SchemaVersion};

#[test]
#[ignore = "Not yet implemented"]
/// Opening an existing database should report its current schema version.
fn test_schema_version_tracking() {
    // GIVEN a SQLite database with schema version 1
    // WHEN SqliteGraphStore::open reads the version
    // THEN it reports version 1
}

#[test]
#[ignore = "Not yet implemented"]
/// Opening a v1 database with v2 code should trigger automatic migration.
fn test_v1_to_v2_migration() {
    // GIVEN a SQLite database at schema version 1
    // WHEN SqliteGraphStore::open runs with v2 code
    // THEN the schema is migrated to v2 and data is preserved
}

#[test]
#[ignore = "Not yet implemented"]
/// Migrated data should be queryable using v2 APIs.
fn test_migrated_data_accessible() {
    // GIVEN a v1 database that has been migrated to v2
    // WHEN nodes and edges are queried using v2 APIs
    // THEN all pre-existing data is correctly accessible
}

#[test]
#[ignore = "Not yet implemented"]
/// Opening a database newer than the running code should fail gracefully.
fn test_future_schema_version_rejected() {
    // GIVEN a SQLite database at schema version 99
    // WHEN SqliteGraphStore::open tries to use it with current code
    // THEN an error is returned indicating incompatible schema version
}

#[test]
#[ignore = "Not yet implemented"]
/// Migration should be idempotent (running twice does not corrupt data).
fn test_migration_idempotency() {
    // GIVEN a v1 database
    // WHEN migration to v2 is triggered twice
    // THEN the database is at v2 with correct data (no duplication or corruption)
}
