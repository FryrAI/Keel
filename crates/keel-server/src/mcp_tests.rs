use super::*;
use keel_core::types::{GraphNode, NodeKind};

fn test_store() -> SharedStore {
    let store = SqliteGraphStore::in_memory().unwrap();
    Arc::new(Mutex::new(store))
}

fn store_with_node() -> SharedStore {
    let store = SqliteGraphStore::in_memory().unwrap();
    let node = GraphNode {
        id: 1,
        hash: "a7Bx3kM9f2Q".to_string(),
        kind: NodeKind::Function,
        name: "doStuff".to_string(),
        signature: "fn doStuff(x: i32) -> bool".to_string(),
        file_path: "src/lib.rs".to_string(),
        line_start: 10,
        line_end: 20,
        docstring: Some("Does stuff".to_string()),
        is_public: true,
        type_hints_present: true,
        has_docstring: true,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
    };
    store.insert_node(&node).unwrap();
    Arc::new(Mutex::new(store))
}

fn rpc(method: &str, params: Option<Value>) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1
    })
    .to_string()
}

fn parse_response(raw: &str) -> Value {
    serde_json::from_str(raw).expect("response should be valid JSON")
}

#[test]
fn test_initialize() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("initialize", None)));
    assert_eq!(resp["jsonrpc"], "2.0");
    assert!(resp["result"]["serverInfo"]["name"].as_str().unwrap() == "keel");
    assert!(resp["error"].is_null());
}

#[test]
fn test_tools_list() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("tools/list", None)));
    let tools: Vec<ToolInfo> = serde_json::from_value(resp["result"].clone()).unwrap();
    assert_eq!(tools.len(), 5);
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"keel/compile"));
    assert!(names.contains(&"keel/discover"));
    assert!(names.contains(&"keel/where"));
    assert!(names.contains(&"keel/explain"));
    assert!(names.contains(&"keel/map"));
}

#[test]
fn test_compile_with_files() {
    let store = test_store();
    let params = serde_json::json!({"files": ["src/main.rs"]});
    let resp = parse_response(&process_line(&store, &rpc("keel/compile", Some(params))));
    let result = &resp["result"];
    assert_eq!(result["status"], "ok");
    assert_eq!(result["files_analyzed"][0], "src/main.rs");
}

#[test]
fn test_compile_no_params() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("keel/compile", None)));
    assert_eq!(resp["result"]["status"], "ok");
    assert!(resp["result"]["files_analyzed"].as_array().unwrap().is_empty());
}

#[test]
fn test_discover_existing_node() {
    let store = store_with_node();
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(&store, &rpc("keel/discover", Some(params))));
    let result = &resp["result"];
    assert_eq!(result["target"]["name"], "doStuff");
    assert_eq!(result["target"]["hash"], "a7Bx3kM9f2Q");
}

#[test]
fn test_discover_not_found() {
    let store = test_store();
    let params = serde_json::json!({"hash": "nonexistent"});
    let resp = parse_response(&process_line(&store, &rpc("keel/discover", Some(params))));
    assert!(resp["error"]["code"].as_i64().unwrap() == -32602);
}

#[test]
fn test_discover_missing_hash() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("keel/discover", None)));
    assert!(resp["error"]["message"].as_str().unwrap().contains("hash"));
}

#[test]
fn test_where_existing_node() {
    let store = store_with_node();
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(&store, &rpc("keel/where", Some(params))));
    assert_eq!(resp["result"]["file"], "src/lib.rs");
    assert_eq!(resp["result"]["line"], 10);
}

#[test]
fn test_where_not_found() {
    let store = test_store();
    let params = serde_json::json!({"hash": "nope"});
    let resp = parse_response(&process_line(&store, &rpc("keel/where", Some(params))));
    assert!(resp["error"].is_object());
}

#[test]
fn test_explain_existing_node() {
    let store = store_with_node();
    let params = serde_json::json!({"error_code": "E001", "hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(&store, &rpc("keel/explain", Some(params))));
    let result = &resp["result"];
    assert_eq!(result["error_code"], "E001");
    assert_eq!(result["hash"], "a7Bx3kM9f2Q");
    assert!(!result["resolution_chain"].as_array().unwrap().is_empty());
}

#[test]
fn test_explain_not_found() {
    let store = test_store();
    let params = serde_json::json!({"hash": "nope"});
    let resp = parse_response(&process_line(&store, &rpc("keel/explain", Some(params))));
    assert!(resp["error"].is_object());
}

#[test]
fn test_map() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("keel/map", None)));
    assert_eq!(resp["result"]["status"], "ok");
}

#[test]
fn test_unknown_method() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("bogus/method", None)));
    assert_eq!(resp["error"]["code"], -32601);
}

#[test]
fn test_parse_error() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, "not valid json"));
    assert_eq!(resp["error"]["code"], -32700);
}

#[test]
fn test_empty_line() {
    let store = test_store();
    let resp = process_line(&store, "");
    assert!(resp.is_empty());
}
