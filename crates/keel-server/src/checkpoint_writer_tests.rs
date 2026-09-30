//! Checkpoint must report and persist a tracked edit after contention clears.

use crate::mcp::process_line_with_root;
use crate::writer_test_support::{checkpoint_git_fixture, state};
use crate::writer_tests::disk_fixture;
use keel_core::{graph_lock, sqlite::SqliteGraphStore, store::GraphStore};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[test]
fn mcp_disk_checkpoint_busy_then_reports_delta_and_updates_hash_after_release() {
    let (dir, store, engine, _source) = disk_fixture(true);
    checkpoint_git_fixture(dir.path());
    let shared = Arc::new(Mutex::new(
        SqliteGraphStore::open(dir.path().join(".keel/graph.db").to_str().unwrap()).unwrap(),
    ));
    let request = json!({"jsonrpc":"2.0", "id":1, "method":"tools/call",
        "params":{"name":"keel/checkpoint","arguments":{}}})
    .to_string();
    let before = state(dir.path());
    let held = graph_lock::try_acquire(&dir.path().join(".keel")).unwrap();
    let response: Value = serde_json::from_str(&process_line_with_root(
        &shared,
        &engine,
        dir.path(),
        &request,
    ))
    .unwrap();
    assert_eq!(response["result"]["isError"], true, "{response}");
    assert!(response.to_string().contains("graph busy"));
    assert_eq!(state(dir.path()), before);
    drop(held);
    let response: Value = serde_json::from_str(&process_line_with_root(
        &shared,
        &engine,
        dir.path(),
        &request,
    ))
    .unwrap();
    assert_ne!(response["result"]["isError"], true, "{response}");
    assert!(response.get("error").is_none(), "{response}");
    let result: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["files"].as_array().unwrap().len(), 1, "{result}");
    assert_eq!(result["files"][0]["file"], "hello.rs");
    assert_eq!(result["files"][0]["changed"][0]["name"], "hello");
    let node = store.get_node_by_id(1).unwrap();
    assert_ne!(node.hash, "seed_hash");
    assert_eq!(result["files"][0]["changed"][0]["hash"], node.hash);
}
