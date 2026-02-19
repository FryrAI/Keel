// Tests for E004 function removed detection (Spec 006 - Enforcement Engine)
use keel_core::hash::compute_hash;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind};
use keel_enforce::violations::check_removed_functions;
use keel_parsers::resolver::{Definition, FileIndex};

use crate::common::in_memory_store;

fn make_node(id: u64, name: &str, sig: &str, body: &str, file: &str, line: u32) -> GraphNode {
    GraphNode {
        id,
        hash: compute_hash(sig, body, ""),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: sig.to_string(),
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

fn make_edge(id: u64, src: u64, tgt: u64) -> GraphEdge {
    GraphEdge {
        id,
        source_id: src,
        target_id: tgt,
        kind: EdgeKind::Calls,
        file_path: "caller.py".to_string(),
        line: 10,
        confidence: 1.0,
    }
}

#[test]
fn test_e004_deleted_function_with_callers() {
    let mut store = in_memory_store();
    let target = make_node(
        1,
        "process",
        "def process(data: str)",
        "return data",
        "utils.py",
        1,
    );
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    for i in 2..=3 {
        let caller = make_node(
            i,
            &format!("caller_{i}"),
            &format!("def caller_{i}()"),
            "process('x')",
            "main.py",
            i as u32 * 10,
        );
        store.update_nodes(vec![NodeChange::Add(caller)]).unwrap();
        store
            .update_edges(vec![EdgeChange::Add(make_edge(i, i, 1))])
            .unwrap();
    }

    let file = FileIndex {
        file_path: "utils.py".to_string(),
        content_hash: 0,
        definitions: vec![],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let violations = check_removed_functions(&file, &store);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].code, "E004");
    assert_eq!(violations[0].severity, "ERROR");
    assert_eq!(violations[0].category, "function_removed");
    assert_eq!(violations[0].affected.len(), 2);
}

#[test]
fn test_e004_deleted_function_no_callers() {
    let mut store = in_memory_store();
    let target = make_node(1, "unused", "def unused()", "pass", "utils.py", 1);
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let file = FileIndex {
        file_path: "utils.py".to_string(),
        content_hash: 0,
        definitions: vec![],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let violations = check_removed_functions(&file, &store);
    assert!(violations.is_empty());
}

#[test]
fn test_e004_includes_original_location() {
    let mut store = in_memory_store();
    let target = make_node(
        1,
        "compute",
        "def compute(x: int)",
        "return x*2",
        "src/calc.py",
        42,
    );
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let caller = make_node(2, "app", "def app()", "compute(5)", "main.py", 10);
    store.update_nodes(vec![NodeChange::Add(caller)]).unwrap();
    store
        .update_edges(vec![EdgeChange::Add(make_edge(1, 2, 1))])
        .unwrap();

    let file = FileIndex {
        file_path: "src/calc.py".to_string(),
        content_hash: 0,
        definitions: vec![],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let violations = check_removed_functions(&file, &store);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].file, "src/calc.py");
    assert_eq!(violations[0].line, 42);
}

#[test]
fn test_e004_fix_hint_present() {
    let mut store = in_memory_store();
    let target = make_node(1, "getData", "def getData()", "return {}", "api.py", 1);
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let caller = make_node(2, "handler", "def handler()", "getData()", "routes.py", 5);
    store.update_nodes(vec![NodeChange::Add(caller)]).unwrap();
    store
        .update_edges(vec![EdgeChange::Add(make_edge(1, 2, 1))])
        .unwrap();

    let file = FileIndex {
        file_path: "api.py".to_string(),
        content_hash: 0,
        definitions: vec![],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let violations = check_removed_functions(&file, &store);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].fix_hint.is_some());
    assert!(violations[0].fix_hint.as_ref().unwrap().contains("getData"));
}

#[test]
fn test_e004_function_still_exists_no_violation() {
    let mut store = in_memory_store();
    let target = make_node(1, "keep_me", "def keep_me()", "pass", "lib.py", 1);
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let caller = make_node(2, "user", "def user()", "keep_me()", "main.py", 1);
    store.update_nodes(vec![NodeChange::Add(caller)]).unwrap();
    store
        .update_edges(vec![EdgeChange::Add(make_edge(1, 2, 1))])
        .unwrap();

    let def = Definition {
        name: "keep_me".to_string(),
        kind: NodeKind::Function,
        signature: "def keep_me()".to_string(),
        file_path: "lib.py".to_string(),
        line_start: 1,
        line_end: 3,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        body_text: "pass".to_string(),
    };
    let file = FileIndex {
        file_path: "lib.py".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let violations = check_removed_functions(&file, &store);
    assert!(violations.is_empty());
}
