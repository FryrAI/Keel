// Schema v7 — `quality_snapshots`, the one table a full re-map does not delete.
//
// Two things are load-bearing here and neither is obvious from reading
// `sqlite.rs`: a v6 database must gain the table without losing its graph, and
// `clear_all` must leave the series alone. If the second one ever regresses,
// `keel quality --trend` silently becomes a report on the current map and
// nothing else — it would still print, it would just have no memory.

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{GraphNode, NodeChange, NodeKind};

fn node(id: u64, name: &str, file: &str) -> GraphNode {
    GraphNode {
        complexity: 0,
        is_trivial_wrapper: false,
        in_test_context: false,
        id,
        hash: format!("qs{id}"),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: format!("fn {name}()"),
        file_path: file.to_string(),
        line_start: 1,
        line_end: 4,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

/// A v6 database (schema v7's table dropped, version marker rewound) must reach
/// v7 on the next open, keeping every node it already held.
#[test]
fn test_v6_to_v7_migration_adds_snapshots_and_keeps_the_graph() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("v6.db");
    let db_str = db_path.to_str().unwrap();

    // GIVEN a database with graph data, rewound to look like v6.
    {
        let mut store = SqliteGraphStore::open(db_str).unwrap();
        store
            .update_nodes(vec![NodeChange::Add(node(1, "keep_me", "src/a.rs"))])
            .unwrap();
    }
    {
        let conn = rusqlite::Connection::open(db_str).unwrap();
        conn.execute_batch(
            "DROP TABLE quality_snapshots;
             UPDATE keel_meta SET value = '6' WHERE key = 'schema_version';",
        )
        .unwrap();
    }

    // WHEN reopened with v7 code
    let store = SqliteGraphStore::open(db_str).unwrap();

    // THEN the version advanced, the table exists and is writable, and the
    // pre-existing graph is untouched.
    assert_eq!(store.schema_version().unwrap(), 7, "v6 db should reach v7");
    assert!(store.get_node("qs1").is_some(), "migration lost graph data");
    store
        .insert_quality_snapshot(Some("abc123"), "{\"version\":1}")
        .expect("v7 table must be writable after migration");
    assert_eq!(store.quality_snapshots(0).len(), 1);
}

/// The single most important property of this table: `keel map` calls
/// `clear_all`, and the series must survive it.
#[test]
fn test_clear_all_preserves_quality_snapshots() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .update_nodes(vec![NodeChange::Add(node(1, "mapped", "src/a.rs"))])
        .unwrap();
    store
        .insert_quality_snapshot(Some("commit1"), "{\"version\":1,\"files_over_budget\":3}")
        .unwrap();

    // A full re-map: everything derived from source is deleted and rebuilt.
    store.clear_all().unwrap();
    assert!(
        store.get_node("qs1").is_none(),
        "clear_all should still clear graph data"
    );
    store
        .update_nodes(vec![NodeChange::Add(node(2, "remapped", "src/a.rs"))])
        .unwrap();

    // A second snapshot, then a second re-map.
    store
        .insert_quality_snapshot(Some("commit2"), "{\"version\":1,\"files_over_budget\":5}")
        .unwrap();
    store.clear_all().unwrap();

    let rows = store.quality_snapshots(0);
    assert_eq!(rows.len(), 2, "two maps must not erase the series");
    assert_eq!(rows[0].commit_sha.as_deref(), Some("commit1"));
    assert_eq!(rows[1].commit_sha.as_deref(), Some("commit2"));
    assert!(rows[0].metrics.contains("\"files_over_budget\":3"));
}

/// `clear_all` enumerates its tables explicitly, so a new table is preserved by
/// omission — which is exactly how one gets deleted by accident later. Assert
/// the intent directly rather than relying on the batch's current text.
#[test]
fn test_clear_all_deletes_every_derived_table_except_snapshots() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .update_nodes(vec![NodeChange::Add(node(1, "derived", "src/a.rs"))])
        .unwrap();
    store
        .insert_quality_snapshot(None, "{\"version\":1}")
        .unwrap();

    store.clear_all().unwrap();

    assert!(store.get_all_modules().is_empty());
    assert!(store.get_nodes_in_file("src/a.rs").is_empty());
    assert_eq!(
        store.quality_snapshots(0).len(),
        1,
        "quality_snapshots must never join the clear_all DELETE batch"
    );
}
