// Tests for MCP (Model Context Protocol) server implementation (Spec 010)
use std::sync::{Arc, Mutex};

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};
use keel_server::mcp::process_line;

type SharedStore = Arc<Mutex<SqliteGraphStore>>;

fn test_store() -> SharedStore {
    let store = SqliteGraphStore::in_memory().unwrap();
    Arc::new(Mutex::new(store))
}

fn store_with_node() -> SharedStore {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&GraphNode {
            id: 1,
            hash: "testHash1234".into(),
            kind: NodeKind::Function,
            name: "processData".into(),
            signature: "fn processData(x: i32) -> bool".into(),
            file_path: "src/processor.rs".into(),
            line_start: 10,
            line_end: 25,
            docstring: Some("Processes data".into()),
            is_public: true,
            type_hints_present: true,
            has_docstring: true,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 0,
        })
        .unwrap();
    Arc::new(Mutex::new(store))
}

fn store_with_graph() -> SharedStore {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&GraphNode {
            id: 1,
            hash: "targetFunc01".into(),
            kind: NodeKind::Function,
            name: "handleReq".into(),
            signature: "fn handleReq(r: Req) -> Resp".into(),
            file_path: "src/handler.rs".into(),
            line_start: 5,
            line_end: 20,
            docstring: Some("Handle request".into()),
            is_public: true,
            type_hints_present: true,
            has_docstring: true,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 0,
        })
        .unwrap();
    store
        .insert_node(&GraphNode {
            id: 2,
            hash: "callerFunc01".into(),
            kind: NodeKind::Function,
            name: "main".into(),
            signature: "fn main()".into(),
            file_path: "src/main.rs".into(),
            line_start: 1,
            line_end: 5,
            docstring: None,
            is_public: true,
            type_hints_present: true,
            has_docstring: false,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 0,
        })
        .unwrap();
    store
        .update_edges(vec![EdgeChange::Add(GraphEdge {
            id: 1,
            source_id: 2,
            target_id: 1,
            kind: EdgeKind::Calls,
            file_path: "src/main.rs".into(),
            line: 3,
        })])
        .unwrap();
    Arc::new(Mutex::new(store))
}

fn rpc(method: &str, params: Option<serde_json::Value>) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1
    })
    .to_string()
}

fn parse_response(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).expect("response should be valid JSON")
}

#[test]
fn test_mcp_server_registers_all_tools() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("tools/list", None)));

    let tools = resp["result"].as_array().unwrap();
    assert_eq!(tools.len(), 5);

    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"keel/compile"));
    assert!(names.contains(&"keel/discover"));
    assert!(names.contains(&"keel/where"));
    assert!(names.contains(&"keel/explain"));
    assert!(names.contains(&"keel/map"));

    // Every tool has an inputSchema
    for tool in tools {
        assert!(tool["inputSchema"].is_object());
    }
}

#[test]
fn test_mcp_compile_tool_returns_violations() {
    let store = test_store();
    let params = serde_json::json!({"files": ["src/test.py"]});
    let resp = parse_response(&process_line(&store, &rpc("keel/compile", Some(params))));

    let result = &resp["result"];
    assert_eq!(result["command"], "compile");
    assert!(result["files_analyzed"].is_array());
    assert!(result["errors"].is_array());
    assert!(result["warnings"].is_array());
}

#[test]
fn test_mcp_compile_tool_clean_returns_empty() {
    let store = test_store();
    let params = serde_json::json!({"files": []});
    let resp = parse_response(&process_line(&store, &rpc("keel/compile", Some(params))));

    let result = &resp["result"];
    assert_eq!(result["status"], "ok");
    assert_eq!(result["errors"].as_array().unwrap().len(), 0);
    assert_eq!(result["warnings"].as_array().unwrap().len(), 0);
}

#[test]
fn test_mcp_discover_tool_returns_adjacency() {
    let store = store_with_graph();
    let params = serde_json::json!({"hash": "targetFunc01"});
    let resp = parse_response(&process_line(&store, &rpc("keel/discover", Some(params))));

    let result = &resp["result"];
    assert_eq!(result["target"]["name"], "handleReq");
    assert_eq!(result["target"]["hash"], "targetFunc01");

    let upstream = result["upstream"].as_array().unwrap();
    assert_eq!(upstream.len(), 1);
    assert_eq!(upstream[0]["name"], "main");
    assert_eq!(upstream[0]["call_line"], 3);
}

#[test]
fn test_mcp_discover_tool_unknown_hash() {
    let store = test_store();
    let params = serde_json::json!({"hash": "doesNotExist"});
    let resp = parse_response(&process_line(&store, &rpc("keel/discover", Some(params))));

    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not found"));
}

#[test]
fn test_mcp_map_tool_triggers_full_remap() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("keel/map", None)));

    let result = &resp["result"];
    assert_eq!(result["status"], "ok");
    assert_eq!(result["format"], "json");
}

#[test]
fn test_mcp_explain_tool_returns_resolution_chain() {
    let store = store_with_node();
    let params = serde_json::json!({"error_code": "E001", "hash": "testHash1234"});
    let resp = parse_response(&process_line(&store, &rpc("keel/explain", Some(params))));

    let result = &resp["result"];
    assert_eq!(result["error_code"], "E001");
    assert_eq!(result["hash"], "testHash1234");

    let chain = result["resolution_chain"].as_array().unwrap();
    assert!(!chain.is_empty());
    assert_eq!(chain[0]["kind"], "lookup");

    assert!(result["summary"]
        .as_str()
        .unwrap()
        .contains("processData"));
}

#[test]
fn test_mcp_where_tool_resolves_hash_to_location() {
    let store = store_with_node();
    let params = serde_json::json!({"hash": "testHash1234"});
    let resp = parse_response(&process_line(&store, &rpc("keel/where", Some(params))));

    let result = &resp["result"];
    assert_eq!(result["file"], "src/processor.rs");
    assert_eq!(result["line_start"], 10);
    assert_eq!(result["line_end"], 25);
}
