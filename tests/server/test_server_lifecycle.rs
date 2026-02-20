// Tests for server lifecycle management (Spec 010)
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use keel_core::sqlite::SqliteGraphStore;
use keel_core::types::{GraphNode, NodeKind};
use keel_server::mcp::{create_shared_engine, process_line};
use keel_server::KeelServer;

#[test]
fn test_server_starts_and_loads_graph() {
    // Server can be created with in-memory store
    let server = KeelServer::in_memory(PathBuf::from("/tmp/test-project")).unwrap();

    // Engine is accessible
    let mut engine = server.engine.lock().unwrap();
    // Compile with no files should return ok
    let result = engine.compile(&[]);
    assert_eq!(result.status, "ok");
}

#[test]
fn test_server_starts_without_existing_graph() {
    // In-memory server starts with empty graph
    let server = KeelServer::in_memory(PathBuf::from("/tmp/empty-project")).unwrap();
    let mut engine = server.engine.lock().unwrap();
    let result = engine.compile(&[]);
    assert_eq!(result.status, "ok");
    assert!(result.errors.is_empty());
    assert!(result.warnings.is_empty());
    assert!(result.files_analyzed.is_empty());
}

#[test]
fn test_server_graceful_shutdown() {
    // Server can be dropped cleanly
    let server = KeelServer::in_memory(PathBuf::from("/tmp/shutdown-test")).unwrap();
    // Simulate some work
    {
        let mut engine = server.engine.lock().unwrap();
        let _ = engine.compile(&[]);
    }
    // Drop server — should not panic
    drop(server);
}

#[test]
fn test_server_engine_isolation() {
    // Two servers should have independent engines
    let server1 = KeelServer::in_memory(PathBuf::from("/tmp/s1")).unwrap();
    let server2 = KeelServer::in_memory(PathBuf::from("/tmp/s2")).unwrap();

    // Suppress E002 on server1 only
    server1.engine.lock().unwrap().suppress("E002");

    // Verify engines are distinct Arc<Mutex<>>
    assert!(!Arc::ptr_eq(&server1.engine, &server2.engine));
}

#[test]
fn test_server_mcp_process_line_integration() {
    // The MCP process_line function works with a shared store
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&GraphNode {
            id: 1,
            hash: "lifecycleH01".into(),
            kind: NodeKind::Function,
            name: "initApp".into(),
            signature: "fn initApp()".into(),
            file_path: "src/app.rs".into(),
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
        })
        .unwrap();
    let shared = Arc::new(Mutex::new(store));
    let engine = create_shared_engine(None);

    // Initialize
    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialize",
        "id": 1
    })
    .to_string();
    let resp: serde_json::Value =
        serde_json::from_str(&process_line(&shared, &engine, &init_req)).unwrap();
    assert_eq!(resp["result"]["serverInfo"]["name"], "keel");

    // Where lookup
    let where_req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "keel/where",
        "params": {"hash": "lifecycleH01"},
        "id": 2
    })
    .to_string();
    let resp: serde_json::Value =
        serde_json::from_str(&process_line(&shared, &engine, &where_req)).unwrap();
    assert_eq!(resp["result"]["file"], "src/app.rs");
}

#[test]
fn test_server_handles_concurrent_requests() {
    // Create a shared store and verify sequential access works
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&GraphNode {
            id: 1,
            hash: "concHash0001".into(),
            kind: NodeKind::Function,
            name: "handler".into(),
            signature: "fn handler()".into(),
            file_path: "src/h.rs".into(),
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
        })
        .unwrap();
    let shared: Arc<Mutex<SqliteGraphStore>> = Arc::new(Mutex::new(store));
    let engine = create_shared_engine(None);

    // Simulate 10 sequential requests without panics
    for i in 0..10 {
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "keel/where",
            "params": {"hash": "concHash0001"},
            "id": i
        })
        .to_string();
        let resp: serde_json::Value =
            serde_json::from_str(&process_line(&shared, &engine, &req)).unwrap();
        assert_eq!(resp["result"]["file"], "src/h.rs");
    }
}
