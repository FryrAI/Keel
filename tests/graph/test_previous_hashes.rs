// Tests for previous hash tracking on nodes (Spec 000 - Graph Schema)

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{GraphNode, NodeChange, NodeKind};
use rusqlite::params;

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
/// update_nodes(NodeChange::Update) does NOT automatically track the old hash
/// in the previous_hashes table. Previous hash tracking requires explicit
/// application-level logic to insert old hashes before updating.
fn test_previous_hash_not_auto_tracked_on_update() {
    let mut store = SqliteGraphStore::in_memory().expect("in-memory store");
    let node = make_node(1, "old_hash_001", "my_func", NodeKind::Function);
    store
        .update_nodes(vec![NodeChange::Add(node)])
        .expect("insert");

    // Update the node with a new hash
    let mut updated = make_node(1, "new_hash_001", "my_func", NodeKind::Function);
    updated.line_end = 20;
    store
        .update_nodes(vec![NodeChange::Update(updated)])
        .expect("update");

    // Verify: the old hash is NOT automatically saved to previous_hashes
    let prev = store.get_previous_hashes(1);
    assert!(
        prev.is_empty(),
        "update_nodes does not auto-track previous hashes; got {:?}",
        prev
    );
}

#[test]
/// Previous hashes list should be limited to 3 entries (most recent).
/// Verified via raw SQL insertion + public get_previous_hashes API.
fn test_previous_hashes_limited_to_three() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("prev_hash.db");
    let db_str = db_path.to_str().unwrap();

    // Create store and insert a node
    let mut store = SqliteGraphStore::open(db_str).unwrap();
    let node = make_node(1, "current_hash", "func_a", NodeKind::Function);
    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();
    drop(store);

    // Insert 5 previous hashes via raw SQL
    {
        let conn = rusqlite::Connection::open(db_str).unwrap();
        for i in 1..=5u32 {
            conn.execute(
                "INSERT INTO previous_hashes (node_id, hash, created_at) VALUES (?1, ?2, datetime('now', ?3))",
                params![1i64, format!("prev_hash_{i}"), format!("-{} seconds", 10 * (5 - i))],
            )
            .unwrap();
        }
    }

    // Re-open store and verify the LIMIT 3 behavior
    let store = SqliteGraphStore::open(db_str).unwrap();
    let prev = store.get_previous_hashes(1);
    assert_eq!(
        prev.len(),
        3,
        "get_previous_hashes should return at most 3, got {} ({:?})",
        prev.len(),
        prev
    );
}

#[test]
/// Previous hashes should be ordered from most recent to oldest.
/// The SQL query uses ORDER BY created_at DESC.
fn test_previous_hashes_ordering() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("prev_order.db");
    let db_str = db_path.to_str().unwrap();

    // Create store and insert a node
    let mut store = SqliteGraphStore::open(db_str).unwrap();
    let node = make_node(1, "current", "func_b", NodeKind::Function);
    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();
    drop(store);

    // Insert previous hashes with explicit timestamps (oldest to newest)
    {
        let conn = rusqlite::Connection::open(db_str).unwrap();
        conn.execute(
            "INSERT INTO previous_hashes (node_id, hash, created_at) VALUES (1, 'oldest', '2025-01-01 00:00:00')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO previous_hashes (node_id, hash, created_at) VALUES (1, 'middle', '2025-06-15 00:00:00')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO previous_hashes (node_id, hash, created_at) VALUES (1, 'newest', '2026-01-01 00:00:00')",
            [],
        ).unwrap();
    }

    // Re-open and verify ordering (most recent first)
    let store = SqliteGraphStore::open(db_str).unwrap();
    let prev = store.get_previous_hashes(1);
    assert_eq!(prev.len(), 3);
    assert_eq!(prev[0], "newest", "first entry should be most recent");
    assert_eq!(prev[1], "middle", "second entry should be middle");
    assert_eq!(prev[2], "oldest", "third entry should be oldest");
}

#[test]
/// A newly created node should have an empty previous_hashes list.
fn test_new_node_has_no_previous_hashes() {
    let mut store = SqliteGraphStore::in_memory().expect("in-memory store");
    let node = make_node(1, "freshHash001", "brand_new_fn", NodeKind::Function);
    store
        .update_nodes(vec![NodeChange::Add(node)])
        .expect("insert should succeed");

    let prev = store.get_previous_hashes(1);
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
