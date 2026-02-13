// Tests for hash collision detection and disambiguation (Spec 000 - Graph Schema)

use keel_core::hash::{compute_hash, compute_hash_disambiguated};
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{GraphError, GraphNode, NodeChange, NodeKind};

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
/// When two different functions produce the same hash, collision should be detected.
fn test_collision_detected_on_duplicate_hash() {
    // GIVEN two functions with different names that share the same hash
    let mut store = SqliteGraphStore::in_memory().expect("in-memory store");
    let colliding_hash = "abcDEF12345";
    let node_a = make_node(1, colliding_hash, "func_alpha", NodeKind::Function);
    let node_b = make_node(2, colliding_hash, "func_beta", NodeKind::Function);

    // WHEN the first node is inserted (should succeed)
    let result_a = store.update_nodes(vec![NodeChange::Add(node_a)]);
    assert!(result_a.is_ok(), "First insert should succeed");

    // THEN inserting the second node with the same hash but different name should error
    let result_b = store.update_nodes(vec![NodeChange::Add(node_b)]);
    assert!(result_b.is_err(), "Second insert with colliding hash should error");

    match result_b.unwrap_err() {
        GraphError::HashCollision { hash, .. } => {
            assert_eq!(hash, colliding_hash);
        }
        other => panic!("Expected HashCollision, got: {:?}", other),
    }
}

#[test]
/// Disambiguated hash should differ from regular hash and still be 11-char base62.
fn test_disambiguated_hash_generation() {
    // GIVEN the same signature, body, and docstring
    let sig = "fn collider()";
    let body = "return 42";
    let doc = "";

    // WHEN compute_hash and compute_hash_disambiguated are called
    let regular_hash = compute_hash(sig, body, doc);
    let disambiguated = compute_hash_disambiguated(sig, body, doc, "src/module_a.rs");

    // THEN the disambiguated hash differs from the regular one
    assert_ne!(
        regular_hash, disambiguated,
        "disambiguated hash must differ from regular hash"
    );

    // AND both are 11-char base62
    assert_eq!(disambiguated.len(), 11, "disambiguated hash must be 11 chars");
    assert!(
        disambiguated.chars().all(|c| c.is_ascii_alphanumeric()),
        "disambiguated hash must be base62, got {:?}",
        disambiguated
    );

    // AND different file paths produce different disambiguated hashes
    let disambiguated_b = compute_hash_disambiguated(sig, body, doc, "src/module_b.rs");
    assert_ne!(
        disambiguated, disambiguated_b,
        "different file paths should produce different disambiguated hashes"
    );
}

#[test]
/// Collision reporting should include both conflicting node names and the hash.
fn test_collision_report_includes_both_nodes() {
    // GIVEN a store with an existing node
    let mut store = SqliteGraphStore::in_memory().expect("in-memory store");
    let hash = "XYZ98765432";
    let node_existing = make_node(1, hash, "existing_func", NodeKind::Function);
    store
        .update_nodes(vec![NodeChange::Add(node_existing)])
        .expect("first insert succeeds");

    // WHEN a second node with the same hash but different name is inserted
    let node_new = make_node(2, hash, "new_func", NodeKind::Function);
    let err = store
        .update_nodes(vec![NodeChange::Add(node_new)])
        .unwrap_err();

    // THEN the error includes hash, existing function name, and new function name
    match err {
        GraphError::HashCollision {
            hash: reported_hash,
            existing,
            new_fn,
        } => {
            assert_eq!(reported_hash, hash, "reported hash must match");
            assert_eq!(existing, "existing_func", "existing name must match");
            assert_eq!(new_fn, "new_func", "new function name must match");
        }
        other => panic!("Expected HashCollision, got: {:?}", other),
    }
}

#[test]
/// Insert first node OK, second with same hash but different name should error.
fn test_multiple_collisions_on_same_hash() {
    // GIVEN a store
    let mut store = SqliteGraphStore::in_memory().expect("in-memory store");
    let hash = "COLLIDEhash";

    // WHEN the first node is inserted
    let node1 = make_node(1, hash, "first_fn", NodeKind::Function);
    let r1 = store.update_nodes(vec![NodeChange::Add(node1)]);
    assert!(r1.is_ok(), "First insert should succeed");

    // THEN the second node with same hash but different name triggers HashCollision
    let node2 = make_node(2, hash, "second_fn", NodeKind::Function);
    let r2 = store.update_nodes(vec![NodeChange::Add(node2)]);
    assert!(r2.is_err(), "Second insert should fail with collision");

    match r2.unwrap_err() {
        GraphError::HashCollision {
            hash: h,
            existing,
            new_fn,
        } => {
            assert_eq!(h, hash);
            assert_eq!(existing, "first_fn");
            assert_eq!(new_fn, "second_fn");
        }
        other => panic!("Expected HashCollision, got: {:?}", other),
    }
}

#[test]
/// Insert 100 nodes with unique hashes, no errors should occur.
fn test_no_false_collision_on_unique_hashes() {
    // GIVEN a store and 100 nodes with unique hashes computed from distinct signatures
    let mut store = SqliteGraphStore::in_memory().expect("in-memory store");

    let mut changes = Vec::with_capacity(100);
    for i in 0..100u64 {
        let sig = format!("fn unique_func_{i}(x: i32) -> i32");
        let body = format!("x + {i}");
        let hash = compute_hash(&sig, &body, "");
        let node = make_node(i + 1, &hash, &format!("unique_func_{i}"), NodeKind::Function);
        changes.push(NodeChange::Add(node));
    }

    // WHEN all nodes are inserted
    let result = store.update_nodes(changes);

    // THEN no errors occur
    assert!(
        result.is_ok(),
        "Inserting 100 unique-hash nodes should succeed, got: {:?}",
        result.err()
    );

    // AND all 100 nodes can be retrieved
    for i in 0..100u64 {
        let sig = format!("fn unique_func_{i}(x: i32) -> i32");
        let body = format!("x + {i}");
        let hash = compute_hash(&sig, &body, "");
        let node = store.get_node(&hash);
        assert!(
            node.is_some(),
            "Node with hash {} (index {}) should exist",
            hash,
            i
        );
    }
}
