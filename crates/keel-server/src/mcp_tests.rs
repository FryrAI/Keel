use super::*;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};

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

fn store_with_edges() -> SharedStore {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    // Target node
    store
        .insert_node(&GraphNode {
            id: 1,
            hash: "targetHash01".to_string(),
            kind: NodeKind::Function,
            name: "handleRequest".to_string(),
            signature: "fn handleRequest(req: Request) -> Response".to_string(),
            file_path: "src/handler.rs".to_string(),
            line_start: 10,
            line_end: 30,
            docstring: Some("Handle HTTP request".to_string()),
            is_public: true,
            type_hints_present: true,
            has_docstring: true,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 0,
        })
        .unwrap();
    // Caller (upstream)
    store
        .insert_node(&GraphNode {
            id: 2,
            hash: "callerHash01".to_string(),
            kind: NodeKind::Function,
            name: "main".to_string(),
            signature: "fn main()".to_string(),
            file_path: "src/main.rs".to_string(),
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
    // Callee (downstream)
    store
        .insert_node(&GraphNode {
            id: 3,
            hash: "calleeHash01".to_string(),
            kind: NodeKind::Function,
            name: "validate".to_string(),
            signature: "fn validate(data: &str) -> bool".to_string(),
            file_path: "src/validate.rs".to_string(),
            line_start: 5,
            line_end: 15,
            docstring: Some("Validate input".to_string()),
            is_public: false,
            type_hints_present: true,
            has_docstring: true,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 0,
        })
        .unwrap();
    // Edge: main -> handleRequest (caller)
    // Edge: handleRequest -> validate (callee)
    store
        .update_edges(vec![
            EdgeChange::Add(GraphEdge {
                id: 1,
                source_id: 2,
                target_id: 1,
                kind: EdgeKind::Calls,
                file_path: "src/main.rs".to_string(),
                line: 3,
            }),
            EdgeChange::Add(GraphEdge {
                id: 2,
                source_id: 1,
                target_id: 3,
                kind: EdgeKind::Calls,
                file_path: "src/handler.rs".to_string(),
                line: 20,
            }),
        ])
        .unwrap();
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
    assert_eq!(resp["result"]["serverInfo"]["name"], "keel");
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
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
fn test_tools_list_has_input_schemas() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("tools/list", None)));
    let tools = resp["result"].as_array().unwrap();
    for tool in tools {
        assert!(tool["inputSchema"].is_object(), "tool {} missing inputSchema", tool["name"]);
    }
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
fn test_compile_batch_start() {
    let store = test_store();
    let params = serde_json::json!({"files": ["a.rs"], "batch_start": true});
    let resp = parse_response(&process_line(&store, &rpc("keel/compile", Some(params))));
    assert_eq!(resp["result"]["status"], "batch_started");
}

#[test]
fn test_compile_batch_end() {
    let store = test_store();
    let params = serde_json::json!({"files": [], "batch_end": true});
    let resp = parse_response(&process_line(&store, &rpc("keel/compile", Some(params))));
    assert_eq!(resp["result"]["status"], "batch_ended");
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
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("not found"));
}

#[test]
fn test_discover_missing_hash() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("keel/discover", None)));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("hash"));
}

#[test]
fn test_discover_with_edges() {
    let store = store_with_edges();
    let params = serde_json::json!({"hash": "targetHash01"});
    let resp = parse_response(&process_line(&store, &rpc("keel/discover", Some(params))));
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
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(&store, &rpc("keel/discover", Some(params))));
    let result = &resp["result"];
    assert!(result["upstream"].as_array().unwrap().is_empty());
    assert!(result["downstream"].as_array().unwrap().is_empty());
}

#[test]
fn test_where_existing_node() {
    let store = store_with_node();
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(&store, &rpc("keel/where", Some(params))));
    assert_eq!(resp["result"]["file"], "src/lib.rs");
    assert_eq!(resp["result"]["line_start"], 10);
    assert_eq!(resp["result"]["line_end"], 20);
    assert_eq!(resp["result"]["stale"], false);
}

#[test]
fn test_where_not_found() {
    let store = test_store();
    let params = serde_json::json!({"hash": "nope"});
    let resp = parse_response(&process_line(&store, &rpc("keel/where", Some(params))));
    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32602);
}

#[test]
fn test_where_missing_hash() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("keel/where", None)));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("hash"));
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
    assert!(result["summary"].as_str().unwrap().contains("doStuff"));
    assert_eq!(result["resolution_tier"], "tier1_treesitter");
}

#[test]
fn test_explain_not_found() {
    let store = test_store();
    let params = serde_json::json!({"hash": "nope"});
    let resp = parse_response(&process_line(&store, &rpc("keel/explain", Some(params))));
    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32602);
}

#[test]
fn test_explain_missing_hash() {
    let store = test_store();
    let params = serde_json::json!({"error_code": "E001"});
    let resp = parse_response(&process_line(&store, &rpc("keel/explain", Some(params))));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("hash"));
}

#[test]
fn test_explain_defaults_error_code() {
    let store = store_with_node();
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(&store, &rpc("keel/explain", Some(params))));
    assert_eq!(resp["result"]["error_code"], "E001");
}

#[test]
fn test_map() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("keel/map", None)));
    assert_eq!(resp["result"]["status"], "ok");
    assert_eq!(resp["result"]["format"], "json");
}

#[test]
fn test_map_with_format() {
    let store = test_store();
    let params = serde_json::json!({"format": "llm"});
    let resp = parse_response(&process_line(&store, &rpc("keel/map", Some(params))));
    assert_eq!(resp["result"]["format"], "llm");
}

#[test]
fn test_map_with_scope() {
    let store = test_store();
    let params = serde_json::json!({"scope": ["auth", "payments"]});
    let resp = parse_response(&process_line(&store, &rpc("keel/map", Some(params))));
    let scope = resp["result"]["scope"].as_array().unwrap();
    assert_eq!(scope.len(), 2);
    assert_eq!(scope[0], "auth");
    assert_eq!(scope[1], "payments");
}

#[test]
fn test_unknown_method() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("bogus/method", None)));
    assert_eq!(resp["error"]["code"], -32601);
    assert!(resp["error"]["message"].as_str().unwrap().contains("bogus/method"));
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

#[test]
fn test_response_preserves_id() {
    let store = test_store();
    let line = r#"{"jsonrpc":"2.0","method":"initialize","params":null,"id":42}"#;
    let resp = parse_response(&process_line(&store, line));
    assert_eq!(resp["id"], 42);
}

#[test]
fn test_response_null_id_when_missing() {
    let store = test_store();
    let line = r#"{"jsonrpc":"2.0","method":"initialize"}"#;
    let resp = parse_response(&process_line(&store, line));
    assert!(resp["id"].is_null());
}

#[test]
fn test_jsonrpc_version_in_response() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &rpc("initialize", None)));
    assert_eq!(resp["jsonrpc"], "2.0");
}
