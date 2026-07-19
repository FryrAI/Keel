//! Tests for `EnforcementEngine::focus` — the minimal-context builder.
//!
//! Graph: caller2 -> caller1 -> target -> callee, each in its own file.

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};

use crate::engine::EnforcementEngine;

fn node(id: u64, hash: &str, name: &str, file: &str, line: u32) -> GraphNode {
    GraphNode {
        id,
        hash: hash.to_string(),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: format!("fn {name}()"),
        file_path: file.to_string(),
        line_start: line,
        line_end: line + 5,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

fn module_node(id: u64, hash: &str, file: &str) -> GraphNode {
    let mut n = node(id, hash, "module", file, 1);
    n.kind = NodeKind::Module;
    n
}

fn call_edge(id: u64, src: u64, tgt: u64) -> GraphEdge {
    GraphEdge {
        id,
        source_id: src,
        target_id: tgt,
        kind: EdgeKind::Calls,
        file_path: "x".to_string(),
        line: 1,
        confidence: 1.0,
    }
}

fn fixture() -> EnforcementEngine {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&node(1, "caller1xxxx", "caller1", "src/caller1.rs", 10))
        .unwrap();
    store
        .insert_node(&node(2, "targetxxxxx", "target", "src/target.rs", 20))
        .unwrap();
    store
        .insert_node(&node(3, "calleexxxxx", "callee", "src/callee.rs", 30))
        .unwrap();
    store
        .insert_node(&node(4, "caller2xxxx", "caller2", "src/caller2.rs", 40))
        .unwrap();
    store
        .update_edges(vec![
            EdgeChange::Add(call_edge(1, 4, 1)), // caller2 -> caller1
            EdgeChange::Add(call_edge(2, 1, 2)), // caller1 -> target
            EdgeChange::Add(call_edge(3, 2, 3)), // target  -> callee
        ])
        .unwrap();
    EnforcementEngine::new(Box::new(store))
}

#[test]
fn focus_collects_transitive_callers_and_direct_callees() {
    let engine = fixture();
    let result = engine.focus("targetxxxxx", 2).unwrap();

    // Callers (at risk): caller1 at d=1, caller2 at d=2.
    let caller_names: Vec<&str> = result.callers.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(caller_names, vec!["caller1", "caller2"]);
    assert_eq!(result.callers[0].distance, 1);
    assert_eq!(result.callers[1].distance, 2);

    // Files include target, both callers, and the direct callee.
    let paths: Vec<&str> = result.files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"src/target.rs"));
    assert!(paths.contains(&"src/callee.rs"));
    assert!(paths.contains(&"src/caller1.rs"));
    assert!(paths.contains(&"src/caller2.rs"));
}

#[test]
fn focus_depth_is_honored() {
    let engine = fixture();
    let result = engine.focus("targetxxxxx", 1).unwrap();
    let caller_names: Vec<&str> = result.callers.iter().map(|s| s.name.as_str()).collect();
    // depth 1: only the direct caller, not the transitive caller2.
    assert_eq!(caller_names, vec!["caller1"]);
}

#[test]
fn focus_read_order_is_dependencies_first() {
    let engine = fixture();
    let result = engine.focus("targetxxxxx", 2).unwrap();
    // Dependencies (callee) precede the target, which precedes callers.
    let order = &result.read_order;
    let pos = |p: &str| order.iter().position(|x| x == p).unwrap();
    assert!(pos("src/callee.rs") < pos("src/target.rs"));
    assert!(pos("src/target.rs") < pos("src/caller1.rs"));
    assert!(pos("src/caller1.rs") < pos("src/caller2.rs"));
}

#[test]
fn focus_files_ranked_target_first() {
    let engine = fixture();
    let result = engine.focus("targetxxxxx", 2).unwrap();
    // Ranked by graph distance: the target file (distance 0) ranks first.
    assert_eq!(result.files[0].path, "src/target.rs");
    assert_eq!(result.files[0].distance, 0);
    assert_eq!(result.files[0].role, crate::types::Relation::Target);
    // The role serializes to the unchanged lowercase wire string.
    assert_eq!(result.files[0].role.as_str(), "target");
}

#[test]
fn focus_file_mode_resolves_symbols() {
    let engine = fixture();
    let result = engine.focus("src/target.rs", 2).unwrap();
    // Same graph as hash mode: target file present, caller1 at risk.
    assert!(result.callers.iter().any(|s| s.name == "caller1"));
    assert!(result.files.iter().any(|f| f.path == "src/target.rs"));
}

#[test]
fn focus_file_mode_resolves_absolute_path() {
    // An editor may send an absolute path while the graph stored a relative
    // one. focus must route through the same path-flexible lookup discover
    // uses (nodes_in_file_flex), so a suffix-matching absolute path still
    // resolves the file's symbols. That lookup enumerates module nodes to match
    // by suffix, so the store needs one (a real graph always has a module node
    // per file). Before FIX 6b the exact-only lookup returned nothing here.
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&module_node(1, "modtarget001", "src/target.rs"))
        .unwrap();
    store
        .insert_node(&node(2, "targetxxxxx", "target", "src/target.rs", 20))
        .unwrap();
    store
        .insert_node(&node(3, "caller1xxxx", "caller1", "src/caller1.rs", 10))
        .unwrap();
    store
        .update_edges(vec![EdgeChange::Add(call_edge(1, 3, 2))]) // caller1 -> target
        .unwrap();
    let engine = EnforcementEngine::new(Box::new(store));

    let result = engine
        .focus("/abs/workspace/src/target.rs", 2)
        .expect("absolute path should resolve via suffix match");
    assert!(result.files.iter().any(|f| f.path == "src/target.rs"));
    assert!(result.callers.iter().any(|s| s.name == "caller1"));
}

#[test]
fn focus_unknown_target_is_none() {
    let engine = fixture();
    assert!(engine.focus("nope", 2).is_none());
}

#[test]
fn focus_relation_serializes_lowercase() {
    // The Relation enum must serialize to the exact lowercase wire strings the
    // extension/server contract expects.
    use crate::types::Relation;
    assert_eq!(
        serde_json::to_string(&Relation::Target).unwrap(),
        "\"target\""
    );
    assert_eq!(
        serde_json::to_string(&Relation::Callee).unwrap(),
        "\"callee\""
    );
    assert_eq!(
        serde_json::to_string(&Relation::Caller).unwrap(),
        "\"caller\""
    );
}
