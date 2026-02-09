// Tests for server lifecycle management (Spec 010)
//
// Validates server startup, shutdown, graph loading, memory management,
// and graceful handling of concurrent requests during lifecycle transitions.
//
// use keel_server::{ServerConfig, Server};
// use keel_core::graph::GraphStore;
// use std::time::Duration;

#[test]
#[ignore = "Not yet implemented"]
fn test_server_starts_and_loads_graph() {
    // GIVEN a project directory with an existing .keel/graph.db
    // WHEN `keel serve` is started
    // THEN the server loads the graph into memory and reports ready status
}

#[test]
#[ignore = "Not yet implemented"]
fn test_server_starts_without_existing_graph() {
    // GIVEN a project directory that has been `keel init`-ed but not yet mapped
    // WHEN `keel serve` is started
    // THEN the server starts with an empty graph and triggers an initial map
}

#[test]
#[ignore = "Not yet implemented"]
fn test_server_graceful_shutdown() {
    // GIVEN a running keel server handling active requests
    // WHEN a shutdown signal (SIGTERM / SIGINT) is received
    // THEN in-flight requests complete, the graph is persisted, and the server exits cleanly
}

#[test]
#[ignore = "Not yet implemented"]
fn test_server_memory_stays_within_bounds() {
    // GIVEN a keel server serving a 100k LOC project
    // WHEN the server has been running and handling requests for a sustained period
    // THEN memory usage stays within the ~50-100MB target range
}

#[test]
#[ignore = "Not yet implemented"]
fn test_server_reloads_graph_after_external_map() {
    // GIVEN a running keel server with a loaded graph
    // WHEN `keel map` is run externally (separate process) updating graph.db
    // THEN the server detects the change and reloads the updated graph
}

#[test]
#[ignore = "Not yet implemented"]
fn test_server_handles_concurrent_requests() {
    // GIVEN a running keel server with a loaded graph
    // WHEN 50 concurrent compile requests arrive simultaneously
    // THEN all requests are handled correctly without data races or panics
}
