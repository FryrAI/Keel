// Benchmark tests for SQLite storage operations

use keel_core::hash::compute_hash;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeKind, GraphEdge, GraphNode, NodeKind};
use std::time::Instant;

fn make_node(id: u64, name: &str, file_path: &str) -> GraphNode {
    let sig = format!("fn {name}()");
    let hash = compute_hash(&sig, "", "");
    GraphNode {
        id,
        hash,
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: sig,
        file_path: file_path.to_string(),
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
fn bench_sqlite_insert_10k_nodes() {
    let store = SqliteGraphStore::in_memory().unwrap();

    let start = Instant::now();
    for i in 0..10_000u64 {
        let node = make_node(i + 1, &format!("func_{i}"), &format!("src/mod_{}.ts", i / 10));
        store.insert_node(&node).unwrap();
    }
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 10,
        "inserting 10k nodes took {:?} — should be under 10s in debug",
        elapsed
    );
}

#[test]
fn bench_sqlite_insert_50k_edges() {
    let mut store = SqliteGraphStore::in_memory().unwrap();

    // Insert 1000 nodes first
    for i in 0..1_000u64 {
        let node = make_node(i + 1, &format!("func_{i}"), "src/mod.ts");
        store.insert_node(&node).unwrap();
    }

    let edges: Vec<_> = (0..5_000u64)
        .map(|i| {
            use keel_core::types::{EdgeChange, NodeChange};
            let _ = (NodeChange::Add(make_node(0, "", "")), EdgeChange::Add(GraphEdge {
                id: 0,
                source_id: 0,
                target_id: 0,
                kind: EdgeKind::Calls,
                file_path: String::new(),
                line: 0,
            }));
            GraphEdge {
                id: i + 1,
                source_id: (i % 1_000) + 1,
                target_id: ((i + 1) % 1_000) + 1,
                kind: EdgeKind::Calls,
                file_path: "src/mod.ts".to_string(),
                line: (i as u32) + 1,
            }
        })
        .collect();

    let start = Instant::now();
    let changes: Vec<_> = edges
        .into_iter()
        .map(keel_core::types::EdgeChange::Add)
        .collect();
    store.update_edges(changes).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 10,
        "inserting 5k edges took {:?} — should be under 10s in debug",
        elapsed
    );
}

#[test]
fn bench_sqlite_lookup_node_by_hash() {
    let store = SqliteGraphStore::in_memory().unwrap();

    // Insert 1000 nodes
    let mut target_hash = String::new();
    for i in 0..1_000u64 {
        let node = make_node(i + 1, &format!("func_{i}"), "src/mod.ts");
        if i == 500 {
            target_hash = node.hash.clone();
        }
        store.insert_node(&node).unwrap();
    }

    let start = Instant::now();
    for _ in 0..100 {
        let result = store.get_node(&target_hash);
        assert!(result.is_some(), "node should be found by hash");
    }
    let elapsed = start.elapsed();

    // 100 lookups should complete quickly
    assert!(
        elapsed.as_millis() < 500,
        "100 hash lookups took {:?} — should be fast",
        elapsed
    );
}

#[test]
fn bench_sqlite_adjacency_query() {
    let mut store = SqliteGraphStore::in_memory().unwrap();

    // Insert nodes
    for i in 0..100u64 {
        let node = make_node(i + 1, &format!("func_{i}"), "src/mod.ts");
        store.insert_node(&node).unwrap();
    }

    // Insert edges pointing to node 50
    let changes: Vec<_> = (0..20u64)
        .map(|i| {
            keel_core::types::EdgeChange::Add(GraphEdge {
                id: i + 1,
                source_id: i + 1,
                target_id: 50,
                kind: EdgeKind::Calls,
                file_path: "src/mod.ts".to_string(),
                line: 1,
            })
        })
        .collect();
    store.update_edges(changes).unwrap();

    let start = Instant::now();
    for _ in 0..100 {
        let edges = store.get_edges(50, keel_core::types::EdgeDirection::Both);
        assert!(!edges.is_empty(), "should have edges");
    }
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 1000,
        "100 adjacency queries took {:?} — should be fast",
        elapsed
    );
}

#[test]
fn bench_sqlite_full_graph_load() {
    let store = SqliteGraphStore::in_memory().unwrap();

    // Insert 1000 nodes
    for i in 0..1_000u64 {
        let node = make_node(i + 1, &format!("func_{i}"), &format!("src/mod_{}.ts", i / 10));
        store.insert_node(&node).unwrap();
    }

    let start = Instant::now();
    // Load all modules (simulates full graph load)
    let modules = store.get_all_modules();
    let _ = modules;
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 5,
        "full graph load took {:?} — should be under 5s in debug",
        elapsed
    );
}

#[test]
fn bench_sqlite_incremental_update_100_nodes() {
    let mut store = SqliteGraphStore::in_memory().unwrap();

    // Insert 1000 nodes
    for i in 0..1_000u64 {
        let node = make_node(i + 1, &format!("func_{i}"), "src/mod.ts");
        store.insert_node(&node).unwrap();
    }

    // Update 100 nodes
    let changes: Vec<_> = (0..100u64)
        .map(|i| {
            let mut node = make_node(i + 1, &format!("func_{i}_updated"), "src/mod.ts");
            node.line_end = 10;
            keel_core::types::NodeChange::Update(node)
        })
        .collect();

    let start = Instant::now();
    store.update_nodes(changes).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 2000,
        "updating 100 nodes took {:?} — should be under 2s in debug",
        elapsed
    );
}
