//! MCP query-tool tests: discover, where, explain, map.
//!
//! Split from `mcp_tests.rs` to keep files under the size cap; shares
//! fixtures with the parent module.

use super::*;

#[test]
fn test_discover_existing_node() {
    let store = store_with_node();
    let engine = engine_with_node();
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(
        &store,
        &engine,
        &rpc("keel/discover", Some(params)),
    ));
    let result = &resp["result"];
    assert_eq!(result["target"]["name"], "doStuff");
    assert_eq!(result["target"]["hash"], "a7Bx3kM9f2Q");
}

#[test]
fn test_discover_not_found() {
    let store = test_store();
    let params = serde_json::json!({"hash": "nonexistent"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/discover", Some(params)),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not found"));
}

#[test]
fn test_discover_missing_hash() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/discover", None),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("hash"));
}

#[test]
fn test_discover_with_edges() {
    let store = store_with_edges();
    let engine = engine_with_edges();
    let params = serde_json::json!({"hash": "targetHash01"});
    let resp = parse_response(&process_line(
        &store,
        &engine,
        &rpc("keel/discover", Some(params)),
    ));
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
    let resp = parse_response(&process_line(
        &store,
        &engine,
        &rpc("keel/discover", Some(params)),
    ));
    let result = &resp["result"];
    assert!(result["upstream"].as_array().unwrap().is_empty());
    assert!(result["downstream"].as_array().unwrap().is_empty());
}

#[test]
fn test_where_existing_node() {
    let store = store_with_node();
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/where", Some(params)),
    ));
    assert_eq!(resp["result"]["file"], "src/lib.rs");
    assert_eq!(resp["result"]["line_start"], 10);
    assert_eq!(resp["result"]["line_end"], 20);
    // The hardcoded `stale` field was dropped as dead metadata.
    assert!(resp["result"].get("stale").is_none());
}

#[test]
fn test_where_not_found() {
    let store = test_store();
    let params = serde_json::json!({"hash": "nope"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/where", Some(params)),
    ));
    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32602);
}

#[test]
fn test_where_missing_hash() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/where", None),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("hash"));
}

#[test]
fn test_explain_existing_node() {
    let store = store_with_edges();
    let engine = engine_with_edges();
    let params = serde_json::json!({"error_code": "E001", "hash": "targetHash01"});
    let resp = parse_response(&process_line(
        &store,
        &engine,
        &rpc("keel/explain", Some(params)),
    ));
    let result = &resp["result"];
    assert_eq!(result["error_code"], "E001");
    assert_eq!(result["hash"], "targetHash01");
    assert!(result["summary"]
        .as_str()
        .unwrap()
        .contains("handleRequest"));
    assert_eq!(result["resolution_tier"], "tree-sitter");
}

/// The chain must be derived from real graph edges, not a fabricated
/// single-step "lookup" placeholder (issue #27).
#[test]
fn test_explain_chain_comes_from_real_edges() {
    let store = store_with_edges();
    let engine = engine_with_edges();
    let params = serde_json::json!({"error_code": "E001", "hash": "targetHash01"});
    let resp = parse_response(&process_line(
        &store,
        &engine,
        &rpc("keel/explain", Some(params)),
    ));

    let chain = resp["result"]["resolution_chain"].as_array().unwrap();
    // targetHash01 has one inbound and one outbound call edge.
    assert_eq!(chain.len(), 2, "chain should mirror the node's two edges");
    for step in chain {
        assert_eq!(step["kind"], "call");
        assert_ne!(
            step["kind"], "lookup",
            "synthetic 'lookup' step must not reappear"
        );
    }
}

/// MCP explain must agree with the engine (and therefore the CLI and HTTP
/// handlers) rather than computing its own confidence and chain.
#[test]
fn test_explain_matches_engine_output() {
    let store = store_with_edges();
    let engine = engine_with_edges();
    let params = serde_json::json!({"error_code": "E001", "hash": "targetHash01"});
    let resp = parse_response(&process_line(
        &store,
        &engine,
        &rpc("keel/explain", Some(params)),
    ));

    let expected = engine
        .lock()
        .unwrap()
        .explain("E001", "targetHash01")
        .unwrap();
    let expected = serde_json::to_value(expected).unwrap();
    assert_eq!(resp["result"], expected);
}

#[test]
fn test_explain_not_found() {
    let store = test_store();
    let params = serde_json::json!({"hash": "nope"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/explain", Some(params)),
    ));
    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32602);
}

#[test]
fn test_explain_missing_hash() {
    let store = test_store();
    let params = serde_json::json!({"error_code": "E001"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/explain", Some(params)),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("hash"));
}

#[test]
fn test_explain_defaults_error_code() {
    let store = store_with_node();
    let engine = engine_with_node();
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(
        &store,
        &engine,
        &rpc("keel/explain", Some(params)),
    ));
    assert_eq!(resp["result"]["error_code"], "E001");
}

#[test]
fn test_map() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/map", None),
    ));
    assert_eq!(resp["result"]["status"], "ok");
    assert_eq!(resp["result"]["format"], "json");
}

#[test]
fn test_map_with_format() {
    let store = test_store();
    let params = serde_json::json!({"format": "llm"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/map", Some(params)),
    ));
    assert_eq!(resp["result"]["format"], "llm");
}

#[test]
fn test_map_file_scoped_uses_file_key() {
    // The file-scoped map takes `file` (renamed from `file_path`) and echoes
    // it back under the same `file` key that node entries use.
    let store = store_with_node();
    let params = serde_json::json!({"file": "src/lib.rs"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/map", Some(params)),
    ));
    let result = &resp["result"];
    assert_eq!(result["status"], "ok");
    assert_eq!(result["file"], "src/lib.rs");
    assert!(result.get("file_path").is_none());
    assert!(result["nodes"].as_array().is_some());
}

#[test]
fn test_map_with_scope() {
    let store = test_store();
    let params = serde_json::json!({"scope": ["auth", "payments"]});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/map", Some(params)),
    ));
    let scope = resp["result"]["scope"].as_array().unwrap();
    assert_eq!(scope.len(), 2);
    assert_eq!(scope[0], "auth");
    assert_eq!(scope[1], "payments");
}
