// Tests for E005 arity mismatch detection (Spec 006 - Enforcement Engine)
use keel_core::hash::compute_hash;
use keel_core::store::GraphStore;
use keel_core::types::{GraphNode, NodeChange, NodeKind};
use keel_enforce::violations::check_arity_mismatch;
use keel_parsers::resolver::{FileIndex, Reference, ReferenceKind};

use crate::common::in_memory_store;

fn make_target(id: u64, name: &str, sig: &str) -> GraphNode {
    GraphNode {
        id,
        hash: compute_hash(sig, "pass", ""),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: sig.to_string(),
        file_path: "lib.py".to_string(),
        line_start: 1,
        line_end: 5,
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

fn make_call_ref(name_with_args: &str, target_hash: &str, line: u32) -> Reference {
    Reference {
        name: name_with_args.to_string(),
        file_path: "main.py".to_string(),
        line,
        kind: ReferenceKind::Call,
        resolved_to: Some(target_hash.to_string()),
    }
}

#[test]
fn test_e005_added_required_parameter() {
    let mut store = in_memory_store();
    let target = make_target(1, "foo", "def foo(a: int, b: int)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let call_ref = make_call_ref("foo(1)", &target_hash, 5);
    let file = FileIndex {
        file_path: "main.py".to_string(),
        content_hash: 0,
        definitions: vec![],
        references: vec![call_ref],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let violations = check_arity_mismatch(&file, &store);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].code, "E005");
    assert_eq!(violations[0].severity, "ERROR");
    assert_eq!(violations[0].category, "arity_mismatch");
    assert!(violations[0].message.contains("foo"));
}

#[test]
fn test_e005_removed_parameter() {
    let mut store = in_memory_store();
    let target = make_target(1, "bar", "def bar(a: int)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let call_ref = make_call_ref("bar(1, 2)", &target_hash, 10);
    let file = FileIndex {
        file_path: "main.py".to_string(),
        content_hash: 0,
        definitions: vec![],
        references: vec![call_ref],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let violations = check_arity_mismatch(&file, &store);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].code, "E005");
}

#[test]
fn test_e005_matching_arity_no_violation() {
    let mut store = in_memory_store();
    let target = make_target(1, "ok", "def ok(a: int, b: int)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let call_ref = make_call_ref("ok(1, 2)", &target_hash, 5);
    let file = FileIndex {
        file_path: "main.py".to_string(),
        content_hash: 0,
        definitions: vec![],
        references: vec![call_ref],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let violations = check_arity_mismatch(&file, &store);
    assert!(violations.is_empty());
}

#[test]
fn test_e005_includes_count_info() {
    let mut store = in_memory_store();
    let target = make_target(1, "xyz", "def xyz(a: int, b: int, c: int)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let call_ref = make_call_ref("xyz(1, 2)", &target_hash, 5);
    let file = FileIndex {
        file_path: "main.py".to_string(),
        content_hash: 0,
        definitions: vec![],
        references: vec![call_ref],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let violations = check_arity_mismatch(&file, &store);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].message.contains("3") || violations[0].message.contains("2"));
}

#[test]
fn test_e005_includes_fix_hint() {
    let mut store = in_memory_store();
    let target = make_target(1, "convert", "def convert(a: int, b: str)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let call_ref = make_call_ref("convert(1)", &target_hash, 5);
    let file = FileIndex {
        file_path: "main.py".to_string(),
        content_hash: 0,
        definitions: vec![],
        references: vec![call_ref],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let violations = check_arity_mismatch(&file, &store);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].fix_hint.is_some());
    assert!(violations[0].fix_hint.as_ref().unwrap().contains("convert"));
}
