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
        package: None,
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
        confidence: 1.0,
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
        confidence: 1.0, file_path: "src/test.rs".to_string(), line: 5,
    };
    store.update_edges(vec![EdgeChange::Add(edge.clone())]).unwrap();
    store
        .update_edges(vec![EdgeChange::Add(edge)])
        .expect("Re-adding same edge should not fail with UNIQUE constraint");
    assert_eq!(store.get_edges(1, EdgeDirection::Outgoing).len(), 1);
}

#[test]
fn test_circuit_breaker_save_and_load() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let state = vec![
        ("E001".to_string(), "abc123".to_string(), 2u32, false),
        ("E002".to_string(), "def456".to_string(), 3u32, true),
    ];
    store.save_circuit_breaker(&state).unwrap();

    let loaded = store.load_circuit_breaker().unwrap();
    assert_eq!(loaded.len(), 2);
    let mut sorted = loaded;
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(sorted[0], ("E001".to_string(), "abc123".to_string(), 2, false));
    assert_eq!(sorted[1], ("E002".to_string(), "def456".to_string(), 3, true));

    // Save again — should fully replace
    store.save_circuit_breaker(&[("E003".to_string(), "ghi789".to_string(), 1, false)]).unwrap();
    let reloaded = store.load_circuit_breaker().unwrap();
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].0, "E003");
}

#[test]
fn test_circuit_breaker_empty_roundtrip() {
    let store = SqliteGraphStore::in_memory().unwrap();
    assert!(store.load_circuit_breaker().unwrap().is_empty());
    store.save_circuit_breaker(&[]).unwrap();
    assert!(store.load_circuit_breaker().unwrap().is_empty());
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

#[test]
fn test_batch_loaded_nodes_match_individual() {
    use crate::types::ExternalEndpoint;

    let store = SqliteGraphStore::in_memory().unwrap();

    // Insert 3 nodes with endpoints and previous hashes
    let mut n1 = test_node(1, "batch_aaa1234", "handler_a");
    n1.file_path = "src/handlers.rs".to_string();
    n1.external_endpoints = vec![ExternalEndpoint {
        kind: "http".to_string(),
        method: "GET".to_string(),
        path: "/api/a".to_string(),
        direction: "serves".to_string(),
    }];
    n1.previous_hashes = vec!["old_hash_a1".to_string(), "old_hash_a2".to_string()];

    let mut n2 = test_node(2, "batch_bbb1234", "handler_b");
    n2.file_path = "src/handlers.rs".to_string();
    n2.external_endpoints = vec![
        ExternalEndpoint {
            kind: "http".to_string(),
            method: "POST".to_string(),
            path: "/api/b".to_string(),
            direction: "serves".to_string(),
        },
        ExternalEndpoint {
            kind: "grpc".to_string(),
            method: "".to_string(),
            path: "svc.DoB".to_string(),
            direction: "calls".to_string(),
        },
    ];

    let mut n3 = test_node(3, "batch_ccc1234", "handler_c");
    n3.file_path = "src/handlers.rs".to_string();
    // n3 has no endpoints or previous hashes (tests empty case)

    store.insert_node(&n1).unwrap();
    store.insert_node(&n2).unwrap();
    store.insert_node(&n3).unwrap();

    // Load individually (the old N+1 path)
    let ind1 = store.node_with_relations(
        store.conn.prepare("SELECT * FROM nodes WHERE id = 1").unwrap()
            .query_row([], SqliteGraphStore::row_to_node).unwrap(),
    );
    let ind2 = store.node_with_relations(
        store.conn.prepare("SELECT * FROM nodes WHERE id = 2").unwrap()
            .query_row([], SqliteGraphStore::row_to_node).unwrap(),
    );
    let ind3 = store.node_with_relations(
        store.conn.prepare("SELECT * FROM nodes WHERE id = 3").unwrap()
            .query_row([], SqliteGraphStore::row_to_node).unwrap(),
    );

    // Load via batch (the new optimized path)
    let batch = store.get_nodes_in_file("src/handlers.rs");
    assert_eq!(batch.len(), 3, "batch should return all 3 nodes");

    // Compare each node's relations
    for ind in &[&ind1, &ind2, &ind3] {
        let batch_node = batch.iter().find(|b| b.id == ind.id)
            .unwrap_or_else(|| panic!("batch missing node {}", ind.id));
        assert_eq!(
            batch_node.external_endpoints.len(),
            ind.external_endpoints.len(),
            "endpoint count mismatch for node {}",
            ind.id
        );
        for (be, ie) in batch_node.external_endpoints.iter()
            .zip(ind.external_endpoints.iter())
        {
            assert_eq!(be.kind, ie.kind);
            assert_eq!(be.method, ie.method);
            assert_eq!(be.path, ie.path);
            assert_eq!(be.direction, ie.direction);
        }
        assert_eq!(
            batch_node.previous_hashes, ind.previous_hashes,
            "previous_hashes mismatch for node {}",
            ind.id
        );
    }
}

#[test]
fn test_find_nodes_by_name_empty_kind_wildcard() {
    let mut store = SqliteGraphStore::in_memory().unwrap();

    // Insert a function and a class with the same name
    let mut func_node = test_node(1, "fname_func_01", "get_data");
    func_node.file_path = "src/api.rs".to_string();
    let mut class_node = test_node(2, "fname_class_01", "get_data");
    class_node.kind = NodeKind::Class;
    class_node.file_path = "src/models.rs".to_string();
    store
        .update_nodes(vec![NodeChange::Add(func_node), NodeChange::Add(class_node)])
        .unwrap();

    // Empty kind + empty exclude_file: should find both
    let all = store.find_nodes_by_name("get_data", "", "");
    assert_eq!(all.len(), 2, "empty kind should match all node kinds");

    // Specific kind: should find only the function
    let funcs = store.find_nodes_by_name("get_data", "function", "");
    assert_eq!(funcs.len(), 1);
    assert_eq!(funcs[0].kind, NodeKind::Function);

    // Specific kind: should find only the class
    let classes = store.find_nodes_by_name("get_data", "class", "");
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].kind, NodeKind::Class);

    // Empty kind + exclude_file: should exclude one file
    let excluded = store.find_nodes_by_name("get_data", "", "src/api.rs");
    assert_eq!(excluded.len(), 1);
    assert_eq!(excluded[0].file_path, "src/models.rs");

    // Full filter: kind + exclude_file
    let full = store.find_nodes_by_name("get_data", "function", "src/api.rs");
    assert_eq!(full.len(), 0, "function in api.rs should be excluded");

    // Non-existent name
    let none = store.find_nodes_by_name("nonexistent", "", "");
    assert!(none.is_empty());
}
