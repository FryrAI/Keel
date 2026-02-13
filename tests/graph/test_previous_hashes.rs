// Tests for previous hash tracking on nodes (Spec 000 - Graph Schema)

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{GraphNode, NodeChange, NodeKind};

fn make_node(id: u64, hash: &str, name: &str, kind: NodeKind) -> GraphNode {
    GraphNode {
        id,
        hash: hash.into(),
        kind,
        name: name.into(),
        signature: format!("{name}()"),
        file_path: "test.rs".into(),
        line_start: 1,
        line_end: 5,
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
#[ignore = "BUG: previous hash tracking requires app-level update logic"]
/// When a node's hash changes, the old hash should be stored in previous_hashes.
fn test_previous_hash_stored_on_change() {
    // Previous hash tracking requires application-level logic to detect
    // that a node's hash changed and insert the old hash into the
    // previous_hashes table. The GraphStore::update_nodes method does
    // not automatically track previous hashes on Update operations.
}

#[test]
#[ignore = "BUG: previous hash tracking requires app-level update logic"]
/// Previous hashes list should be limited to 3 entries (most recent).
fn test_previous_hashes_limited_to_three() {
    // The SQL query uses LIMIT 3 (confirmed in load_previous_hashes),
    // but populating the previous_hashes table requires app-level logic
    // that is not yet implemented in the store layer.
    // When implemented, insert 5 previous hashes via raw SQL and verify
    // that get_previous_hashes returns only the 3 most recent.
}

#[test]
#[ignore = "BUG: previous hash tracking requires app-level update logic"]
/// Previous hashes should be ordered from most recent to oldest.
fn test_previous_hashes_ordering() {
    // The SQL query orders by created_at DESC (confirmed in load_previous_hashes),
    // but populating the previous_hashes table requires app-level logic.
    // When implemented, insert hashes with known timestamps and verify ordering.
}

#[test]
/// A newly created node should have an empty previous_hashes list.
fn test_new_node_has_no_previous_hashes() {
    // GIVEN a fresh store with a newly inserted node
    let mut store = SqliteGraphStore::in_memory().expect("in-memory store");
    let node = make_node(1, "freshHash001", "brand_new_fn", NodeKind::Function);
    store
        .update_nodes(vec![NodeChange::Add(node)])
        .expect("insert should succeed");

    // WHEN previous_hashes is queried for the new node
    let prev = store.get_previous_hashes(1);

    // THEN the list is empty
    assert!(
        prev.is_empty(),
        "newly created node should have no previous hashes, got {:?}",
        prev
    );
}

#[test]
#[ignore = "BUG: get_node doesn't search previous_hashes"]
/// Looking up a node by a previous hash should still find the current node.
fn test_lookup_by_previous_hash() {
    // The get_node method only searches by current hash (WHERE hash = ?1).
    // It does not fall back to the previous_hashes table.
    // When implemented, this test should:
    // 1. Insert a node with hash "old_hash_val"
    // 2. Update the node to hash "new_hash_val", storing "old_hash_val" as previous
    // 3. Call get_node("old_hash_val") and expect to find the node with "new_hash_val"
}
