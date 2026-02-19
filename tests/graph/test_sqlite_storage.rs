// Tests for SqliteGraphStore CRUD operations (Spec 000 - Graph Schema)

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{
    EdgeChange, EdgeDirection, EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind,
};

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
        package: None,
    }
}

fn make_edge(id: u64, src: u64, tgt: u64, kind: EdgeKind) -> GraphEdge {
    GraphEdge {
        id,
        source_id: src,
        target_id: tgt,
        kind,
        confidence: 1.0,
        file_path: "test.rs".into(),
        line: 1,
    }
}

#[test]
/// Inserting a node into SQLite and reading it back should preserve all fields.
fn test_sqlite_create_and_read_node() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = make_node(1, "hash_abc", "my_func", NodeKind::Function);

    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();

    let read = store.get_node("hash_abc");
    assert!(read.is_some(), "node should be readable after insert");
    let read = read.unwrap();
    assert_eq!(read.id, 1);
    assert_eq!(read.hash, "hash_abc");
    assert_eq!(read.name, "my_func");
    assert_eq!(read.kind, NodeKind::Function);
    assert_eq!(read.signature, "my_func()");
    assert_eq!(read.file_path, "test.rs");
    assert_eq!(read.line_start, 1);
    assert_eq!(read.line_end, 5);
    assert!(read.is_public);
    assert!(read.type_hints_present);
    assert!(!read.has_docstring);
    assert!(read.docstring.is_none());
}

#[test]
/// Updating an existing node should modify the stored data.
fn test_sqlite_update_node() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = make_node(1, "hash_old", "my_func", NodeKind::Function);
    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();

    let mut updated = make_node(1, "hash_new", "my_func", NodeKind::Function);
    updated.line_end = 20;
    store
        .update_nodes(vec![NodeChange::Update(updated)])
        .unwrap();

    // Old hash should no longer resolve
    assert!(store.get_node("hash_old").is_none());
    // New hash should resolve
    let read = store.get_node("hash_new").unwrap();
    assert_eq!(read.hash, "hash_new");
    assert_eq!(read.line_end, 20);
    assert_eq!(read.id, 1);
}

#[test]
/// Deleting a node should remove it from storage.
fn test_sqlite_delete_node() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = make_node(1, "hash_del", "doomed", NodeKind::Function);
    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();

    assert!(store.get_node("hash_del").is_some());

    store.update_nodes(vec![NodeChange::Remove(1)]).unwrap();
    assert!(
        store.get_node("hash_del").is_none(),
        "node should be gone after Remove"
    );
    assert!(store.get_node_by_id(1).is_none());
}

#[test]
/// Inserting an edge and reading it back should preserve source, target, and kind.
fn test_sqlite_create_and_read_edge() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let n1 = make_node(10, "hash_src", "caller", NodeKind::Function);
    let n2 = make_node(20, "hash_tgt", "callee", NodeKind::Function);
    store
        .update_nodes(vec![NodeChange::Add(n1), NodeChange::Add(n2)])
        .unwrap();

    let edge = make_edge(1, 10, 20, EdgeKind::Calls);
    store.update_edges(vec![EdgeChange::Add(edge)]).unwrap();

    let edges = store.get_edges(10, EdgeDirection::Outgoing);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].source_id, 10);
    assert_eq!(edges[0].target_id, 20);
    assert_eq!(edges[0].kind, EdgeKind::Calls);
    assert_eq!(edges[0].file_path, "test.rs");
    assert_eq!(edges[0].line, 1);
}

#[test]
/// Reading edges for a node should distinguish incoming from outgoing.
fn test_sqlite_read_edges_for_node() {
    let mut store = SqliteGraphStore::in_memory().unwrap();

    // Central node (id=1) plus 5 satellites (ids 2..=6)
    let central = make_node(1, "hash_ctr", "central", NodeKind::Function);
    let mut nodes = vec![NodeChange::Add(central)];
    for i in 2..=6u64 {
        nodes.push(NodeChange::Add(make_node(
            i,
            &format!("hash_{i}"),
            &format!("sat_{i}"),
            NodeKind::Function,
        )));
    }
    store.update_nodes(nodes).unwrap();

    // 3 outgoing edges from central
    let mut edges = Vec::new();
    for (eid, tgt) in [(100, 2), (101, 3), (102, 4)] {
        edges.push(EdgeChange::Add(make_edge(eid, 1, tgt, EdgeKind::Calls)));
    }
    // 2 incoming edges to central
    for (eid, src) in [(103, 5), (104, 6)] {
        edges.push(EdgeChange::Add(make_edge(eid, src, 1, EdgeKind::Calls)));
    }
    store.update_edges(edges).unwrap();

    let outgoing = store.get_edges(1, EdgeDirection::Outgoing);
    assert_eq!(outgoing.len(), 3, "should have 3 outgoing edges");
    for e in &outgoing {
        assert_eq!(e.source_id, 1);
    }

    let incoming = store.get_edges(1, EdgeDirection::Incoming);
    assert_eq!(incoming.len(), 2, "should have 2 incoming edges");
    for e in &incoming {
        assert_eq!(e.target_id, 1);
    }

    let both = store.get_edges(1, EdgeDirection::Both);
    assert_eq!(both.len(), 5, "Both direction should return all 5 edges");
}

#[test]
/// Deleting a node should cascade-delete its associated edges.
fn test_sqlite_delete_node_cascades_edges() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let n1 = make_node(1, "hash_a", "func_a", NodeKind::Function);
    let n2 = make_node(2, "hash_b", "func_b", NodeKind::Function);
    let n3 = make_node(3, "hash_c", "func_c", NodeKind::Function);
    store
        .update_nodes(vec![
            NodeChange::Add(n1),
            NodeChange::Add(n2),
            NodeChange::Add(n3),
        ])
        .unwrap();

    store
        .update_edges(vec![
            EdgeChange::Add(make_edge(10, 1, 2, EdgeKind::Calls)),
            EdgeChange::Add(make_edge(11, 3, 1, EdgeKind::Imports)),
        ])
        .unwrap();

    // Verify edges exist
    assert_eq!(store.get_edges(1, EdgeDirection::Both).len(), 2);

    // Delete node 1
    store.update_nodes(vec![NodeChange::Remove(1)]).unwrap();

    // Edges referencing node 1 should be gone
    assert!(
        store.get_edges(1, EdgeDirection::Both).is_empty(),
        "edges should cascade-delete with node"
    );
    // Node 2's outgoing edges from node 1 are gone
    let n2_in = store.get_edges(2, EdgeDirection::Incoming);
    assert!(n2_in.is_empty(), "incoming edge to node 2 should be gone");
    // Node 3's outgoing edge to node 1 is gone
    let n3_out = store.get_edges(3, EdgeDirection::Outgoing);
    assert!(n3_out.is_empty(), "outgoing edge from node 3 should be gone");
}
