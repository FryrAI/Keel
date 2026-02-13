// Benchmark tests for discover (adjacency lookup) performance
// Uses SqliteGraphStore and EnforcementEngine directly for precise timing.

use keel_core::hash::compute_hash;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};
use keel_enforce::engine::EnforcementEngine;
use std::time::Instant;

fn make_node(id: u64, name: &str) -> GraphNode {
    let sig = format!("fn {name}()");
    let hash = compute_hash(&sig, &format!("body_{id}"), "");
    GraphNode {
        id,
        hash,
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: sig,
        file_path: format!("src/mod_{}.ts", id / 10),
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

fn setup_graph(node_count: u64, edges_per_node: u64) -> (SqliteGraphStore, Vec<String>) {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let mut hashes = Vec::new();

    for i in 1..=node_count {
        let node = make_node(i, &format!("func_{i}"));
        hashes.push(node.hash.clone());
        store.insert_node(&node).unwrap();
    }

    // Create edges: each node calls the next `edges_per_node` nodes
    let mut edge_changes = Vec::new();
    let mut edge_id = 1u64;
    for i in 1..=node_count {
        for j in 1..=edges_per_node {
            let target = ((i + j - 1) % node_count) + 1;
            if target != i {
                edge_changes.push(EdgeChange::Add(GraphEdge {
                    id: edge_id,
                    source_id: i,
                    target_id: target,
                    kind: EdgeKind::Calls,
                    file_path: format!("src/mod_{}.ts", i / 10),
                    line: 1,
                }));
                edge_id += 1;
            }
        }
    }
    store.update_edges(edge_changes).unwrap();

    (store, hashes)
}

#[test]
fn bench_discover_node_under_50ms() {
    let (store, hashes) = setup_graph(1_000, 3);
    let engine = EnforcementEngine::new(Box::new(store));

    let target_hash = &hashes[500];
    let start = Instant::now();
    let result = engine.discover(target_hash, 1);
    let elapsed = start.elapsed();

    assert!(result.is_some(), "discover should find node");
    // Debug mode: allow 1s (release target: 50ms)
    assert!(elapsed.as_millis() < 1000, "discover took {:?}", elapsed);
}

#[test]
fn bench_discover_highly_connected_node() {
    let mut store = SqliteGraphStore::in_memory().unwrap();

    // Create a hub node called by many others
    let hub = make_node(1, "hub_function");
    let hub_hash = hub.hash.clone();
    store.insert_node(&hub).unwrap();

    // Create 200 callers
    let mut edge_changes = Vec::new();
    for i in 2..=201u64 {
        let node = make_node(i, &format!("caller_{i}"));
        store.insert_node(&node).unwrap();
        edge_changes.push(EdgeChange::Add(GraphEdge {
            id: i,
            source_id: i,
            target_id: 1,
            kind: EdgeKind::Calls,
            file_path: format!("src/caller_{i}.ts"),
            line: 1,
        }));
    }
    store.update_edges(edge_changes).unwrap();

    let engine = EnforcementEngine::new(Box::new(store));

    let start = Instant::now();
    let result = engine.discover(&hub_hash, 1);
    let elapsed = start.elapsed();

    assert!(result.is_some(), "discover should find hub node");
    assert!(elapsed.as_millis() < 1000, "highly-connected discover took {:?}", elapsed);
}

#[test]
fn bench_discover_leaf_node() {
    let (store, hashes) = setup_graph(100, 0);
    let engine = EnforcementEngine::new(Box::new(store));

    // Leaf node with no edges
    let start = Instant::now();
    let result = engine.discover(&hashes[50], 1);
    let elapsed = start.elapsed();

    assert!(result.is_some(), "discover should find leaf node");
    assert!(elapsed.as_millis() < 500, "leaf discover took {:?}", elapsed);
}

#[test]
fn bench_discover_sequential_100_lookups() {
    let (store, hashes) = setup_graph(500, 2);
    let engine = EnforcementEngine::new(Box::new(store));

    let start = Instant::now();
    for i in 0..100 {
        let result = engine.discover(&hashes[i * 4], 1);
        assert!(result.is_some(), "discover should find node {i}");
    }
    let elapsed = start.elapsed();

    // 100 lookups should complete in reasonable time
    assert!(
        elapsed.as_secs() < 10,
        "100 sequential discovers took {:?}",
        elapsed
    );
}
