use super::*;
use crate::mcp_tools::ToolInfo;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};
use keel_enforce::engine::EnforcementEngine;

#[path = "mcp_protocol_tests.rs"]
mod protocol;
#[path = "mcp_query_tests.rs"]
mod query;
#[path = "mcp_tool_handler_tests.rs"]
mod tool_handlers;

fn test_engine() -> SharedEngine {
    create_shared_engine(None)
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
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
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
    Arc::new(Mutex::new(EnforcementEngine::new(Box::new(
        populated_node_store(),
    ))))
}

fn make_node(id: u64, hash: &str, name: &str, sig: &str, file: &str) -> GraphNode {
    GraphNode {
        id,
        hash: hash.into(),
        kind: NodeKind::Function,
        name: name.into(),
        signature: sig.into(),
        file_path: file.into(),
        line_start: 1,
        line_end: 20,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

fn make_edge_test_data() -> (Vec<GraphNode>, Vec<EdgeChange>) {
    let nodes = vec![
        make_node(
            1,
            "targetHash01",
            "handleRequest",
            "fn handleRequest(req: Request) -> Response",
            "src/handler.rs",
        ),
        make_node(2, "callerHash01", "main", "fn main()", "src/main.rs"),
        make_node(
            3,
            "calleeHash01",
            "validate",
            "fn validate(data: &str) -> bool",
            "src/validate.rs",
        ),
    ];
    let edges = vec![
        EdgeChange::Add(GraphEdge {
            id: 1,
            source_id: 2,
            target_id: 1,
            kind: EdgeKind::Calls,
            file_path: "src/main.rs".into(),
            line: 3,
            confidence: 1.0,
        }),
        EdgeChange::Add(GraphEdge {
            id: 2,
            source_id: 1,
            target_id: 3,
            kind: EdgeKind::Calls,
            file_path: "src/handler.rs".into(),
            line: 20,
            confidence: 1.0,
        }),
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
    Arc::new(Mutex::new(EnforcementEngine::new(Box::new(
        populated_edge_store(),
    ))))
}

/// Store with a module + function node — needed for search which iterates modules.
fn store_with_module_and_node() -> SharedStore {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&GraphNode {
            id: 100,
            hash: "moduleHash01".to_string(),
            kind: NodeKind::Module,
            name: "lib".to_string(),
            signature: String::new(),
            file_path: "src/lib.rs".to_string(),
            line_start: 1,
            line_end: 50,
            docstring: None,
            is_public: true,
            type_hints_present: true,
            has_docstring: false,
            is_associated: false,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 0,
            package: None,
        })
        .unwrap();
    store.insert_node(&make_test_node()).unwrap();
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

/// Build an MCP-spec `tools/call` request for `name` with `arguments`.
fn tools_call(name: &str, arguments: Option<Value>) -> String {
    rpc(
        "tools/call",
        Some(serde_json::json!({ "name": name, "arguments": arguments })),
    )
}

/// Unwrap an MCP `CallToolResult` back into the tool's underlying JSON payload.
fn unwrap_tool_result(resp: &Value) -> Value {
    assert!(
        resp["error"].is_null(),
        "expected a successful tools/call, got error: {}",
        resp["error"]
    );
    let content = resp["result"]["content"]
        .as_array()
        .expect("CallToolResult must have a content array");
    assert_eq!(content.len(), 1, "expected exactly one content block");
    assert_eq!(content[0]["type"], "text");
    let text = content[0]["text"]
        .as_str()
        .expect("content block must carry text");
    serde_json::from_str(text).expect("tool payload should be valid JSON")
}

#[test]
fn test_initialize() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("initialize", None),
    ));
    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["result"]["serverInfo"]["name"], "keel");
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
    assert!(resp["error"].is_null());
}

#[test]
fn test_tools_list() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("tools/list", None),
    ));
    let tools: Vec<ToolInfo> = serde_json::from_value(resp["result"]["tools"].clone()).unwrap();
    assert_eq!(tools.len(), 16);
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"keel/compile"));
    assert!(names.contains(&"keel/skeleton"));
    assert!(names.contains(&"keel/focus"));
    assert!(names.contains(&"keel/discover"));
    assert!(names.contains(&"keel/where"));
    assert!(names.contains(&"keel/explain"));
    assert!(names.contains(&"keel/map"));
    assert!(names.contains(&"keel/check"));
    assert!(names.contains(&"keel/audit"));
    assert!(names.contains(&"keel/fix"));
    assert!(names.contains(&"keel/search"));
    assert!(names.contains(&"keel/name"));
    assert!(names.contains(&"keel/analyze"));
    assert!(names.contains(&"keel/context"));
}

#[test]
fn test_tools_list_has_input_schemas() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("tools/list", None),
    ));
    let tools = resp["result"]["tools"].as_array().unwrap();
    for tool in tools {
        assert!(
            tool["inputSchema"].is_object(),
            "tool {} missing inputSchema",
            tool["name"]
        );
    }
}

/// MCP requires `{"result":{"tools":[...]}}`; a bare array makes spec-compliant
/// clients treat the fetch as unanswered and time out (issue #27).
#[test]
fn test_tools_list_is_wrapped_in_object() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("tools/list", None),
    ));
    assert!(
        resp["result"].is_object(),
        "tools/list result must be an object, not a bare array"
    );
    assert!(resp["result"]["tools"].is_array());
}

#[test]
fn test_compile_with_files() {
    let store = test_store();
    let params = serde_json::json!({"files": ["src/main.rs"]});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/compile", Some(params)),
    ));
    let result = &resp["result"];
    assert_eq!(result["status"], "ok");
    assert_eq!(result["files_analyzed"][0], "src/main.rs");
}

#[test]
fn test_compile_no_params() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/compile", None),
    ));
    assert_eq!(resp["result"]["status"], "ok");
    assert!(resp["result"]["files_analyzed"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn test_compile_batch_start() {
    let store = test_store();
    let params = serde_json::json!({"files": ["a.rs"], "batch_start": true});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/compile", Some(params)),
    ));
    assert_eq!(resp["result"]["status"], "batch_started");
}

#[test]
fn test_compile_batch_end() {
    let store = test_store();
    let params = serde_json::json!({"files": [], "batch_end": true});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/compile", Some(params)),
    ));
    assert_eq!(resp["result"]["status"], "batch_ended");
}

#[test]
fn test_unknown_method() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("bogus/method", None),
    ));
    assert_eq!(resp["error"]["code"], -32601);
    assert!(resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("bogus/method"));
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

/// A message with no `id` is a notification, so it gets no response at all —
/// previously this returned a response carrying `"id": null` (issue #27).
#[test]
fn test_no_response_when_id_missing() {
    let store = test_store();
    let line = r#"{"jsonrpc":"2.0","method":"initialize"}"#;
    assert!(process_line(&store, &test_engine(), line).is_empty());
}

#[test]
fn test_jsonrpc_version_in_response() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("initialize", None),
    ));
    assert_eq!(resp["jsonrpc"], "2.0");
}

// --- keel/check tests ---
