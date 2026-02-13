// Tests for SqliteGraphStore CRUD operations (Spec 000 - Graph Schema)

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{
    EdgeChange, EdgeDirection, EdgeKind, GraphEdge, GraphError, GraphNode, NodeChange, NodeKind,
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
    }
}

fn make_edge(id: u64, src: u64, tgt: u64, kind: EdgeKind) -> GraphEdge {
    GraphEdge {
        id,
        source_id: src,
        target_id: tgt,
        kind,
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

#[test]
#[ignore = "BUG: ModuleProfile insert not exposed via GraphStore trait"]
/// Storing and retrieving a ModuleProfile should preserve all profile data.
/// The module_profiles table exists but there is no public insert method.
/// get_module_profile reads from the table, but no save_module_profile is
/// exposed on SqliteGraphStore or GraphStore.
fn test_sqlite_module_profile_storage() {
    // Cannot test without a public insert API for module_profiles.
    // store.conn is pub(crate), inaccessible from integration tests.
    unreachable!("No public API to insert ModuleProfile");
}

#[test]
#[ignore = "BUG: resolution_cache insert/read not exposed via public API"]
/// The resolution cache should store and retrieve cached resolution results.
/// The resolution_cache table exists in the schema but no public methods
/// are exposed to insert or query it from outside the crate.
fn test_sqlite_resolution_cache() {
    unreachable!("No public API for resolution_cache");
}

#[test]
/// The circuit breaker state should be stored and retrieved.
fn test_sqlite_circuit_breaker_state() {
    let store = SqliteGraphStore::in_memory().unwrap();

    let state = vec![
        ("E001".to_string(), "hash_abc".to_string(), 2u32, false),
        ("E002".to_string(), "hash_def".to_string(), 3u32, true),
    ];
    store.save_circuit_breaker(&state).unwrap();

    let loaded = store.load_circuit_breaker().unwrap();
    assert_eq!(loaded.len(), 2);

    let first = loaded.iter().find(|r| r.0 == "E001").unwrap();
    assert_eq!(first.1, "hash_abc");
    assert_eq!(first.2, 2);
    assert!(!first.3, "should not be downgraded");

    let second = loaded.iter().find(|r| r.0 == "E002").unwrap();
    assert_eq!(second.1, "hash_def");
    assert_eq!(second.2, 3);
    assert!(second.3, "should be downgraded");
}

#[test]
/// Bulk insertion with a duplicate hash (different name) should fail atomically.
fn test_sqlite_bulk_insert_atomicity() {
    let mut store = SqliteGraphStore::in_memory().unwrap();

    // Seed a node so the collision target exists
    let existing = make_node(1, "dup_hash", "original_fn", NodeKind::Function);
    store.update_nodes(vec![NodeChange::Add(existing)]).unwrap();

    // Batch with a hash collision: same hash "dup_hash", different name
    let batch = vec![
        NodeChange::Add(make_node(2, "unique_a", "fn_a", NodeKind::Function)),
        NodeChange::Add(make_node(3, "dup_hash", "collider_fn", NodeKind::Function)),
        NodeChange::Add(make_node(4, "unique_b", "fn_b", NodeKind::Function)),
    ];

    let result = store.update_nodes(batch);
    assert!(
        result.is_err(),
        "batch with hash collision should fail: {:?}",
        result
    );
    if let Err(GraphError::HashCollision { hash, .. }) = &result {
        assert_eq!(hash, "dup_hash");
    } else {
        panic!("expected HashCollision error, got: {:?}", result);
    }

    // None of the batch nodes should have been persisted (transaction rolled back)
    assert!(
        store.get_node("unique_a").is_none(),
        "transaction should have rolled back node unique_a"
    );
    assert!(
        store.get_node("unique_b").is_none(),
        "transaction should have rolled back node unique_b"
    );
    // Original node should still be intact
    assert!(store.get_node("dup_hash").is_some());
}

#[test]
/// SQLite store should handle concurrent reads without corruption.
fn test_sqlite_concurrent_reads() {
    // SQLite in-memory databases are per-connection, so we use a temp file
    // that multiple connections can open for concurrent reads.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("concurrent.db");
    let db_str = db_path.to_str().unwrap();

    // Populate the store
    let mut store = SqliteGraphStore::open(db_str).unwrap();
    let mut nodes = Vec::new();
    for i in 1..=10u64 {
        nodes.push(NodeChange::Add(make_node(
            i,
            &format!("conc_hash_{i}"),
            &format!("fn_{i}"),
            NodeKind::Function,
        )));
    }
    store.update_nodes(nodes).unwrap();
    drop(store);

    // Spawn readers each with their own connection
    let path_owned = db_str.to_string();
    let handles: Vec<_> = (1..=4)
        .map(|thread_id| {
            let p = path_owned.clone();
            std::thread::spawn(move || {
                let reader = SqliteGraphStore::open(&p).unwrap();
                for i in 1..=10u64 {
                    let node = reader.get_node(&format!("conc_hash_{i}"));
                    assert!(
                        node.is_some(),
                        "thread {thread_id} failed to read node {i}"
                    );
                    assert_eq!(node.unwrap().name, format!("fn_{i}"));
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("reader thread panicked");
    }
}

#[test]
/// Opening a SQLite store on a new database should auto-create the schema.
fn test_sqlite_auto_create_schema() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("fresh.db");
    let db_str = db_path.to_str().unwrap();

    assert!(!db_path.exists(), "db should not exist yet");

    let store = SqliteGraphStore::open(db_str).unwrap();
    assert!(db_path.exists(), "db file should have been created");

    let version = store.schema_version().unwrap();
    assert_eq!(version, 1, "schema version should be 1");

    // Verify we can perform basic operations on the fresh schema
    let modules = store.get_all_modules();
    assert!(modules.is_empty(), "fresh db should have no modules");
}
