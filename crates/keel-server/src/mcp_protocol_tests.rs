//! MCP/JSON-RPC protocol conformance tests (GitHub issue #27).
//!
//! Covers notification silence, `initialize` version negotiation, and
//! `tools/call` routing. Split from `mcp_tests.rs` to keep both files
//! reviewable.

use super::*;

// --- JSON-RPC notification handling (issue #27, bug 1) ---

/// The exact message from the issue report: it drew a `-32601` error with
/// `"id": null`, desynchronizing spec-compliant clients.
#[test]
fn test_notifications_initialized_gets_no_response() {
    let store = test_store();
    let line = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    assert!(
        process_line(&store, &test_engine(), line).is_empty(),
        "notifications must never be answered"
    );
}

#[test]
fn test_notification_with_params_gets_no_response() {
    let store = test_store();
    let line = r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#;
    assert!(process_line(&store, &test_engine(), line).is_empty());
}

/// Even a notification naming a real tool stays silent — the absence of `id`
/// decides, not the method.
#[test]
fn test_tool_notification_gets_no_response() {
    let store = test_store();
    let line = r#"{"jsonrpc":"2.0","method":"keel/compile","params":{"files":[]}}"#;
    assert!(process_line(&store, &test_engine(), line).is_empty());
}

/// JSON-RPC 2.0: a *present* `"id": null` is a request id ("String, Number,
/// or NULL value if included"), so the message is a request, not a
/// notification — it MUST be answered, echoing the null id.
#[test]
fn test_explicit_null_id_is_answered() {
    let store = test_store();
    let line = r#"{"jsonrpc":"2.0","method":"initialize","id":null}"#;
    let resp = parse_response(&process_line(&store, &test_engine(), line));
    assert!(resp["id"].is_null());
    assert_eq!(resp["result"]["serverInfo"]["name"], "keel");
}

/// Malformed input still gets a parse-error response — the notification rule
/// cannot apply to a message we could not parse.
#[test]
fn test_parse_error_still_answered() {
    let store = test_store();
    let resp = parse_response(&process_line(&store, &test_engine(), "{not json"));
    assert_eq!(resp["error"]["code"], -32700);
    assert!(resp["id"].is_null());
}

// --- initialize protocol version negotiation (issue #27, bug 3) ---

#[test]
fn test_initialize_echoes_client_protocol_version() {
    let store = test_store();
    let params = serde_json::json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": { "name": "test", "version": "1" }
    });
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("initialize", Some(params)),
    ));
    assert_eq!(resp["result"]["protocolVersion"], "2025-06-18");
}

#[test]
fn test_initialize_falls_back_when_version_absent() {
    let store = test_store();
    let params = serde_json::json!({ "capabilities": {} });
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("initialize", Some(params)),
    ));
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
}

// --- tools/call routing (issue #27, bug 2) ---

#[test]
fn test_tools_call_compile() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &tools_call(
            "keel/compile",
            Some(serde_json::json!({"files": ["src/main.rs"]})),
        ),
    ));
    let payload = unwrap_tool_result(&resp);
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["files_analyzed"][0], "src/main.rs");
}

#[test]
fn test_tools_call_discover() {
    let store = store_with_node();
    let engine = engine_with_node();
    let resp = parse_response(&process_line(
        &store,
        &engine,
        &tools_call(
            "keel/discover",
            Some(serde_json::json!({"hash": "a7Bx3kM9f2Q"})),
        ),
    ));
    let payload = unwrap_tool_result(&resp);
    assert_eq!(payload["target"]["name"], "doStuff");
    assert_eq!(payload["target"]["hash"], "a7Bx3kM9f2Q");
}

#[test]
fn test_tools_call_where() {
    let store = store_with_node();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &tools_call(
            "keel/where",
            Some(serde_json::json!({"hash": "a7Bx3kM9f2Q"})),
        ),
    ));
    let payload = unwrap_tool_result(&resp);
    assert_eq!(payload["file"], "src/lib.rs");
    assert_eq!(payload["line_start"], 10);
}

#[test]
fn test_tools_call_explain() {
    let store = store_with_edges();
    let engine = engine_with_edges();
    let resp = parse_response(&process_line(
        &store,
        &engine,
        &tools_call(
            "keel/explain",
            Some(serde_json::json!({"error_code": "E001", "hash": "targetHash01"})),
        ),
    ));
    let payload = unwrap_tool_result(&resp);
    assert_eq!(payload["error_code"], "E001");
    assert_eq!(payload["hash"], "targetHash01");
    assert!(payload["resolution_chain"].is_array());
}

#[test]
fn test_tools_call_map() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &tools_call("keel/map", Some(serde_json::json!({"format": "llm"}))),
    ));
    let payload = unwrap_tool_result(&resp);
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["format"], "llm");
}

#[test]
fn test_tools_call_search() {
    let store = store_with_module_and_node();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &tools_call("keel/search", Some(serde_json::json!({"query": "doStuff"}))),
    ));
    let payload = unwrap_tool_result(&resp);
    assert_eq!(payload["count"], 1);
    assert_eq!(payload["results"][0]["name"], "doStuff");
}

#[test]
fn test_tools_call_check() {
    let store = store_with_node();
    let engine = engine_with_node();
    let resp = parse_response(&process_line(
        &store,
        &engine,
        &tools_call(
            "keel/check",
            Some(serde_json::json!({"hash": "a7Bx3kM9f2Q"})),
        ),
    ));
    let payload = unwrap_tool_result(&resp);
    assert_eq!(payload["target"]["hash"], "a7Bx3kM9f2Q");
}

#[test]
fn test_tools_call_context() {
    let store = store_with_module_and_node();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &tools_call(
            "keel/context",
            Some(serde_json::json!({"file": "src/lib.rs"})),
        ),
    ));
    let payload = unwrap_tool_result(&resp);
    assert_eq!(payload["command"], "context");
    assert_eq!(payload["file"], "src/lib.rs");
}

/// Every tool in the manifest must actually be reachable through `tools/call`
/// — a name in `tools/list` that routes nowhere is what clients hit first.
#[test]
fn test_every_advertised_tool_is_callable() {
    let store = test_store();
    let listed = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("tools/list", None),
    ));
    let tools = listed["result"]["tools"].as_array().unwrap().clone();
    assert_eq!(tools.len(), 16);

    for tool in &tools {
        let name = tool["name"].as_str().unwrap();
        let resp = parse_response(&process_line(
            &store,
            &test_engine(),
            &tools_call(name, Some(serde_json::json!({}))),
        ));
        // Arguments are intentionally empty, so a tool may reject them with
        // -32602. What must never happen is "unknown tool" or "method not found".
        if let Some(code) = resp["error"]["code"].as_i64() {
            assert_ne!(code, -32601, "{name} is advertised but not routed");
            let message = resp["error"]["message"].as_str().unwrap_or_default();
            assert!(
                !message.contains("Unknown tool"),
                "{name} is advertised but not routed: {message}"
            );
        }
    }
}

#[test]
fn test_tools_call_unknown_tool_is_invalid_params() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &tools_call("keel/nonexistent", Some(serde_json::json!({}))),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Unknown tool"));
}

#[test]
fn test_tools_call_missing_name_is_invalid_params() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("tools/call", Some(serde_json::json!({"arguments": {}}))),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("name"));
}

/// Per MCP, tool EXECUTION failures are reported in-band as a
/// `CallToolResult` with `isError: true` (so the model can read the message
/// and recover); JSON-RPC errors are reserved for protocol faults. The
/// legacy direct-method path still surfaces the raw JSON-RPC error.
#[test]
fn test_tools_call_reports_handler_error_in_band() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &tools_call("keel/discover", Some(serde_json::json!({"hash": "nope"}))),
    ));
    assert!(resp["error"].is_null(), "no protocol-level error expected");
    assert_eq!(resp["result"]["isError"], true);
    assert!(resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("not found"));
}

/// The pre-existing direct-method calling convention keeps working so older
/// integrations do not break.
#[test]
fn test_legacy_direct_method_still_works() {
    let store = store_with_node();
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/where", Some(params)),
    ));
    // Legacy path returns the payload directly, with no CallToolResult wrapper.
    assert_eq!(resp["result"]["file"], "src/lib.rs");
    assert!(resp["result"]["content"].is_null());
}

/// End-to-end replay of the issue's reproduction script.
#[test]
fn test_issue_27_handshake_sequence() {
    let store = test_store();
    let engine = test_engine();

    let init = parse_response(&process_line(
        &store,
        &engine,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
    ));
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["protocolVersion"], "2024-11-05");

    let notification = process_line(
        &store,
        &engine,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );
    assert!(notification.is_empty(), "notification must be silent");

    let list = parse_response(&process_line(
        &store,
        &engine,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    ));
    assert_eq!(list["id"], 2);
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 16);

    let call = parse_response(&process_line(
        &store,
        &engine,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"keel/compile","arguments":{"files":[]}}}"#,
    ));
    assert_eq!(call["id"], 3);
    assert_eq!(unwrap_tool_result(&call)["status"], "ok");
}
