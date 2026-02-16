use super::*;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};
use keel_enforce::engine::EnforcementEngine;

fn test_engine() -> SharedEngine {
    create_shared_engine()
}

fn test_store() -> SharedStore {
    let store = SqliteGraphStore::in_memory().unwrap();
    Arc::new(Mutex::new(store))
}

fn make_test_node() -> GraphNode {
    GraphNode {
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
    }
}

fn populated_node_store() -> SqliteGraphStore {
    let store = SqliteGraphStore::in_memory().unwrap();
    store.insert_node(&make_test_node()).unwrap();
    store
}

fn store_with_node() -> SharedStore {
    Arc::new(Mutex::new(populated_node_store()))
}

fn engine_with_node() -> SharedEngine {
    Arc::new(Mutex::new(EnforcementEngine::new(Box::new(populated_node_store()))))
}

fn make_node(id: u64, hash: &str, name: &str, sig: &str, file: &str) -> GraphNode {
    GraphNode {
        id, hash: hash.into(), kind: NodeKind::Function, name: name.into(),
        signature: sig.into(), file_path: file.into(),
        line_start: 1, line_end: 20, docstring: None,
        is_public: true, type_hints_present: true, has_docstring: false,
        external_endpoints: vec![], previous_hashes: vec![], module_id: 0,
    }
}

fn make_edge_test_data() -> (Vec<GraphNode>, Vec<EdgeChange>) {
    let nodes = vec![
        make_node(1, "targetHash01", "handleRequest", "fn handleRequest(req: Request) -> Response", "src/handler.rs"),
        make_node(2, "callerHash01", "main", "fn main()", "src/main.rs"),
        make_node(3, "calleeHash01", "validate", "fn validate(data: &str) -> bool", "src/validate.rs"),
    ];
    let edges = vec![
        EdgeChange::Add(GraphEdge { id: 1, source_id: 2, target_id: 1, kind: EdgeKind::Calls, file_path: "src/main.rs".into(), line: 3, confidence: 1.0 }),
        EdgeChange::Add(GraphEdge { id: 2, source_id: 1, target_id: 3, kind: EdgeKind::Calls, file_path: "src/handler.rs".into(), line: 20, confidence: 1.0 }),
    ];
    (nodes, edges)
}

fn populated_edge_store() -> SqliteGraphStore {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let (nodes, edges) = make_edge_test_data();
    for node in &nodes {
        store.insert_node(node).unwrap();
    }
    store.update_edges(edges).unwrap();
    store
}

fn store_with_edges() -> SharedStore {
    Arc::new(Mutex::new(populated_edge_store()))
}

fn engine_with_edges() -> SharedEngine {
    Arc::new(Mutex::new(EnforcementEngine::new(Box::new(populated_edge_store()))))
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
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("initialize", None)));
    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["result"]["serverInfo"]["name"], "keel");
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
    assert!(resp["error"].is_null());
}

#[test]
fn test_tools_list() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("tools/list", None)));
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
fn test_tools_list_has_input_schemas() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("tools/list", None)));
    let tools = resp["result"].as_array().unwrap();
    for tool in tools {
        assert!(tool["inputSchema"].is_object(), "tool {} missing inputSchema", tool["name"]);
    }
}

#[test]
fn test_compile_with_files() {
    let store = test_store();
    let params = serde_json::json!({"files": ["src/main.rs"]});
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/compile", Some(params))));
    let result = &resp["result"];
    assert_eq!(result["status"], "ok");
    assert_eq!(result["files_analyzed"][0], "src/main.rs");
}

#[test]
fn test_compile_no_params() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/compile", None)));
    assert_eq!(resp["result"]["status"], "ok");
    assert!(resp["result"]["files_analyzed"].as_array().unwrap().is_empty());
}

#[test]
fn test_compile_batch_start() {
    let store = test_store();
    let params = serde_json::json!({"files": ["a.rs"], "batch_start": true});
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/compile", Some(params))));
    assert_eq!(resp["result"]["status"], "batch_started");
}

#[test]
fn test_compile_batch_end() {
    let store = test_store();
    let params = serde_json::json!({"files": [], "batch_end": true});
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/compile", Some(params))));
    assert_eq!(resp["result"]["status"], "batch_ended");
}

#[test]
fn test_discover_existing_node() {
    let store = store_with_node();
    let engine = engine_with_node();
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(&store, &engine, &rpc("keel/discover", Some(params))));
    let result = &resp["result"];
    assert_eq!(result["target"]["name"], "doStuff");
    assert_eq!(result["target"]["hash"], "a7Bx3kM9f2Q");
}

#[test]
fn test_discover_not_found() {
    let store = test_store();
    let params = serde_json::json!({"hash": "nonexistent"});
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/discover", Some(params))));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("not found"));
}

#[test]
fn test_discover_missing_hash() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/discover", None)));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("hash"));
}

#[test]
fn test_discover_with_edges() {
    let store = store_with_edges();
    let engine = engine_with_edges();
    let params = serde_json::json!({"hash": "targetHash01"});
    let resp = parse_response(&process_line(&store, &engine, &rpc("keel/discover", Some(params))));
    let result = &resp["result"];

    assert_eq!(result["target"]["name"], "handleRequest");

    let upstream = result["upstream"].as_array().unwrap();
    assert_eq!(upstream.len(), 1);
    assert_eq!(upstream[0]["name"], "main");
    assert_eq!(upstream[0]["hash"], "callerHash01");
    assert_eq!(upstream[0]["call_line"], 3);

    let downstream = result["downstream"].as_array().unwrap();
    assert_eq!(downstream.len(), 1);
    assert_eq!(downstream[0]["name"], "validate");
    assert_eq!(downstream[0]["hash"], "calleeHash01");
    assert_eq!(downstream[0]["call_line"], 20);
}

#[test]
fn test_discover_no_edges() {
    let store = store_with_node();
    let engine = engine_with_node();
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(&store, &engine, &rpc("keel/discover", Some(params))));
    let result = &resp["result"];
    assert!(result["upstream"].as_array().unwrap().is_empty());
    assert!(result["downstream"].as_array().unwrap().is_empty());
}

#[test]
fn test_where_existing_node() {
    let store = store_with_node();
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/where", Some(params))));
    assert_eq!(resp["result"]["file"], "src/lib.rs");
    assert_eq!(resp["result"]["line_start"], 10);
    assert_eq!(resp["result"]["line_end"], 20);
    assert_eq!(resp["result"]["stale"], false);
}

#[test]
fn test_where_not_found() {
    let store = test_store();
    let params = serde_json::json!({"hash": "nope"});
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/where", Some(params))));
    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32602);
}

#[test]
fn test_where_missing_hash() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/where", None)));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("hash"));
}

#[test]
fn test_explain_existing_node() {
    let store = store_with_node();
    let params = serde_json::json!({"error_code": "E001", "hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/explain", Some(params))));
    let result = &resp["result"];
    assert_eq!(result["error_code"], "E001");
    assert_eq!(result["hash"], "a7Bx3kM9f2Q");
    assert!(!result["resolution_chain"].as_array().unwrap().is_empty());
    assert!(result["summary"].as_str().unwrap().contains("doStuff"));
    assert_eq!(result["resolution_tier"], "tree-sitter");
}

#[test]
fn test_explain_not_found() {
    let store = test_store();
    let params = serde_json::json!({"hash": "nope"});
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/explain", Some(params))));
    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32602);
}

#[test]
fn test_explain_missing_hash() {
    let store = test_store();
    let params = serde_json::json!({"error_code": "E001"});
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/explain", Some(params))));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("hash"));
}

#[test]
fn test_explain_defaults_error_code() {
    let store = store_with_node();
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/explain", Some(params))));
    assert_eq!(resp["result"]["error_code"], "E001");
}

#[test]
fn test_map() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/map", None)));
    assert_eq!(resp["result"]["status"], "ok");
    assert_eq!(resp["result"]["format"], "json");
}

#[test]
fn test_map_with_format() {
    let store = test_store();
    let params = serde_json::json!({"format": "llm"});
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/map", Some(params))));
    assert_eq!(resp["result"]["format"], "llm");
}

#[test]
fn test_map_with_scope() {
    let store = test_store();
    let params = serde_json::json!({"scope": ["auth", "payments"]});
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("keel/map", Some(params))));
    let scope = resp["result"]["scope"].as_array().unwrap();
    assert_eq!(scope.len(), 2);
    assert_eq!(scope[0], "auth");
    assert_eq!(scope[1], "payments");
}

#[test]
fn test_unknown_method() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("bogus/method", None)));
    assert_eq!(resp["error"]["code"], -32601);
    assert!(resp["error"]["message"].as_str().unwrap().contains("bogus/method"));
}

#[test]
fn test_parse_error() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &test_engine(), "not valid json"));
    assert_eq!(resp["error"]["code"], -32700);
}

#[test]
fn test_empty_line() {
    let store = test_store();
    let resp = process_line(&store, &test_engine(), "");
    assert!(resp.is_empty());
}

#[test]
fn test_response_preserves_id() {
    let store = test_store();
    let line = r#"{"jsonrpc":"2.0","method":"initialize","params":null,"id":42}"#;
    let resp = parse_response(&process_line(&store, &test_engine(), line));
    assert_eq!(resp["id"], 42);
}

#[test]
fn test_response_null_id_when_missing() {
    let store = test_store();
    let line = r#"{"jsonrpc":"2.0","method":"initialize"}"#;
    let resp = parse_response(&process_line(&store, &test_engine(), line));
    assert!(resp["id"].is_null());
}

#[test]
fn test_jsonrpc_version_in_response() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &test_engine(), &rpc("initialize", None)));
    assert_eq!(resp["jsonrpc"], "2.0");
}
