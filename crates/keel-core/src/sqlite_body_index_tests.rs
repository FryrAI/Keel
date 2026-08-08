//! Tests for the body-hash duplicate index (schema v5).
//!
//! Split from `sqlite_tests.rs` to keep both files under the 400-line cap.

use super::*;
use crate::store::GraphStore;
use crate::types::{BodyIndexEntry, FragmentCloneEntry};

fn entry(node_hash: &str, body_hash: &str, name: &str, file: &str, line: u32) -> BodyIndexEntry {
    entry_t2(node_hash, body_hash, "", name, file, line)
}

/// An entry carrying a Type-2 fingerprint as well.
fn entry_t2(
    node_hash: &str,
    body_hash: &str,
    t2_hash: &str,
    name: &str,
    file: &str,
    line: u32,
) -> BodyIndexEntry {
    BodyIndexEntry {
        body_hash: body_hash.to_string(),
        t2_hash: t2_hash.to_string(),
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
    assert_eq!(store.schema_version().unwrap(), 7);
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
    assert_eq!(store.schema_version().unwrap(), 7, "v4 db should reach v7");
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
    assert_eq!(store.schema_version().unwrap(), 7);
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

// --- Type-2 fingerprint column (issue #59) ---

#[test]
fn test_body_index_t2_roundtrip() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let e = entry_t2("node1", "bodyAAA", "t2AAA", "parse", "src/a.rs", 10);
    store.replace_body_index(vec![e.clone()]).unwrap();

    assert_eq!(store.find_t2_body_matches("t2AAA"), vec![e.clone()]);
    assert_eq!(
        store.find_body_matches("bodyAAA"),
        vec![e],
        "the Type-1 lookup must still return the same row"
    );
}

/// The point of the second column: bodies that differ only by identifier names
/// share a `t2_hash` while their `body_hash` values differ.
#[test]
fn test_body_index_finds_t2_duplicates() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![
            entry_t2("node1", "bodyA", "sharedT2", "parse", "src/a.rs", 10),
            entry_t2("node2", "bodyB", "sharedT2", "parseAgain", "src/b.rs", 20),
            entry_t2("node3", "bodyC", "otherT2", "unrelated", "src/c.rs", 30),
        ])
        .unwrap();

    let near = store.find_t2_body_matches("sharedT2");
    assert_eq!(near.len(), 2, "both near-clones should be returned");
    assert_eq!(store.find_t2_body_matches("otherT2").len(), 1);

    // The Type-1 namespace is untouched by the Type-2 collision.
    assert_eq!(store.find_body_matches("bodyA").len(), 1);
    assert!(store.find_body_matches("sharedT2").is_empty());
}

/// Pre-upgrade rows (and rows whose body never cleared the Type-2 floor) store
/// `''`. Matching those against each other would make every unindexed function
/// a near-duplicate of every other one.
#[test]
fn test_empty_t2_fingerprint_never_matches() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![
            entry("node1", "bodyA", "parse", "src/a.rs", 10),
            entry("node2", "bodyB", "render", "src/b.rs", 20),
        ])
        .unwrap();

    assert!(store.find_t2_body_matches("").is_empty());
}

/// A database created before the column existed gains it on open, keeps its
/// rows, and reads them back as "not indexed for Type-2".
#[test]
fn test_existing_database_gains_t2_hash_column() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("pre_t2.db");
    let db_str = db_path.to_str().unwrap();

    // GIVEN a body_index with the current primary key but no t2_hash column.
    {
        let store = SqliteGraphStore::open(db_str).unwrap();
        store
            .conn
            .execute_batch(
                "DROP TABLE body_index;
                 CREATE TABLE body_index (
                     node_hash TEXT NOT NULL,
                     body_hash TEXT NOT NULL,
                     name TEXT NOT NULL,
                     file_path TEXT NOT NULL,
                     line INTEGER NOT NULL,
                     PRIMARY KEY (node_hash, file_path, line)
                 );
                 INSERT INTO body_index VALUES ('n1', 'legacyBody', 'old', 'src/old.rs', 3);",
            )
            .unwrap();
    }

    // WHEN reopened.
    let mut store = SqliteGraphStore::open(db_str).unwrap();

    // THEN the legacy row survives, reads back empty, and never matches.
    let legacy = store.find_body_matches("legacyBody");
    assert_eq!(legacy.len(), 1, "existing rows must survive the ALTER");
    assert_eq!(legacy[0].t2_hash, "");
    assert!(store.find_t2_body_matches("").is_empty());

    // AND the column is usable for new writes.
    store
        .replace_body_index(vec![entry_t2(
            "n2",
            "bodyX",
            "t2X",
            "fresh",
            "src/new.rs",
            5,
        )])
        .expect("t2_hash column usable after the idempotent ALTER");
    assert_eq!(store.find_t2_body_matches("t2X").len(), 1);
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

// --- fragment clones (issue #66) -----------------------------------------
//
// Read back through `quality_inputs`, the one consumer: the measurements have
// no lookup API of their own, and asserting on the only path that reads them
// is what keeps the SQL and the metric from drifting.

fn fragment(node_hash: &str, file: &str, cloned: u32, code: u32) -> FragmentCloneEntry {
    FragmentCloneEntry {
        node_hash: node_hash.to_string(),
        name: "f".to_string(),
        file_path: file.to_string(),
        line: 1,
        cloned_lines: cloned,
        code_lines: code,
    }
}

#[test]
fn test_fragment_clones_roundtrip() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_fragment_clones(vec![
            fragment("n1", "src/a.rs", 12, 40),
            fragment("n2", "src/b.rs", 0, 25),
        ])
        .expect("replace should succeed");

    let mut rows = store.quality_inputs().fragment_clones_by_fn;
    rows.sort();
    assert_eq!(
        rows,
        vec![
            ("src/a.rs".to_string(), 12, 40),
            ("src/b.rs".to_string(), 0, 25),
        ]
    );
}

#[test]
fn test_replace_fragment_clones_overwrites_previous_measurement() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_fragment_clones(vec![fragment("old", "src/old.rs", 30, 30)])
        .unwrap();
    store
        .replace_fragment_clones(vec![fragment("new", "src/new.rs", 1, 10)])
        .unwrap();

    let rows = store.quality_inputs().fragment_clones_by_fn;
    assert_eq!(rows, vec![("src/new.rs".to_string(), 1, 10)]);
}

#[test]
fn test_clear_all_wipes_fragment_clones() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_fragment_clones(vec![fragment("n1", "src/a.rs", 5, 10)])
        .unwrap();

    store.clear_all().unwrap();
    assert!(
        store.quality_inputs().fragment_clones_by_fn.is_empty(),
        "clear_all must wipe the measurements for a clean re-map"
    );
}
