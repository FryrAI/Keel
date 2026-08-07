// Tests for hash collision detection and disambiguation (Spec 000 - Graph Schema)

use keel_core::hash::{compute_hash, compute_hash_disambiguated};
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{GraphNode, NodeChange, NodeKind};

fn make_node(id: u64, hash: &str, name: &str, kind: NodeKind) -> GraphNode {
    GraphNode {
        complexity: 0,
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
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

#[test]
/// When two different functions produce the same hash, the collision is
/// resolved by re-salting the second node — never by aborting the persist
/// (#48: one colliding pair must not take down the gate for a whole repo).
fn test_collision_auto_disambiguates_on_duplicate_hash() {
    // GIVEN two functions with different names that share the same hash
    let mut store = SqliteGraphStore::in_memory().expect("in-memory store");
    let colliding_hash = "abcDEF12345";
    let node_a = make_node(1, colliding_hash, "func_alpha", NodeKind::Function);
    let node_b = make_node(2, colliding_hash, "func_beta", NodeKind::Function);

    // WHEN the first node is inserted (should succeed)
    let result_a = store.update_nodes(vec![NodeChange::Add(node_a)]);
    assert!(result_a.is_ok(), "First insert should succeed");

    // THEN inserting the second node with the same hash but different name
    // succeeds under a file-path-salted hash
    store
        .update_nodes(vec![NodeChange::Add(node_b)])
        .expect("collision must not abort the persist");

    assert_eq!(
        store.get_node(colliding_hash).unwrap().name,
        "func_alpha",
        "original node keeps its hash"
    );
    let salted = store.get_node_by_id(2).expect("collider persisted");
    assert_eq!(salted.name, "func_beta");
    assert_ne!(salted.hash, colliding_hash, "collider got a distinct hash");
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
    assert_eq!(
        disambiguated.len(),
        11,
        "disambiguated hash must be 11 chars"
    );
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
/// An Update that tries to claim a hash owned by a DIFFERENT node is re-salted
/// instead of aborting (#48) — the owner keeps its hash.
fn test_update_collision_auto_disambiguates() {
    // GIVEN a store with two nodes under distinct hashes
    let mut store = SqliteGraphStore::in_memory().expect("in-memory store");
    let owned_hash = "XYZ98765432";
    let node_owner = make_node(1, owned_hash, "existing_func", NodeKind::Function);
    let node_other = make_node(2, "other4567890", "new_func", NodeKind::Function);
    store
        .update_nodes(vec![
            NodeChange::Add(node_owner),
            NodeChange::Add(node_other),
        ])
        .expect("seed inserts succeed");

    // WHEN node 2 is updated to claim node 1's hash
    let mut claiming = make_node(2, owned_hash, "new_func", NodeKind::Function);
    claiming.file_path = "other.rs".into();
    store
        .update_nodes(vec![NodeChange::Update(claiming)])
        .expect("update collision must not abort the persist");

    // THEN the owner keeps its hash and node 2 landed on a salted one
    assert_eq!(store.get_node(owned_hash).unwrap().name, "existing_func");
    let salted = store.get_node_by_id(2).expect("updated node persisted");
    assert_ne!(salted.hash, owned_hash);
}

#[test]
/// Repeated collisions against the same hash each get their own salted
/// identity — three same-hash inserts end as three distinct nodes.
fn test_multiple_collisions_on_same_hash() {
    // GIVEN a store
    let mut store = SqliteGraphStore::in_memory().expect("in-memory store");
    let hash = "COLLIDEhash";

    // WHEN three different-named nodes are inserted under the same hash with
    // identical signatures and file paths — the ordinal walk must separate
    // them (a single file-path salt would give nodes 2 and 3 the same hash)
    for (id, name) in [(1, "first_fn"), (2, "second_fn"), (3, "third_fn")] {
        let mut node = make_node(id, hash, name, NodeKind::Function);
        node.signature = "identical_sig()".into();
        store
            .update_nodes(vec![NodeChange::Add(node)])
            .expect("collisions must not abort the persist");
    }

    // THEN all three persisted with pairwise-distinct hashes
    let hashes: Vec<String> = (1..=3)
        .map(|id| store.get_node_by_id(id).expect("node persisted").hash)
        .collect();
    assert_eq!(hashes[0], hash, "first insert keeps the plain hash");
    assert_ne!(hashes[0], hashes[1]);
    assert_ne!(hashes[0], hashes[2]);
    assert_ne!(hashes[1], hashes[2]);
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
        let node = make_node(
            i + 1,
            &hash,
            &format!("unique_func_{i}"),
            NodeKind::Function,
        );
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
