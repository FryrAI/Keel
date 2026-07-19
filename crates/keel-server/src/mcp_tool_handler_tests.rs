//! MCP tool-handler tests: check, fix, search, name, context, analyze.
//!
//! Split from `mcp_tests.rs` to keep files under the size cap; shares
//! fixtures with the parent module.

use super::*;

#[test]
fn test_check_existing_node() {
    let store = store_with_node();
    let engine = engine_with_node();
    let params = serde_json::json!({"hash": "a7Bx3kM9f2Q"});
    let resp = parse_response(&process_line(
        &store,
        &engine,
        &rpc("keel/check", Some(params)),
    ));
    let result = &resp["result"];
    assert_eq!(result["target"]["hash"], "a7Bx3kM9f2Q");
    assert_eq!(result["target"]["name"], "doStuff");
    assert!(result["risk"].is_object());
    assert!(result["risk"]["level"].is_string());
}

#[test]
fn test_check_not_found() {
    let store = test_store();
    let params = serde_json::json!({"hash": "nonexistent"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/check", Some(params)),
    ));
    assert_eq!(resp["error"]["code"], -32602);
}

#[test]
fn test_check_missing_hash() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/check", None),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("hash"));
}

// --- keel/fix tests ---

#[test]
fn test_fix_empty_files() {
    let store = test_store();
    let params = serde_json::json!({"files": []});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/fix", Some(params)),
    ));
    let result = &resp["result"];
    assert_eq!(result["command"], "fix");
    assert_eq!(result["violations_addressed"], 0);
}

#[test]
fn test_fix_no_params() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/fix", None),
    ));
    let result = &resp["result"];
    assert_eq!(result["command"], "fix");
}

// --- keel/search tests ---

#[test]
fn test_search_no_results() {
    let store = test_store();
    let params = serde_json::json!({"query": "nonexistent"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/search", Some(params)),
    ));
    let result = &resp["result"];
    assert_eq!(result["count"], 0);
    assert!(result["results"].as_array().unwrap().is_empty());
}

#[test]
fn test_search_finds_node() {
    let store = store_with_module_and_node();
    let params = serde_json::json!({"query": "doStuff"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/search", Some(params)),
    ));
    let result = &resp["result"];
    assert_eq!(result["count"], 1);
    assert_eq!(result["results"][0]["name"], "doStuff");
}

#[test]
fn test_search_case_insensitive() {
    let store = store_with_module_and_node();
    let params = serde_json::json!({"query": "dostuff"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/search", Some(params)),
    ));
    assert_eq!(resp["result"]["count"], 1);
}

#[test]
fn test_search_missing_query() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/search", None),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("query"));
}

#[test]
fn test_search_with_kind_filter() {
    let store = store_with_module_and_node();
    let params = serde_json::json!({"query": "doStuff", "kind": "class"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/search", Some(params)),
    ));
    // doStuff is a function, not a class — should not match
    assert_eq!(resp["result"]["count"], 0);
}

// --- keel/name tests ---

#[test]
fn test_name_missing_description() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/name", None),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("description"));
}

#[test]
fn test_name_empty_graph() {
    let store = test_store();
    let params = serde_json::json!({"description": "handle user authentication"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/name", Some(params)),
    ));
    let result = &resp["result"];
    assert_eq!(result["command"], "name");
    // Empty graph yields no suggestions
    assert!(result["suggestions"].as_array().unwrap().is_empty());
}

// --- keel/context tests ---

#[test]
fn test_context_missing_file_param() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/context", None),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("file"));
}

#[test]
fn test_context_file_not_in_graph() {
    let store = test_store();
    let params = serde_json::json!({"file": "nonexistent.rs"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/context", Some(params)),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("No graph data"));
}

#[test]
fn test_context_single_node_no_edges() {
    let store = store_with_module_and_node();
    let params = serde_json::json!({"file": "src/lib.rs"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/context", Some(params)),
    ));
    let result = &resp["result"];
    assert_eq!(result["command"], "context");
    assert_eq!(result["file"], "src/lib.rs");

    let symbols = result["symbols"].as_array().unwrap();
    // Should contain doStuff but NOT the module node
    assert_eq!(symbols.len(), 1, "should have exactly 1 non-module symbol");
    assert_eq!(symbols[0]["name"], "doStuff");
    assert_eq!(symbols[0]["kind"], "function");
    assert!(symbols[0]["callers"].as_array().unwrap().is_empty());
    assert!(symbols[0]["callees"].as_array().unwrap().is_empty());
}

#[test]
fn test_context_with_edges_filters_external_only() {
    // Build a store with 3 nodes across 2 files + edges between them
    let mut store = SqliteGraphStore::in_memory().unwrap();

    // File A: two functions (fn_a1 calls fn_a2 internally)
    let fn_a1 = GraphNode {
        id: 10,
        hash: "hashA1".into(),
        kind: NodeKind::Function,
        name: "fn_a1".into(),
        signature: "fn fn_a1()".into(),
        file_path: "src/a.rs".into(),
        line_start: 1,
        line_end: 10,
        is_public: true,
        ..make_test_node()
    };
    let fn_a2 = GraphNode {
        id: 11,
        hash: "hashA2".into(),
        kind: NodeKind::Function,
        name: "fn_a2".into(),
        signature: "fn fn_a2()".into(),
        file_path: "src/a.rs".into(),
        line_start: 12,
        line_end: 20,
        is_public: false,
        ..make_test_node()
    };
    // File B: one function that calls fn_a1
    let fn_b1 = GraphNode {
        id: 20,
        hash: "hashB1".into(),
        kind: NodeKind::Function,
        name: "fn_b1".into(),
        signature: "fn fn_b1()".into(),
        file_path: "src/b.rs".into(),
        line_start: 1,
        line_end: 10,
        is_public: true,
        ..make_test_node()
    };

    store.insert_node(&fn_a1).unwrap();
    store.insert_node(&fn_a2).unwrap();
    store.insert_node(&fn_b1).unwrap();

    // Edge: fn_a1 -> fn_a2 (internal to a.rs)
    // Edge: fn_b1 -> fn_a1 (external: b.rs calls a.rs)
    store
        .update_edges(vec![
            EdgeChange::Add(GraphEdge {
                id: 100,
                source_id: 10,
                target_id: 11,
                kind: EdgeKind::Calls,
                file_path: "src/a.rs".into(),
                line: 5,
                confidence: 1.0,
            }),
            EdgeChange::Add(GraphEdge {
                id: 101,
                source_id: 20,
                target_id: 10,
                kind: EdgeKind::Calls,
                file_path: "src/b.rs".into(),
                line: 5,
                confidence: 1.0,
            }),
        ])
        .unwrap();

    let shared = Arc::new(Mutex::new(store));
    let params = serde_json::json!({"file": "src/a.rs"});
    let resp = parse_response(&process_line(
        &shared,
        &test_engine(),
        &rpc("keel/context", Some(params)),
    ));
    let result = &resp["result"];
    let symbols = result["symbols"].as_array().unwrap();

    assert_eq!(symbols.len(), 2, "should have fn_a1 and fn_a2");

    // fn_a1 should have external caller fn_b1, but NOT internal callee fn_a2
    let a1 = symbols.iter().find(|s| s["name"] == "fn_a1").unwrap();
    let a1_callers = a1["callers"].as_array().unwrap();
    assert_eq!(a1_callers.len(), 1, "fn_a1 should have 1 external caller");
    assert_eq!(a1_callers[0]["name"], "fn_b1");
    assert_eq!(a1_callers[0]["file"], "src/b.rs");

    // fn_a1's outgoing edge to fn_a2 is internal — should be excluded
    let a1_callees = a1["callees"].as_array().unwrap();
    assert!(
        a1_callees.is_empty(),
        "fn_a1 internal callee fn_a2 should be filtered out"
    );

    // fn_a2 should have no external edges at all
    let a2 = symbols.iter().find(|s| s["name"] == "fn_a2").unwrap();
    assert!(a2["callers"].as_array().unwrap().is_empty());
    assert!(a2["callees"].as_array().unwrap().is_empty());
}

#[test]
fn test_context_json_has_required_fields() {
    let store = store_with_module_and_node();
    let params = serde_json::json!({"file": "src/lib.rs"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/context", Some(params)),
    ));
    let result = &resp["result"];

    assert!(result["version"].is_string());
    assert_eq!(result["command"], "context");
    assert_eq!(result["file"], "src/lib.rs");
    assert!(result["symbols"].is_array());

    let sym = &result["symbols"][0];
    assert!(sym["name"].is_string());
    assert!(sym["hash"].is_string());
    assert!(sym["kind"].is_string());
    assert!(sym["line_start"].is_number());
    assert!(sym["line_end"].is_number());
    assert!(sym["is_public"].is_boolean());
    assert!(sym["signature"].is_string());
    assert!(sym["callers"].is_array());
    assert!(sym["callees"].is_array());
}

// --- keel/analyze tests ---

#[test]
fn test_analyze_missing_file() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/analyze", None),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("file"));
}

#[test]
fn test_analyze_file_not_in_graph() {
    let store = test_store();
    let params = serde_json::json!({"file": "nonexistent.rs"});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/analyze", Some(params)),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("No graph data"));
}

// --- keel/skeleton tests ---

#[test]
fn test_skeleton_missing_file() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/skeleton", None),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("file"));
}

/// The MCP `keel/skeleton` tool must return exactly what the CLI produces for
/// the same file — both call `keel_enforce::skeleton::build_skeleton`, so the
/// serialized result (CLI `--json`) and the MCP payload must be equal.
#[test]
fn test_skeleton_mcp_matches_cli() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("sample.ts");
    std::fs::write(
        &file,
        "import { z } from './z';\n\
         export function pub(a: number): string { return `${a}`; }\n\
         function priv_helper(): void {}\n",
    )
    .unwrap();
    let _abs = file.to_string_lossy().to_string();

    // CLI path: JsonFormatter emits `serde_json::to_string_pretty(&SkeletonResult)`.
    let root_for_expected = std::fs::canonicalize(dir.path()).unwrap();
    let expected = keel_enforce::skeleton::build_skeleton(
        &root_for_expected,
        std::path::Path::new(
            &std::fs::canonicalize(&file)
                .unwrap()
                .to_string_lossy()
                .to_string(),
        ),
        &std::fs::read_to_string(&file).unwrap(),
        false,
        false,
    )
    .unwrap();
    let expected_value = serde_json::to_value(&expected).unwrap();

    let store = test_store();
    // The MCP edge confines the file param to the served root, so anchor the
    // request at the temp dir and pass the file relative to it.
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let params = serde_json::json!({ "file": "sample.ts" });
    let resp = parse_response(&crate::mcp::process_line_with_root(
        &store,
        &test_engine(),
        &root,
        &rpc("keel/skeleton", Some(params)),
    ));

    assert_eq!(resp["result"], expected_value);
    assert_eq!(resp["result"]["command"], "skeleton");
    // Signature-only: the exported function is present, private one filtered out.
    let names: Vec<&str> = resp["result"]["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"pub"));
    assert!(!names.contains(&"priv_helper"));
}

// --- keel/focus tests ---

#[test]
fn test_focus_missing_target() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/focus", None),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("target"));
}

#[test]
fn test_focus_returns_context_for_node() {
    // Graph: caller -> target, in separate files.
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&make_node(
            1,
            "targethashh",
            "target",
            "fn target()",
            "src/target.rs",
        ))
        .unwrap();
    store
        .insert_node(&make_node(
            2,
            "callerhashh",
            "caller",
            "fn caller()",
            "src/caller.rs",
        ))
        .unwrap();
    store
        .update_edges(vec![EdgeChange::Add(GraphEdge {
            id: 1,
            source_id: 2,
            target_id: 1,
            kind: EdgeKind::Calls,
            file_path: "src/caller.rs".into(),
            line: 5,
            confidence: 1.0,
        })])
        .unwrap();
    let engine: SharedEngine = Arc::new(Mutex::new(EnforcementEngine::new(Box::new(store))));

    let params = serde_json::json!({ "target": "targethashh", "depth": 2 });
    let resp = parse_response(&process_line(
        &test_store(),
        &engine,
        &rpc("keel/focus", Some(params)),
    ));
    let result = &resp["result"];
    assert_eq!(result["command"], "focus");
    assert_eq!(result["target"], "targethashh");
    // caller is a symbol at risk.
    assert_eq!(result["callers"][0]["name"], "caller");
    // read order lists files.
    assert!(result["read_order"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p == "src/target.rs"));
}

// --- keel/validate-plan tests ---

#[test]
fn test_validate_plan_detects_removal_risk() {
    let store = Arc::new(Mutex::new(populated_edge_store()));
    let params = serde_json::json!({"plan": "Step 1: Remove handleRequest entirely."});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/validate-plan", Some(params)),
    ));
    let result = &resp["result"];
    assert_eq!(result["command"], "validate-plan");
    assert_eq!(result["unrecognized"], false);
    let actions = result["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0]["action"], "remove");
    assert_eq!(actions[0]["symbol"], "handleRequest");
    assert_eq!(actions[0]["risk"], "HIGH");
    assert!(actions[0]["caller_count"].as_u64().unwrap() >= 1);
}

#[test]
fn test_validate_plan_nonsense_unrecognized() {
    let store = test_store();
    let params = serde_json::json!({"plan": "Water the plants and take a nap."});
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/validate-plan", Some(params)),
    ));
    assert_eq!(resp["result"]["unrecognized"], true);
    assert!(resp["result"]["actions"].as_array().unwrap().is_empty());
}

#[test]
fn test_validate_plan_missing_param() {
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/validate-plan", None),
    ));
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("plan"));
}

// --- keel/checkpoint tests ---

#[test]
fn test_checkpoint_returns_shaped_result() {
    // Runs git in the crate cwd; with an empty in-memory store and no matching
    // files it returns a well-formed, empty checkpoint. We assert only shape.
    let store = test_store();
    let resp = parse_response(&process_line(
        &store,
        &test_engine(),
        &rpc("keel/checkpoint", None),
    ));
    let result = &resp["result"];
    assert_eq!(result["command"], "checkpoint");
    assert!(result["files"].is_array());
    assert!(result["violations"].is_array());
    assert!(result["commits"].is_array());
    assert!(result["affected_callers"].is_array());
}
