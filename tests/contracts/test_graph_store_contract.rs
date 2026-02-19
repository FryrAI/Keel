/// Contract tests for the GraphStore trait via SqliteGraphStore.
///
/// These tests verify that SqliteGraphStore correctly implements all
/// GraphStore trait methods. They are NOT ignored because the SQLite
/// implementation already exists.
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{
    EdgeChange, EdgeDirection, EdgeKind, GraphEdge, GraphError, GraphNode, NodeChange, NodeKind,
};

/// Helper to create a minimal test node.
fn test_node(id: u64, hash: &str, name: &str, kind: NodeKind, module_id: u64) -> GraphNode {
    GraphNode {
        id,
        hash: hash.to_string(),
        kind,
        name: name.to_string(),
        signature: format!("fn {}()", name),
        file_path: "src/contract_test.rs".to_string(),
        line_start: 1,
        line_end: 10,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id,
        package: None,
    }
}

fn test_edge(id: u64, source: u64, target: u64, kind: EdgeKind) -> GraphEdge {
    GraphEdge {
        id,
        source_id: source,
        target_id: target,
        kind,
        file_path: "src/contract_test.rs".to_string(),
        line: 5,
        confidence: 1.0,
    }
}

// ---------------------------------------------------------------------------
// get_node / get_node_by_id
// ---------------------------------------------------------------------------

#[test]
fn contract_get_node_by_hash() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(1, "hash_aaa0001", "my_func", NodeKind::Function, 0);
    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();

    let found = store.get_node("hash_aaa0001");
    assert!(found.is_some(), "get_node should find node by hash");
    assert_eq!(found.unwrap().name, "my_func");
}

#[test]
fn contract_get_node_by_hash_missing() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let found = store.get_node("nonexistent_hash");
    assert!(
        found.is_none(),
        "get_node should return None for missing hash"
    );
}

#[test]
fn contract_get_node_by_id() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(42, "hash_bbb0042", "lookup", NodeKind::Function, 0);
    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();

    let found = store.get_node_by_id(42);
    assert!(found.is_some());
    assert_eq!(found.unwrap().hash, "hash_bbb0042");
}

#[test]
fn contract_get_node_by_id_missing() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let found = store.get_node_by_id(99999);
    assert!(found.is_none());
}

// ---------------------------------------------------------------------------
// get_edges
// ---------------------------------------------------------------------------

#[test]
fn contract_get_edges_outgoing() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let n1 = test_node(1, "hash_caller01", "caller", NodeKind::Function, 0);
    let n2 = test_node(2, "hash_callee01", "callee", NodeKind::Function, 0);
    store
        .update_nodes(vec![NodeChange::Add(n1), NodeChange::Add(n2)])
        .unwrap();
    store
        .update_edges(vec![EdgeChange::Add(test_edge(1, 1, 2, EdgeKind::Calls))])
        .unwrap();

    let edges = store.get_edges(1, EdgeDirection::Outgoing);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].target_id, 2);
}

#[test]
fn contract_get_edges_incoming() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let n1 = test_node(1, "hash_src00001", "source", NodeKind::Function, 0);
    let n2 = test_node(2, "hash_tgt00001", "target", NodeKind::Function, 0);
    store
        .update_nodes(vec![NodeChange::Add(n1), NodeChange::Add(n2)])
        .unwrap();
    store
        .update_edges(vec![EdgeChange::Add(test_edge(1, 1, 2, EdgeKind::Calls))])
        .unwrap();

    let edges = store.get_edges(2, EdgeDirection::Incoming);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].source_id, 1);
}

#[test]
fn contract_get_edges_both() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let n1 = test_node(1, "hash_a_both01", "a", NodeKind::Function, 0);
    let n2 = test_node(2, "hash_b_both01", "b", NodeKind::Function, 0);
    let n3 = test_node(3, "hash_c_both01", "c", NodeKind::Function, 0);
    store
        .update_nodes(vec![
            NodeChange::Add(n1),
            NodeChange::Add(n2),
            NodeChange::Add(n3),
        ])
        .unwrap();
    store
        .update_edges(vec![
            EdgeChange::Add(test_edge(1, 1, 2, EdgeKind::Calls)),
            EdgeChange::Add(test_edge(2, 2, 3, EdgeKind::Calls)),
        ])
        .unwrap();

    let edges = store.get_edges(2, EdgeDirection::Both);
    assert_eq!(
        edges.len(),
        2,
        "Node 2 should have 1 incoming + 1 outgoing edge"
    );
}

#[test]
fn contract_get_edges_empty() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let n = test_node(1, "hash_lonely01", "lonely", NodeKind::Function, 0);
    store.update_nodes(vec![NodeChange::Add(n)]).unwrap();

    let edges = store.get_edges(1, EdgeDirection::Both);
    assert!(edges.is_empty());
}

// ---------------------------------------------------------------------------
// get_nodes_in_file
// ---------------------------------------------------------------------------

#[test]
fn contract_get_nodes_in_file() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let n1 = test_node(1, "hash_file_a01", "func_a", NodeKind::Function, 0);
    let mut n2 = test_node(2, "hash_file_b01", "func_b", NodeKind::Function, 0);
    n2.file_path = "src/other.rs".to_string();
    store
        .update_nodes(vec![NodeChange::Add(n1), NodeChange::Add(n2)])
        .unwrap();

    let nodes = store.get_nodes_in_file("src/contract_test.rs");
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].name, "func_a");
}

// ---------------------------------------------------------------------------
// get_all_modules
// ---------------------------------------------------------------------------

#[test]
fn contract_get_all_modules() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let m1 = test_node(100, "hash_mod_a001", "mod_a", NodeKind::Module, 0);
    let m2 = test_node(101, "hash_mod_b001", "mod_b", NodeKind::Module, 0);
    let f1 = test_node(1, "hash_func_001", "func_a", NodeKind::Function, 100);
    store
        .update_nodes(vec![
            NodeChange::Add(m1),
            NodeChange::Add(m2),
            NodeChange::Add(f1),
        ])
        .unwrap();

    let modules = store.get_all_modules();
    assert_eq!(modules.len(), 2, "Should return only module-kind nodes");
}

// ---------------------------------------------------------------------------
// update_nodes (Add, Update, Remove)
// ---------------------------------------------------------------------------

#[test]
fn contract_update_nodes_add() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(1, "hash_add_0001", "new_func", NodeKind::Function, 0);
    let result = store.update_nodes(vec![NodeChange::Add(node)]);
    assert!(result.is_ok());
    assert!(store.get_node("hash_add_0001").is_some());
}

#[test]
fn contract_update_nodes_update() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(1, "hash_upd_0001", "original", NodeKind::Function, 0);
    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();

    let mut updated = test_node(1, "hash_upd_0002", "renamed", NodeKind::Function, 0);
    updated.signature = "fn renamed() -> bool".to_string();
    store
        .update_nodes(vec![NodeChange::Update(updated)])
        .unwrap();

    let found = store.get_node_by_id(1).unwrap();
    assert_eq!(found.name, "renamed");
    assert_eq!(found.hash, "hash_upd_0002");
}

#[test]
fn contract_update_nodes_remove() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(1, "hash_rm_00001", "doomed", NodeKind::Function, 0);
    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();
    store.update_nodes(vec![NodeChange::Remove(1)]).unwrap();
    assert!(store.get_node_by_id(1).is_none());
}

#[test]
fn contract_update_nodes_batch() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let nodes = vec![
        NodeChange::Add(test_node(1, "hash_batch_01", "fn_a", NodeKind::Function, 0)),
        NodeChange::Add(test_node(2, "hash_batch_02", "fn_b", NodeKind::Function, 0)),
        NodeChange::Add(test_node(3, "hash_batch_03", "fn_c", NodeKind::Function, 0)),
    ];
    let result = store.update_nodes(nodes);
    assert!(result.is_ok());
    assert!(store.get_node_by_id(1).is_some());
    assert!(store.get_node_by_id(2).is_some());
    assert!(store.get_node_by_id(3).is_some());
}

// ---------------------------------------------------------------------------
// update_edges (Add, Remove)
// ---------------------------------------------------------------------------

#[test]
fn contract_update_edges_add() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let n1 = test_node(1, "hash_ea_00001", "src_fn", NodeKind::Function, 0);
    let n2 = test_node(2, "hash_ea_00002", "tgt_fn", NodeKind::Function, 0);
    store
        .update_nodes(vec![NodeChange::Add(n1), NodeChange::Add(n2)])
        .unwrap();

    let result = store.update_edges(vec![EdgeChange::Add(test_edge(1, 1, 2, EdgeKind::Calls))]);
    assert!(result.is_ok());

    let edges = store.get_edges(1, EdgeDirection::Outgoing);
    assert_eq!(edges.len(), 1);
}

#[test]
fn contract_update_edges_remove() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let n1 = test_node(1, "hash_er_00001", "src_fn", NodeKind::Function, 0);
    let n2 = test_node(2, "hash_er_00002", "tgt_fn", NodeKind::Function, 0);
    store
        .update_nodes(vec![NodeChange::Add(n1), NodeChange::Add(n2)])
        .unwrap();
    store
        .update_edges(vec![EdgeChange::Add(test_edge(1, 1, 2, EdgeKind::Calls))])
        .unwrap();

    store.update_edges(vec![EdgeChange::Remove(1)]).unwrap();
    let edges = store.get_edges(1, EdgeDirection::Outgoing);
    assert!(edges.is_empty());
}

// ---------------------------------------------------------------------------
// get_previous_hashes
// ---------------------------------------------------------------------------

#[test]
fn contract_get_previous_hashes_empty() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(1, "hash_ph_00001", "func", NodeKind::Function, 0);
    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();

    let prev = store.get_previous_hashes(1);
    assert!(prev.is_empty(), "New node should have no previous hashes");
}

// ---------------------------------------------------------------------------
// get_module_profile
// ---------------------------------------------------------------------------

#[test]
fn contract_get_module_profile_missing() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let profile = store.get_module_profile(999);
    assert!(
        profile.is_none(),
        "Module profile should be None for nonexistent module"
    );
}

// ---------------------------------------------------------------------------
// Hash collision detection
// ---------------------------------------------------------------------------

#[test]
fn contract_hash_collision_different_names() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let n1 = test_node(1, "collision_hash", "func_a", NodeKind::Function, 0);
    store.update_nodes(vec![NodeChange::Add(n1)]).unwrap();

    let n2 = test_node(2, "collision_hash", "func_b", NodeKind::Function, 0);
    let result = store.update_nodes(vec![NodeChange::Add(n2)]);
    assert!(
        result.is_err(),
        "Should detect hash collision for different function names"
    );

    match result.unwrap_err() {
        GraphError::HashCollision {
            hash,
            existing,
            new_fn,
        } => {
            assert_eq!(hash, "collision_hash");
            assert_eq!(existing, "func_a");
            assert_eq!(new_fn, "func_b");
        }
        other => panic!("Expected HashCollision error, got: {:?}", other),
    }
}

#[test]
fn contract_hash_collision_same_name_allowed() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let n1 = test_node(1, "same_hash_001", "func_a", NodeKind::Function, 0);
    store.update_nodes(vec![NodeChange::Add(n1)]).unwrap();

    // Same hash + same name = same function re-mapped. Should succeed (INSERT OR REPLACE).
    let n2 = test_node(2, "same_hash_001", "func_a", NodeKind::Function, 0);
    store
        .update_nodes(vec![NodeChange::Add(n2)])
        .expect("Re-adding same function (same hash + name) should succeed on re-map");

    // The node should now exist with the new id
    let node = store.get_node("same_hash_001").unwrap();
    assert_eq!(node.name, "func_a");
}
