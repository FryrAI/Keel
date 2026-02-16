use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};
use keel_parsers::resolver::{Definition, FileIndex};

use crate::engine::EnforcementEngine;

fn make_node(id: u64, hash: &str, name: &str, sig: &str, file: &str) -> GraphNode {
    GraphNode {
        id,
        hash: hash.to_string(),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: sig.to_string(),
        file_path: file.to_string(),
        line_start: 10,
        line_end: 20,
        docstring: Some(format!("Doc for {}", name)),
        is_public: true,
        type_hints_present: true,
        has_docstring: true,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
    }
}

fn make_call_edge(id: u64, src: u64, tgt: u64, file: &str) -> GraphEdge {
    GraphEdge {
        id,
        source_id: src,
        target_id: tgt,
        kind: EdgeKind::Calls,
        file_path: file.to_string(),
        line: 15,
        confidence: 1.0,
    }
}

fn make_definition(name: &str, sig: &str, body: &str, file: &str) -> Definition {
    Definition {
        name: name.to_string(),
        kind: NodeKind::Function,
        signature: sig.to_string(),
        file_path: file.to_string(),
        line_start: 10,
        line_end: 20,
        docstring: Some(format!("Doc for {}", name)),
        is_public: true,
        type_hints_present: true,
        body_text: body.to_string(),
    }
}

#[test]
fn test_where_hash_not_found() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let engine = EnforcementEngine::new(Box::new(store));
    assert!(engine.where_hash("nonexistent").is_none());
}

#[test]
fn test_discover_not_found() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let engine = EnforcementEngine::new(Box::new(store));
    assert!(engine.discover("nonexistent", 1).is_none());
}

#[test]
fn test_where_hash_found() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&make_node(1, "abc12345678", "foo", "fn foo()", "src/lib.rs"))
        .unwrap();
    let engine = EnforcementEngine::new(Box::new(store));

    let result = engine.where_hash("abc12345678");
    assert!(result.is_some());
    let (file, line) = result.unwrap();
    assert_eq!(file, "src/lib.rs");
    assert_eq!(line, 10);
}

#[test]
fn test_discover_with_callers_and_callees() {
    let mut store = SqliteGraphStore::in_memory().unwrap();

    // Create nodes: caller -> target -> callee
    let caller = make_node(1, "cal11111111", "caller_fn", "fn caller_fn()", "src/a.rs");
    let target = make_node(2, "tgt11111111", "target_fn", "fn target_fn(x: i32)", "src/b.rs");
    let callee = make_node(3, "cle11111111", "callee_fn", "fn callee_fn()", "src/c.rs");

    store.insert_node(&caller).unwrap();
    store.insert_node(&target).unwrap();
    store.insert_node(&callee).unwrap();

    // caller calls target, target calls callee
    store
        .update_edges(vec![
            EdgeChange::Add(make_call_edge(1, 1, 2, "src/a.rs")),
            EdgeChange::Add(make_call_edge(2, 2, 3, "src/b.rs")),
        ])
        .unwrap();

    let engine = EnforcementEngine::new(Box::new(store));
    let result = engine.discover("tgt11111111", 1).unwrap();

    assert_eq!(result.target.name, "target_fn");
    assert_eq!(result.target.hash, "tgt11111111");
    assert_eq!(result.upstream.len(), 1);
    assert_eq!(result.upstream[0].name, "caller_fn");
    assert_eq!(result.downstream.len(), 1);
    assert_eq!(result.downstream[0].name, "callee_fn");
}

#[test]
fn test_explain_with_edges() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let node = make_node(1, "abc12345678", "foo", "fn foo()", "src/lib.rs");
    let callee = make_node(2, "def11111111", "bar", "fn bar()", "src/bar.rs");
    store.insert_node(&node).unwrap();
    store.insert_node(&callee).unwrap();

    store
        .update_edges(vec![EdgeChange::Add(make_call_edge(1, 1, 2, "src/lib.rs"))])
        .unwrap();

    let engine = EnforcementEngine::new(Box::new(store));
    let result = engine.explain("E001", "abc12345678").unwrap();

    assert_eq!(result.error_code, "E001");
    assert_eq!(result.hash, "abc12345678");
    assert_eq!(result.confidence, 0.92);
    assert!(!result.resolution_chain.is_empty());
    assert_eq!(result.resolution_chain[0].kind, "call");
}

// --- Integration tests that span compile + discovery ---

#[test]
fn test_e001_and_e002_combined_on_same_file() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let old_hash =
        keel_core::hash::compute_hash("fn foo(x: i32)", "{ x + 1 }", "Doc for foo");
    let mut node = make_node(1, &old_hash, "foo", "fn foo(x: i32)", "src/lib.py");
    node.docstring = Some("Doc for foo".to_string());
    store.insert_node(&node).unwrap();

    let caller = make_node(2, "cal11111111", "bar", "fn bar()", "src/bar.py");
    store.insert_node(&caller).unwrap();

    let mut store_mut = store;
    store_mut
        .update_edges(vec![EdgeChange::Add(make_call_edge(1, 2, 1, "src/bar.py"))])
        .unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store_mut));

    let mut changed_foo =
        make_definition("foo", "fn foo(x: i32, y: i32)", "{ x + y }", "src/lib.py");
    changed_foo.type_hints_present = true;

    let mut no_hints = make_definition("process", "def process(x)", "pass", "src/lib.py");
    no_hints.type_hints_present = false;

    let file = FileIndex {
        file_path: "src/lib.py".to_string(),
        content_hash: 0,
        definitions: vec![changed_foo, no_hints],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let result = engine.compile(&[file]);
    assert_eq!(result.status, "error");

    let e001 = result.errors.iter().filter(|v| v.code == "E001").count();
    let e002 = result.errors.iter().filter(|v| v.code == "E002").count();
    assert!(e001 > 0, "E001 broken_caller should fire");
    assert!(e002 > 0, "E002 missing_type_hints should fire");
}

#[test]
fn test_circuit_breaker_downgrade() {
    // Verifies: (1) first compile fires E001, (2) after persist,
    // recompiling the same file produces no violation (graph is current).
    let store = SqliteGraphStore::in_memory().unwrap();
    let old_hash = keel_core::hash::compute_hash("fn foo()", "{ 1 }", "Doc for foo");
    let mut node = make_node(1, &old_hash, "foo", "fn foo()", "src/lib.rs");
    node.docstring = Some("Doc for foo".to_string());
    store.insert_node(&node).unwrap();

    let caller = make_node(2, "cal11111111", "bar", "fn bar()", "src/bar.rs");
    store.insert_node(&caller).unwrap();

    let mut store_mut = store;
    store_mut
        .update_edges(vec![EdgeChange::Add(make_call_edge(1, 2, 1, "src/bar.rs"))])
        .unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store_mut));

    let file = FileIndex {
        file_path: "src/lib.rs".to_string(),
        content_hash: 0,
        definitions: vec![make_definition(
            "foo",
            "fn foo(x: i32)",
            "{ x }",
            "src/lib.rs",
        )],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let r1 = engine.compile(&[file.clone()]);
    assert!(r1.errors.iter().any(|v| v.code == "E001" && v.severity == "ERROR"));

    let r2 = engine.compile(&[file.clone()]);
    let e001_count = r2.errors.iter().filter(|v| v.code == "E001").count();
    assert_eq!(e001_count, 0, "E001 should not fire after graph is persisted");
}
