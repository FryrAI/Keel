// Tests for E001 broken caller detection (Spec 006 - Enforcement Engine)
use keel_core::hash::compute_hash;
use keel_core::store::GraphStore;
use keel_core::types::{
    EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind, EdgeChange,
};
use keel_enforce::violations::check_broken_callers;
use keel_parsers::resolver::{Definition, FileIndex};

use crate::common::in_memory_store;

fn make_definition(name: &str, sig: &str, body: &str, file: &str, line: u32) -> Definition {
    Definition {
        name: name.to_string(),
        kind: NodeKind::Function,
        signature: sig.to_string(),
        file_path: file.to_string(),
        line_start: line,
        line_end: line + 5,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        body_text: body.to_string(),
    }
}

fn make_file_index(file: &str, defs: Vec<Definition>) -> FileIndex {
    FileIndex {
        file_path: file.to_string(),
        content_hash: 0,
        definitions: defs,
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }
}

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
fn test_e001_signature_change_breaks_callers() {
    let mut store = in_memory_store();
    let orig_node = make_node(1, "foo", "def foo(a: int)", "return a", "lib.py", 1);
    store.update_nodes(vec![NodeChange::Add(orig_node)]).unwrap();

    for i in 2..=4 {
        let caller = make_node(i, &format!("caller_{i}"), &format!("def caller_{i}()"), "foo(1)", "main.py", i as u32 * 10);
        store.update_nodes(vec![NodeChange::Add(caller)]).unwrap();
        store.update_edges(vec![EdgeChange::Add(make_edge(i, i, 1))]).unwrap();
    }

    let new_def = make_definition("foo", "def foo(a: int, b: str)", "return a + b", "lib.py", 1);
    let file = make_file_index("lib.py", vec![new_def]);

    let violations = check_broken_callers(&file, &store);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].code, "E001");
    assert_eq!(violations[0].severity, "ERROR");
    assert_eq!(violations[0].category, "broken_caller");
    assert_eq!(violations[0].affected.len(), 3);
}

#[test]
fn test_e001_includes_fix_hint() {
    let mut store = in_memory_store();
    let orig = make_node(1, "greet", "def greet(name: str)", "return name", "lib.py", 1);
    store.update_nodes(vec![NodeChange::Add(orig)]).unwrap();

    let caller = make_node(2, "main", "def main()", "greet('hi')", "main.py", 1);
    store.update_nodes(vec![NodeChange::Add(caller)]).unwrap();
    store.update_edges(vec![EdgeChange::Add(make_edge(1, 2, 1))]).unwrap();

    let new_def = make_definition("greet", "def greet(name: str, lang: str)", "return name + lang", "lib.py", 1);
    let file = make_file_index("lib.py", vec![new_def]);
    let violations = check_broken_callers(&file, &store);

    assert_eq!(violations.len(), 1);
    assert!(violations[0].fix_hint.is_some());
    assert!(violations[0].fix_hint.as_ref().unwrap().contains("greet"));
}

#[test]
fn test_e001_includes_location() {
    let mut store = in_memory_store();
    let orig = make_node(1, "process", "def process(data: str)", "return data", "utils.py", 5);
    store.update_nodes(vec![NodeChange::Add(orig)]).unwrap();

    let caller = make_node(2, "handler", "def handler()", "process('x')", "handler.py", 20);
    store.update_nodes(vec![NodeChange::Add(caller)]).unwrap();
    store.update_edges(vec![EdgeChange::Add(make_edge(1, 2, 1))]).unwrap();

    let new_def = make_definition("process", "def process(data: list)", "return list(data)", "utils.py", 5);
    let file = make_file_index("utils.py", vec![new_def]);
    let violations = check_broken_callers(&file, &store);

    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].file, "utils.py");
    assert_eq!(violations[0].line, 5);
    assert_eq!(violations[0].affected[0].file, "handler.py");
    assert_eq!(violations[0].affected[0].line, 20);
}

#[test]
fn test_e001_no_callers_no_violation() {
    let mut store = in_memory_store();
    let orig = make_node(1, "lonely", "def lonely(x: int)", "return x", "lib.py", 1);
    store.update_nodes(vec![NodeChange::Add(orig)]).unwrap();

    let new_def = make_definition("lonely", "def lonely(x: int, y: int)", "return x + y", "lib.py", 1);
    let file = make_file_index("lib.py", vec![new_def]);
    let violations = check_broken_callers(&file, &store);

    assert!(violations.is_empty());
}

#[test]
fn test_e001_no_change_no_violation() {
    let mut store = in_memory_store();
    let orig = make_node(1, "stable", "def stable(x: int)", "return x", "lib.py", 1);
    store.update_nodes(vec![NodeChange::Add(orig)]).unwrap();

    let caller = make_node(2, "user", "def user()", "stable(1)", "main.py", 1);
    store.update_nodes(vec![NodeChange::Add(caller)]).unwrap();
    store.update_edges(vec![EdgeChange::Add(make_edge(1, 2, 1))]).unwrap();

    let same_def = make_definition("stable", "def stable(x: int)", "return x", "lib.py", 1);
    let file = make_file_index("lib.py", vec![same_def]);
    let violations = check_broken_callers(&file, &store);

    assert!(violations.is_empty());
}

#[test]
fn test_e001_multiple_changes_multiple_errors() {
    let mut store = in_memory_store();

    let fn_a = make_node(1, "func_a", "def func_a(x: int)", "return x", "lib.py", 1);
    let fn_b = make_node(2, "func_b", "def func_b(y: str)", "return y", "lib.py", 10);
    store.update_nodes(vec![NodeChange::Add(fn_a), NodeChange::Add(fn_b)]).unwrap();

    let caller_a = make_node(3, "call_a", "def call_a()", "func_a(1)", "main.py", 1);
    let caller_b = make_node(4, "call_b", "def call_b()", "func_b('hi')", "main.py", 10);
    store.update_nodes(vec![NodeChange::Add(caller_a), NodeChange::Add(caller_b)]).unwrap();
    store.update_edges(vec![
        EdgeChange::Add(make_edge(1, 3, 1)),
        EdgeChange::Add(make_edge(2, 4, 2)),
    ]).unwrap();

    let new_a = make_definition("func_a", "def func_a(x: int, z: bool)", "return x and z", "lib.py", 1);
    let new_b = make_definition("func_b", "def func_b(y: str, w: str)", "return y + w", "lib.py", 10);
    let file = make_file_index("lib.py", vec![new_a, new_b]);
    let violations = check_broken_callers(&file, &store);

    assert_eq!(violations.len(), 2);
    assert!(violations.iter().all(|v| v.code == "E001"));
}

#[test]
fn test_e001_severity_is_error() {
    let mut store = in_memory_store();
    let orig = make_node(1, "f", "def f(x: int)", "return x", "a.py", 1);
    store.update_nodes(vec![NodeChange::Add(orig)]).unwrap();
    let caller = make_node(2, "g", "def g()", "f(1)", "b.py", 1);
    store.update_nodes(vec![NodeChange::Add(caller)]).unwrap();
    store.update_edges(vec![EdgeChange::Add(make_edge(1, 2, 1))]).unwrap();

    let new_def = make_definition("f", "def f(x: int, y: int)", "return x+y", "a.py", 1);
    let file = make_file_index("a.py", vec![new_def]);
    let violations = check_broken_callers(&file, &store);

    assert!(!violations.is_empty());
    assert_eq!(violations[0].severity, "ERROR");
}

#[test]
fn test_e001_error_code_format() {
    let mut store = in_memory_store();
    let orig = make_node(1, "h", "def h()", "pass", "a.py", 1);
    store.update_nodes(vec![NodeChange::Add(orig)]).unwrap();
    let caller = make_node(2, "k", "def k()", "h()", "b.py", 1);
    store.update_nodes(vec![NodeChange::Add(caller)]).unwrap();
    store.update_edges(vec![EdgeChange::Add(make_edge(1, 2, 1))]).unwrap();

    let new_def = make_definition("h", "def h(x: int)", "return x", "a.py", 1);
    let file = make_file_index("a.py", vec![new_def]);
    let violations = check_broken_callers(&file, &store);

    assert!(!violations.is_empty());
    assert_eq!(violations[0].code, "E001");
}

#[test]
fn test_e001_confidence_from_resolution_tier() {
    let mut store = in_memory_store();
    let orig = make_node(1, "fn1", "def fn1(x: int)", "return x", "a.py", 1);
    store.update_nodes(vec![NodeChange::Add(orig)]).unwrap();
    let caller = make_node(2, "fn2", "def fn2()", "fn1(1)", "b.py", 1);
    store.update_nodes(vec![NodeChange::Add(caller)]).unwrap();
    store.update_edges(vec![EdgeChange::Add(make_edge(1, 2, 1))]).unwrap();

    let new_def = make_definition("fn1", "def fn1(x: str)", "return str(x)", "a.py", 1);
    let file = make_file_index("a.py", vec![new_def]);
    let violations = check_broken_callers(&file, &store);

    assert!(!violations.is_empty());
    assert!(violations[0].confidence > 0.0);
    assert!(violations[0].confidence <= 1.0);
    assert_eq!(violations[0].resolution_tier, "tree-sitter");
}
