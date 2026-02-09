// Benchmark tests for discover (adjacency lookup) performance
//
// Validates that `keel discover <hash>` meets the <50ms target for
// returning callers, callees, and node metadata from the graph.
//
// use keel_core::graph::GraphStore;
// use keel_core::discover::{discover_node, DiscoverResult};
// use std::time::Instant;

#[test]
#[ignore = "Not yet implemented"]
fn bench_discover_node_under_50ms() {
    // GIVEN a graph with 50,000 nodes and 200,000 edges loaded in memory
    // WHEN discover is called for a node with 15 callers and 8 callees
    // THEN the DiscoverResult is returned in under 50 milliseconds
}

#[test]
#[ignore = "Not yet implemented"]
fn bench_discover_highly_connected_node() {
    // GIVEN a graph with a utility function called by 500 other nodes
    // WHEN discover is called for that highly-connected node
    // THEN the DiscoverResult is returned in under 50 milliseconds
}

#[test]
#[ignore = "Not yet implemented"]
fn bench_discover_leaf_node() {
    // GIVEN a graph with 50,000 nodes
    // WHEN discover is called for a leaf node with 0 callers and 0 callees
    // THEN the DiscoverResult is returned in under 10 milliseconds
}

#[test]
#[ignore = "Not yet implemented"]
fn bench_discover_sequential_100_lookups() {
    // GIVEN a graph with 50,000 nodes and 200,000 edges
    // WHEN discover is called 100 times sequentially for different nodes
    // THEN all 100 lookups complete in under 2 seconds total (avg <20ms each)
}
