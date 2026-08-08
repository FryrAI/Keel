//! Extension ↔ HTTP-server contract tests.
//!
//! The VS Code extension (`extensions/vscode/src/extension.ts`) talks to
//! `keel serve --http` over a fixed set of routes and payload shapes. These
//! tests pin every route the extension calls — method, path, request body, and
//! the response fields the extension parses — so that server/extension drift
//! (GitHub issue #34) fails CI instead of failing silently at runtime.
//!
//! Each route below maps to a concrete extension feature:
//!   GET  /health                              — status bar / health poll
//!   POST /compile {"path"|"files"}            — compile-on-save, Compile cmd
//!   GET  /discover/{hash}                     — Discover command
//!   GET  /discover/{name}?file=&line=         — CodeLens counts, hover
//!   GET  /where/{hash}                        — Where command
//!   GET  /map?format=llm                      — Show Map command

use std::sync::{Arc, Mutex};

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind};
use keel_enforce::engine::EnforcementEngine;
use keel_server::http::{router, SharedEngine};

const API_MODULE_HASH: &str = "mod_api_hash0";
const HANDLE_REQUEST_HASH: &str = "fn_api_00009";
const API_FILE: &str = "src/api.rs";

fn func(id: u64, hash: &str, name: &str, line: u32) -> GraphNode {
    GraphNode {
        complexity: 0,
        is_trivial_wrapper: false,
        in_test_context: false,
        id,
        hash: hash.to_string(),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: format!("fn {}()", name),
        file_path: API_FILE.to_string(),
        line_start: line,
        line_end: line + 5,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 100,
        package: None,
    }
}

fn calls_edge(id: u64, source: u64, target: u64, line: u32) -> GraphEdge {
    GraphEdge {
        id,
        source_id: source,
        target_id: target,
        kind: EdgeKind::Calls,
        file_path: API_FILE.to_string(),
        line,
        confidence: 1.0,
    }
}

/// One module (`src/api.rs`) with `handle_request` calling two helpers, so the
/// discover routes have real callers/callees to report.
fn contract_engine() -> SharedEngine {
    let mut store = SqliteGraphStore::in_memory().unwrap();

    let module = GraphNode {
        complexity: 0,
        is_trivial_wrapper: false,
        in_test_context: false,
        id: 100,
        hash: API_MODULE_HASH.to_string(),
        kind: NodeKind::Module,
        name: "module_api".to_string(),
        signature: String::new(),
        file_path: API_FILE.to_string(),
        line_start: 1,
        line_end: 80,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    };

    let nodes = vec![
        NodeChange::Add(module),
        NodeChange::Add(func(9, HANDLE_REQUEST_HASH, "handle_request", 5)),
        NodeChange::Add(func(10, "fn_api_00010", "parse_body", 22)),
        NodeChange::Add(func(11, "fn_api_00011", "send_response", 40)),
    ];
    store.update_nodes(nodes).unwrap();

    let edges = vec![
        EdgeChange::Add(calls_edge(1, 9, 10, 8)),
        EdgeChange::Add(calls_edge(2, 9, 11, 12)),
    ];
    store.update_edges(edges).unwrap();

    Arc::new(Mutex::new(EnforcementEngine::new(Box::new(store))))
}

async fn get(uri: &str) -> (StatusCode, Vec<u8>, Option<String>) {
    let resp = router(contract_engine(), std::env::temp_dir())
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let body = to_bytes(resp.into_body(), 100_000).await.unwrap().to_vec();
    (status, body, content_type)
}

async fn post_json(uri: &str, body: &str) -> (StatusCode, Vec<u8>) {
    let resp = router(contract_engine(), std::env::temp_dir())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let body = to_bytes(resp.into_body(), 100_000).await.unwrap().to_vec();
    (status, body)
}

fn as_json(body: &[u8]) -> Value {
    serde_json::from_slice(body).expect("response body is not valid JSON")
}

#[tokio::test]
async fn health_reports_status_and_version() {
    let (status, body, _) = get("/health").await;
    assert_eq!(status, StatusCode::OK);
    let json = as_json(&body);
    assert_eq!(json["status"], "ok");
    assert!(json["version"].as_str().is_some_and(|v| !v.is_empty()));
}

#[tokio::test]
async fn compile_accepts_path_payload() {
    // The extension POSTs {"path": ...}; the pre-fix server required {"files"}
    // and rejected this outright. Guard the accept-both-shapes fix.
    let (status, body) = post_json("/compile", r#"{"path":"src/api.rs"}"#).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "compile must accept a {{path}} body"
    );
    let json = as_json(&body);
    assert!(json.get("errors").is_some_and(Value::is_array));
    assert!(json.get("warnings").is_some_and(Value::is_array));
    assert!(json.get("status").is_some());
}

#[tokio::test]
async fn compile_still_accepts_files_payload() {
    let (status, body) = post_json("/compile", r#"{"files":[]}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(as_json(&body)["status"], "ok");
}

#[tokio::test]
async fn discover_by_hash_returns_full_result() {
    // cmdDiscover dumps the raw body; it must stay the full DiscoverResult.
    let (status, body, _) = get(&format!("/discover/{}", HANDLE_REQUEST_HASH)).await;
    assert_eq!(status, StatusCode::OK);
    let json = as_json(&body);
    assert_eq!(json["target"]["hash"], HANDLE_REQUEST_HASH);
    assert_eq!(json["target"]["name"], "handle_request");
}

#[tokio::test]
async fn discover_by_name_returns_flat_codelens_shape() {
    // CodeLens + hover call /discover/{name}?file=&line= and read the flat
    // {hash, name, callers[], callees[], module_context} shape.
    let (status, body, _) = get(&format!(
        "/discover/handle_request?file={}&line=5",
        API_FILE
    ))
    .await;
    assert_eq!(status, StatusCode::OK);
    let json = as_json(&body);
    assert_eq!(json["hash"], HANDLE_REQUEST_HASH);
    assert_eq!(json["name"], "handle_request");
    assert!(json["callers"].is_array());
    let callees = json["callees"].as_array().expect("callees array");
    assert_eq!(callees.len(), 2, "handle_request calls two helpers");
    assert!(json["callees"][0]["name"].as_str().is_some());
    assert!(json.get("module_context").is_some());
}

#[tokio::test]
async fn discover_by_name_missing_symbol_is_404() {
    let (status, _, _) = get(&format!("/discover/no_such_fn?file={}&line=1", API_FILE)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn where_resolves_hash_to_file_and_line() {
    let (status, body, _) = get(&format!("/where/{}", HANDLE_REQUEST_HASH)).await;
    assert_eq!(status, StatusCode::OK);
    let json = as_json(&body);
    assert_eq!(json["file"], API_FILE);
    assert_eq!(json["line"], 5);
}

#[tokio::test]
async fn map_llm_returns_plain_text_listing() {
    let (status, body, content_type) = get("/map?format=llm").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        content_type
            .as_deref()
            .unwrap_or("")
            .starts_with("text/plain"),
        "llm map is dumped verbatim into an editor doc, so it must be text"
    );
    let text = String::from_utf8(body).unwrap();
    // Rendered by the shared keel-output LLM map formatter (same as `keel
    // map --llm`): header is "MAP nodes=... modules=..." and each module is
    // listed on its own MODULE line.
    assert!(text.starts_with("MAP nodes="), "got: {text}");
    assert!(text.contains("modules="));
    assert!(text.contains(API_FILE));
}

#[tokio::test]
async fn map_json_returns_module_summary() {
    let (status, body, _) = get("/map").await;
    assert_eq!(status, StatusCode::OK);
    let json = as_json(&body);
    // /map now serializes the shared MapResult (same shape as `keel map
    // --json` and MCP `keel/map`): counts live under `summary`, and each
    // module carries its `path`.
    assert_eq!(json["summary"]["modules"], 1);
    let modules = json["modules"].as_array().expect("modules array");
    assert!(modules.iter().any(|m| m["path"] == API_FILE));
}

#[tokio::test]
async fn search_finds_nodes_by_name() {
    let (status, body, _) = get("/search?q=handle").await;
    assert_eq!(status, StatusCode::OK);
    let json = as_json(&body);
    assert_eq!(json["query"], "handle");
    let results = json["results"].as_array().expect("results array");
    assert!(results.iter().any(|r| r["name"] == "handle_request"));
}
