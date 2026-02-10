use super::*;
use crate::store::GraphStore;
use crate::types::{EdgeChange, EdgeDirection, EdgeKind, GraphEdge, NodeChange, NodeKind};

fn test_node(id: u64, hash: &str, name: &str) -> GraphNode {
    GraphNode {
        id,
        hash: hash.to_string(),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: format!("fn {}()", name),
        file_path: "src/test.rs".to_string(),
        line_start: 1,
        line_end: 10,
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
fn test_create_and_read_node() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(1, "abc12345678", "test_fn");
    store.update_nodes(vec![NodeChange::Add(node.clone())]).unwrap();

    let retrieved = store.get_node("abc12345678").unwrap();
    assert_eq!(retrieved.name, "test_fn");
    assert_eq!(retrieved.hash, "abc12345678");
}

#[test]
fn test_get_node_by_id() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(42, "def12345678", "lookup_fn");
    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();

    let retrieved = store.get_node_by_id(42).unwrap();
    assert_eq!(retrieved.name, "lookup_fn");
}

#[test]
fn test_update_node() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(1, "abc12345678", "old_name");
    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();

    let mut updated = test_node(1, "xyz12345678", "new_name");
    updated.signature = "fn new_name() -> i32".to_string();
    store.update_nodes(vec![NodeChange::Update(updated)]).unwrap();

    let retrieved = store.get_node_by_id(1).unwrap();
    assert_eq!(retrieved.name, "new_name");
    assert_eq!(retrieved.hash, "xyz12345678");
}

#[test]
fn test_remove_node() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(1, "abc12345678", "doomed_fn");
    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();
    store.update_nodes(vec![NodeChange::Remove(1)]).unwrap();

    assert!(store.get_node_by_id(1).is_none());
}

#[test]
fn test_edges() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let n1 = test_node(1, "aaa12345678", "caller");
    let n2 = test_node(2, "bbb12345678", "callee");
    store.update_nodes(vec![NodeChange::Add(n1), NodeChange::Add(n2)]).unwrap();

    let edge = GraphEdge {
        id: 1,
        source_id: 1,
        target_id: 2,
        kind: EdgeKind::Calls,
        file_path: "src/test.rs".to_string(),
        line: 5,
    };
    store.update_edges(vec![EdgeChange::Add(edge)]).unwrap();

    let outgoing = store.get_edges(1, EdgeDirection::Outgoing);
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].target_id, 2);

    let incoming = store.get_edges(2, EdgeDirection::Incoming);
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].source_id, 1);
}

#[test]
fn test_schema_version() {
    let store = SqliteGraphStore::in_memory().unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
}

#[test]
fn test_readd_same_node_no_unique_constraint_error() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = test_node(1, "abc12345678", "test_fn");
    store.update_nodes(vec![NodeChange::Add(node.clone())]).unwrap();
    store
        .update_nodes(vec![NodeChange::Add(node)])
        .expect("Re-adding same node should not fail with UNIQUE constraint");
    let retrieved = store.get_node("abc12345678").unwrap();
    assert_eq!(retrieved.name, "test_fn");
}

#[test]
fn test_readd_same_edge_no_unique_constraint_error() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let n1 = test_node(1, "aaa12345678", "caller");
    let n2 = test_node(2, "bbb12345678", "callee");
    store.update_nodes(vec![NodeChange::Add(n1), NodeChange::Add(n2)]).unwrap();
    let edge = GraphEdge {
        id: 1, source_id: 1, target_id: 2, kind: EdgeKind::Calls,
        file_path: "src/test.rs".to_string(), line: 5,
    };
    store.update_edges(vec![EdgeChange::Add(edge.clone())]).unwrap();
    store
        .update_edges(vec![EdgeChange::Add(edge)])
        .expect("Re-adding same edge should not fail with UNIQUE constraint");
    assert_eq!(store.get_edges(1, EdgeDirection::Outgoing).len(), 1);
}

#[test]
fn test_hash_collision_different_names_still_errors() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node1 = test_node(1, "collision_hash", "func_a");
    store.update_nodes(vec![NodeChange::Add(node1)]).unwrap();
    let node2 = test_node(2, "collision_hash", "func_b");
    assert!(
        store.update_nodes(vec![NodeChange::Add(node2)]).is_err(),
        "Hash collision between different functions should still error"
    );
}
