// Tests for previous hash tracking on nodes (Spec 000 - Graph Schema)
//
// use keel_core::types::GraphNode;
// use keel_core::hash::compute_hash;

#[test]
#[ignore = "Not yet implemented"]
/// When a node's hash changes, the old hash should be stored in previous_hashes.
fn test_previous_hash_stored_on_change() {
    // GIVEN a node with hash "abc12345678"
    // WHEN the node's body changes and a new hash "xyz98765432" is computed
    // THEN previous_hashes contains "abc12345678"
}

#[test]
#[ignore = "Not yet implemented"]
/// Previous hashes list should be limited to 3 entries (most recent).
fn test_previous_hashes_limited_to_three() {
    // GIVEN a node that has changed 5 times
    // WHEN previous_hashes is queried
    // THEN only the 3 most recent previous hashes are stored
}

#[test]
#[ignore = "Not yet implemented"]
/// Previous hashes should be ordered from most recent to oldest.
fn test_previous_hashes_ordering() {
    // GIVEN a node with 3 previous hashes
    // WHEN previous_hashes is queried
    // THEN hashes are ordered [most_recent, middle, oldest]
}

#[test]
#[ignore = "Not yet implemented"]
/// A newly created node should have an empty previous_hashes list.
fn test_new_node_has_no_previous_hashes() {
    // GIVEN a freshly created node
    // WHEN previous_hashes is queried
    // THEN the list is empty
}

#[test]
#[ignore = "Not yet implemented"]
/// Looking up a node by a previous hash should still find the current node.
fn test_lookup_by_previous_hash() {
    // GIVEN a node whose hash changed from "old_hash_val" to "new_hash_val"
    // WHEN querying the graph store with "old_hash_val"
    // THEN the current node (with hash "new_hash_val") is returned
}
