//! Tests for `previous_hashes` persistence through `GraphStore::update_nodes`.
//!
//! Progressive adoption reads this field back to mean "modified since the last
//! map", so the write path must not drop it. Split from `sqlite_tests.rs` to
//! keep both files under the 400-line cap.

use super::*;

/// `update_nodes` used to drop `previous_hashes` on the floor; progressive
/// adoption reads it back to mean "modified since last map".
#[test]
fn test_update_nodes_persists_previous_hashes_on_update() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(1, "newHash0001", "renamed_fn");
    store
        .update_nodes(vec![NodeChange::Add(node.clone())])
        .unwrap();

    let mut changed = node.clone();
    changed.previous_hashes = vec!["old".to_string()];
    store
        .update_nodes(vec![NodeChange::Update(changed)])
        .unwrap();

    let read_back = store.get_nodes_in_file("src/test.rs");
    assert_eq!(read_back.len(), 1);
    assert_eq!(
        read_back[0].previous_hashes,
        vec!["old".to_string()],
        "previous_hashes must survive a round trip through update_nodes"
    );
}

#[test]
fn test_update_nodes_persists_previous_hashes_on_add() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let mut node = test_node(1, "abc12345678", "fresh_fn");
    node.previous_hashes = vec!["priorHash01".to_string()];

    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();

    let read_back = store.get_nodes_in_file("src/test.rs");
    assert_eq!(
        read_back[0].previous_hashes,
        vec!["priorHash01".to_string()]
    );
}

/// Appending is cumulative across syncs, and idempotent — the engine may
/// replay the same old hash without duplicating it.
#[test]
fn test_previous_hashes_accumulate_and_dedupe() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(1, "hash0000001", "evolving_fn");
    store
        .update_nodes(vec![NodeChange::Add(node.clone())])
        .unwrap();

    for hash in ["gen1", "gen1", "gen2"] {
        let mut changed = node.clone();
        changed.previous_hashes = vec![hash.to_string()];
        store
            .update_nodes(vec![NodeChange::Update(changed)])
            .unwrap();
    }

    let mut found = store.get_nodes_in_file("src/test.rs")[0]
        .previous_hashes
        .clone();
    found.sort();
    assert_eq!(found, vec!["gen1".to_string(), "gen2".to_string()]);
}

/// A node carrying no rename history must not erase history already recorded
/// — parsers construct nodes with an empty vec on every re-map.
#[test]
fn test_empty_previous_hashes_does_not_erase_history() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(1, "hash0000002", "stable_fn");
    store
        .update_nodes(vec![NodeChange::Add(node.clone())])
        .unwrap();

    let mut with_history = node.clone();
    with_history.previous_hashes = vec!["keepme".to_string()];
    store
        .update_nodes(vec![NodeChange::Update(with_history)])
        .unwrap();

    // Re-map style write: same node, no history attached.
    store.update_nodes(vec![NodeChange::Update(node)]).unwrap();

    assert_eq!(
        store.get_nodes_in_file("src/test.rs")[0].previous_hashes,
        vec!["keepme".to_string()],
        "an empty previous_hashes must be a no-op, not a wipe"
    );
}

/// `clear_all` is the intended re-baseline for progressive adoption.
#[test]
fn test_clear_all_resets_previous_hashes() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let mut node = test_node(1, "hash0000003", "fn_with_history");
    node.previous_hashes = vec!["old".to_string()];
    store
        .update_nodes(vec![NodeChange::Add(node.clone())])
        .unwrap();

    store.clear_all().unwrap();
    store
        .update_nodes(vec![NodeChange::Add(test_node(
            1,
            "hash0000003",
            "fn_with_history",
        ))])
        .unwrap();

    assert!(
        store.get_nodes_in_file("src/test.rs")[0]
            .previous_hashes
            .is_empty(),
        "a full re-map should re-baseline history"
    );
}
