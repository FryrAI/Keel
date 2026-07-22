//! Tests for the body-hash duplicate index (schema v5).
//!
//! Split from `sqlite_tests.rs` to keep both files under the 400-line cap.

use super::*;
use crate::store::GraphStore;
use crate::types::BodyIndexEntry;

fn entry(node_hash: &str, body_hash: &str, name: &str, file: &str, line: u32) -> BodyIndexEntry {
    BodyIndexEntry {
        body_hash: body_hash.to_string(),
        node_hash: node_hash.to_string(),
        name: name.to_string(),
        file_path: file.to_string(),
        line,
    }
}

#[test]
fn test_body_index_roundtrip() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![entry("node1", "bodyAAA", "parse", "src/a.rs", 10)])
        .expect("replace should succeed");

    let found = store.find_body_matches("bodyAAA");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0], entry("node1", "bodyAAA", "parse", "src/a.rs", 10));
}

/// The point of the index: two nodes sharing a body hash are both returned.
#[test]
fn test_body_index_finds_all_duplicates() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![
            entry("node1", "dupBody", "parse", "src/a.rs", 10),
            entry("node2", "dupBody", "parseAgain", "src/b.rs", 20),
            entry("node3", "otherBody", "unrelated", "src/c.rs", 30),
        ])
        .unwrap();

    let dups = store.find_body_matches("dupBody");
    assert_eq!(dups.len(), 2, "both duplicates should be returned");
    let names: Vec<&str> = dups.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"parse"));
    assert!(names.contains(&"parseAgain"));

    assert_eq!(store.find_body_matches("otherBody").len(), 1);
}

#[test]
fn test_body_index_replace_overwrites_previous_contents() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![entry("old1", "oldBody", "gone", "src/old.rs", 1)])
        .unwrap();
    assert_eq!(store.find_body_matches("oldBody").len(), 1);

    // A second replace is a full rebuild, not a merge.
    store
        .replace_body_index(vec![entry("new1", "newBody", "kept", "src/new.rs", 2)])
        .unwrap();

    assert!(
        store.find_body_matches("oldBody").is_empty(),
        "stale entries must not survive a rebuild"
    );
    assert_eq!(store.find_body_matches("newBody").len(), 1);
}

#[test]
fn test_body_index_replace_with_empty_clears_index() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![entry("n1", "someBody", "f", "src/a.rs", 1)])
        .unwrap();

    store.replace_body_index(Vec::new()).unwrap();
    assert!(store.find_body_matches("someBody").is_empty());
}

#[test]
fn test_body_index_find_on_empty_index() {
    let store = SqliteGraphStore::in_memory().unwrap();
    assert!(store.find_body_matches("anything").is_empty());
}

#[test]
fn test_body_index_find_unknown_hash() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![entry("n1", "known", "f", "src/a.rs", 1)])
        .unwrap();
    assert!(store.find_body_matches("unknown").is_empty());
}

/// Hash disambiguation salts with the file path only, so two byte-identical
/// definitions in the *same* file share a node hash. Both must survive — a
/// bare `node_hash` primary key silently dropped one, making W006 attribution
/// nondeterministic.
#[test]
fn test_same_node_hash_different_lines_both_survive() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![
            entry("dupHash", "sharedBody", "helper", "src/a.rs", 10),
            entry("dupHash", "sharedBody", "helper", "src/a.rs", 40),
        ])
        .expect("identical node_hash on different lines must not collide");

    let found = store.find_body_matches("sharedBody");
    assert_eq!(found.len(), 2, "both definitions must be indexed");
    let lines: Vec<u32> = found.iter().map(|e| e.line).collect();
    assert_eq!(lines, vec![10, 40], "ordered by file_path, line");
}

/// The same node hash in two different files is also distinct.
#[test]
fn test_same_node_hash_different_files_both_survive() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![
            entry("dupHash", "sharedBody", "helper", "src/a.rs", 1),
            entry("dupHash", "sharedBody", "helper", "src/b.rs", 1),
        ])
        .unwrap();

    assert_eq!(store.find_body_matches("sharedBody").len(), 2);
}

/// `(node_hash, file_path, line)` is the primary key, so re-indexing the exact
/// same location in one batch must not blow up on a constraint violation.
#[test]
fn test_body_index_duplicate_node_hash_in_batch() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![
            entry("sameNode", "bodyA", "first", "src/a.rs", 1),
            entry("sameNode", "bodyB", "second", "src/a.rs", 1),
        ])
        .expect("duplicate node_hash must not error");

    // Last write wins.
    assert!(store.find_body_matches("bodyA").is_empty());
    assert_eq!(store.find_body_matches("bodyB").len(), 1);
}

#[test]
fn test_clear_all_wipes_body_index() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![entry("n1", "someBody", "f", "src/a.rs", 1)])
        .unwrap();

    store.clear_all().unwrap();
    assert!(
        store.find_body_matches("someBody").is_empty(),
        "clear_all must wipe the body index for a clean re-map"
    );
}

#[test]
fn test_fresh_database_is_current() {
    let store = SqliteGraphStore::in_memory().unwrap();
    assert_eq!(store.schema_version().unwrap(), 6);
}

/// A v4 database must migrate to v5 and gain a usable body index, with its
/// existing rows intact.
#[test]
fn test_migration_v4_to_v5() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("v4.db");
    let db_str = db_path.to_str().unwrap();

    // GIVEN a v4 database holding a node.
    {
        let store = SqliteGraphStore::open(db_str).unwrap();
        store
            .conn
            .execute_batch(
                "INSERT INTO nodes (hash, kind, name, file_path, line_start, line_end)
                 VALUES ('keepme', 'function', 'survivor', 'src/a.rs', 1, 5);
                 DROP TABLE body_index;
                 UPDATE keel_meta SET value = '4' WHERE key = 'schema_version';",
            )
            .unwrap();
    }

    // WHEN reopened with v5 code.
    let mut store = SqliteGraphStore::open(db_str).unwrap();

    // THEN the version advanced and the index works.
    assert_eq!(store.schema_version().unwrap(), 6, "v4 db should reach v6");
    store
        .replace_body_index(vec![entry("n1", "b1", "f", "src/a.rs", 1)])
        .expect("body index usable after migration");
    assert_eq!(store.find_body_matches("b1").len(), 1);

    // AND the pre-existing data survived.
    assert!(
        store.get_node("keepme").is_some(),
        "migration must preserve existing rows"
    );
}

/// Reopening an already-migrated database must be a no-op, not a re-migration
/// that wipes the index.
#[test]
fn test_migration_v5_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("v5.db");
    let db_str = db_path.to_str().unwrap();

    {
        let mut store = SqliteGraphStore::open(db_str).unwrap();
        store
            .replace_body_index(vec![entry("n1", "persisted", "f", "src/a.rs", 1)])
            .unwrap();
    }

    let store = SqliteGraphStore::open(db_str).unwrap();
    assert_eq!(store.schema_version().unwrap(), 6);
    assert_eq!(
        store.find_body_matches("persisted").len(),
        1,
        "reopening must not drop indexed entries"
    );
}

/// A database carrying the pre-fix `node_hash`-only primary key must be
/// recreated on open, not silently kept — otherwise it keeps dropping rows.
#[test]
fn test_stale_body_index_schema_is_recreated() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("stale.db");
    let db_str = db_path.to_str().unwrap();

    // GIVEN a database with the old, unsound single-column primary key.
    {
        let store = SqliteGraphStore::open(db_str).unwrap();
        store
            .conn
            .execute_batch(
                "DROP TABLE body_index;
                 CREATE TABLE body_index (
                     node_hash TEXT PRIMARY KEY,
                     body_hash TEXT NOT NULL,
                     name TEXT NOT NULL,
                     file_path TEXT NOT NULL,
                     line INTEGER NOT NULL
                 );",
            )
            .unwrap();
    }

    // WHEN reopened, THEN the composite key is in force and both rows persist.
    let mut store = SqliteGraphStore::open(db_str).unwrap();
    store
        .replace_body_index(vec![
            entry("dupHash", "sharedBody", "helper", "src/a.rs", 10),
            entry("dupHash", "sharedBody", "helper", "src/a.rs", 40),
        ])
        .expect("stale schema should have been recreated");
    assert_eq!(store.find_body_matches("sharedBody").len(), 2);
}

/// End-to-end with the real hasher: reformatted copies of the same function
/// collide in the index, which is what W006 will key on.
#[test]
fn test_body_index_with_real_body_hashes() {
    let mut store = SqliteGraphStore::in_memory().unwrap();

    let original = "let total = 0;\nfor (x of xs) { total += x; }\nreturn total;";
    let reformatted =
        "    let total = 0;\n\n    for (x of xs) {   total += x; }\n    return total;";
    let different = "return xs.reduce((a, b) => a + b, 0);";

    let h_original = crate::hash::compute_body_hash(original);
    let h_different = crate::hash::compute_body_hash(different);

    store
        .replace_body_index(vec![
            entry("nodeA", &h_original, "sumLoop", "src/a.rs", 10),
            entry(
                "nodeB",
                &crate::hash::compute_body_hash(reformatted),
                "sumLoopCopy",
                "src/b.rs",
                20,
            ),
            entry("nodeC", &h_different, "sumReduce", "src/c.rs", 30),
        ])
        .unwrap();

    let dups = store.find_body_matches(&h_original);
    assert_eq!(
        dups.len(),
        2,
        "reformatted copy should collide with the original"
    );
    assert_eq!(store.find_body_matches(&h_different).len(), 1);
}
