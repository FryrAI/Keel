// Tests for E005 arity mismatch detection (Spec 006 - Enforcement Engine)
//
// Since issue #54, E005 compares the parser-counted call-site arity
// (`Reference::call_arity`) against a precisely-parsed signature, and only
// for references the compile pipeline resolved (`resolved_to`).
use std::collections::HashMap;

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
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

fn make_call_ref(name: &str, target_hash: &str, arity: u32) -> Reference {
    Reference {
        name: name.to_string(),
        file_path: "main.py".to_string(),
        line: 5,
        kind: ReferenceKind::Call,
        resolved_to: Some(target_hash.to_string()),
        call_arity: Some(arity),
    }
}

fn file_with(references: Vec<Reference>) -> FileIndex {
    FileIndex {
        file_path: "main.py".to_string(),
        content_hash: 0,
        definitions: vec![],
        references,
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }
}

fn check(file: &FileIndex, store: &dyn GraphStore) -> Vec<keel_enforce::types::Violation> {
    check_arity_mismatch(file, store, &HashMap::new())
}

#[test]
fn test_e005_added_required_parameter() {
    let mut store = in_memory_store();
    let target = make_target(1, "foo", "def foo(a: int, b: int)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let file = file_with(vec![make_call_ref("foo", &target_hash, 1)]);
    let violations = check(&file, &store);
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

    let file = file_with(vec![make_call_ref("bar", &target_hash, 2)]);
    let violations = check(&file, &store);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].code, "E005");
}

#[test]
fn test_e005_matching_arity_no_violation() {
    let mut store = in_memory_store();
    let target = make_target(1, "ok", "def ok(a: int, b: int)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let file = file_with(vec![make_call_ref("ok", &target_hash, 2)]);
    assert!(check(&file, &store).is_empty());
}

#[test]
fn test_e005_includes_count_info() {
    let mut store = in_memory_store();
    let target = make_target(1, "xyz", "def xyz(a: int, b: int, c: int)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let file = file_with(vec![make_call_ref("xyz", &target_hash, 2)]);
    let violations = check(&file, &store);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].message.contains('3') && violations[0].message.contains('2'));
}

#[test]
fn test_e005_includes_fix_hint() {
    let mut store = in_memory_store();
    let target = make_target(1, "convert", "def convert(a: int, b: str)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let file = file_with(vec![make_call_ref("convert", &target_hash, 1)]);
    let violations = check(&file, &store);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].fix_hint.is_some());
    assert!(violations[0].fix_hint.as_ref().unwrap().contains("convert"));
}

#[test]
fn test_e005_skips_unresolved_and_unknown_arity() {
    let mut store = in_memory_store();
    let target = make_target(1, "foo", "def foo(a: int, b: int)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    // Unresolved reference: no target to compare against.
    let mut unresolved = make_call_ref("foo", &target_hash, 1);
    unresolved.resolved_to = None;
    // Unknown call arity (splat/spread, attribute macro): not comparable.
    let mut no_arity = make_call_ref("foo", &target_hash, 0);
    no_arity.call_arity = None;

    let file = file_with(vec![unresolved, no_arity]);
    assert!(check(&file, &store).is_empty());
}

#[test]
fn test_e005_skips_uncountable_signatures() {
    let mut store = in_memory_store();
    // Variadic and defaulted parameter lists have no single "expected" count.
    let variadic = make_target(1, "log", "def log(*args)");
    let defaulted = make_target(2, "get", "def get(key, default=None)");
    let variadic_hash = variadic.hash.clone();
    let defaulted_hash = defaulted.hash.clone();
    store
        .update_nodes(vec![NodeChange::Add(variadic), NodeChange::Add(defaulted)])
        .unwrap();

    let file = file_with(vec![
        make_call_ref("log", &variadic_hash, 5),
        make_call_ref("get", &defaulted_hash, 1),
    ]);
    assert!(check(&file, &store).is_empty());
}

#[test]
fn test_e005_tolerates_explicit_receiver_through_type() {
    let mut store = in_memory_store();
    let target = make_target(1, "__init__", "def __init__(self, x)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    // `Base.__init__(self, x)` writes the receiver explicitly: 2 written args
    // against a stripped-receiver arity of 1 — legal through the type.
    let file = file_with(vec![make_call_ref("Base.__init__", &target_hash, 2)]);
    assert!(check(&file, &store).is_empty());

    // Two EXTRA args through the type is still a real mismatch.
    let file = file_with(vec![make_call_ref("Base.__init__", &target_hash, 3)]);
    assert_eq!(check(&file, &store).len(), 1);
}

#[test]
fn test_e005_value_receiver_gets_no_tolerance() {
    let mut store = in_memory_store();
    // `registry.register(self)` passes `self` as a genuine argument: the
    // value receiver `registry` earns no off-by-one tolerance.
    let target = make_target(1, "register", "def register(self, widget)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let file = file_with(vec![make_call_ref("registry.register", &target_hash, 1)]);
    assert!(check(&file, &store).is_empty());

    let file = file_with(vec![make_call_ref("registry.register", &target_hash, 2)]);
    assert_eq!(check(&file, &store).len(), 1);
}

#[test]
fn test_e005_associated_target_tolerates_type_receiver_in_go_only() {
    let mut store = in_memory_store();
    // A Go method expression `T.Method(t, x)`: the stored signature carries no
    // receiver token, but the node is marked associated, so the type-qualified
    // call may still write the receiver as its first argument.
    let mut target = make_target(1, "Scan", "Scan(x int)");
    target.is_associated = true;
    target.file_path = "lib.go".to_string();
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let file = file_with(vec![make_call_ref("Table.Scan", &target_hash, 2)]);
    assert!(check(&file, &store).is_empty());

    // Outside Go the same shape is a REAL extra argument: a receiver-less
    // associated function (`Foo::new`, a Python @staticmethod) takes exactly
    // what its signature says, and `Foo.compute(bogus, x)` must fire.
    let mut py_static = make_target(2, "compute", "def compute(x)");
    py_static.is_associated = true;
    let py_hash = py_static.hash.clone();
    store
        .update_nodes(vec![NodeChange::Add(py_static)])
        .unwrap();

    let file = file_with(vec![make_call_ref("Foo.compute", &py_hash, 2)]);
    assert_eq!(
        check(&file, &store).len(),
        1,
        "a receiver-less associated fn outside Go gets no tolerance"
    );
}

#[test]
fn test_e005_batch_signature_wins_over_stored() {
    let mut store = in_memory_store();
    // Stored contract: 2 params. The same compile batch re-parsed lib.py with
    // a 3-param signature — the fresh contract is what callers are judged by.
    let target = make_target(1, "foo", "def foo(a: int, b: int)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let fresh_def = keel_parsers::resolver::Definition {
        name: "foo".to_string(),
        kind: NodeKind::Function,
        signature: "def foo(a: int, b: int, c: int)".to_string(),
        file_path: "lib.py".to_string(),
        line_start: 1,
        line_end: 5,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        body_text: "pass".to_string(),
        in_test_context: false,
        in_trait_context: false,
        is_associated: false,
        is_auto_invoked: false,
        is_decorated: false,
        has_keep_marker: false,
        is_macro: false,
    };
    let lib_file = FileIndex {
        file_path: "lib.py".to_string(),
        content_hash: 0,
        definitions: vec![fresh_def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };
    let batch: HashMap<&str, &FileIndex> = [("lib.py", &lib_file)].into();

    // 3 args matches the fresh signature (would mismatch the stored one).
    let file = file_with(vec![make_call_ref("foo", &target_hash, 3)]);
    assert!(check_arity_mismatch(&file, &store, &batch).is_empty());

    // 2 args matches the stale stored signature — now a real mismatch.
    let file = file_with(vec![make_call_ref("foo", &target_hash, 2)]);
    assert_eq!(check_arity_mismatch(&file, &store, &batch).len(), 1);
}

#[test]
fn test_e005_ambiguous_batch_fallback_skips() {
    let mut store = in_memory_store();
    // Stored target at line 1; the batch file has moved things around AND
    // legally holds TWO same-named defs. The exact (name, line) match fails
    // and the name-only fallback is a coin flip — E005 must skip, not guess.
    let target = make_target(1, "foo", "def foo(a: int, b: int)");
    let target_hash = target.hash.clone();
    store.update_nodes(vec![NodeChange::Add(target)]).unwrap();

    let def_at = |line: u32, sig: &str| keel_parsers::resolver::Definition {
        name: "foo".to_string(),
        kind: NodeKind::Function,
        signature: sig.to_string(),
        file_path: "lib.py".to_string(),
        line_start: line,
        line_end: line + 3,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        body_text: "pass".to_string(),
        in_test_context: false,
        in_trait_context: false,
        is_associated: false,
        is_auto_invoked: false,
        is_decorated: false,
        has_keep_marker: false,
        is_macro: false,
    };
    let lib_file = FileIndex {
        file_path: "lib.py".to_string(),
        content_hash: 0,
        definitions: vec![
            def_at(10, "def foo(a: int)"),
            def_at(30, "def foo(a: int, b: int, c: int)"),
        ],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };
    let batch: HashMap<&str, &FileIndex> = [("lib.py", &lib_file)].into();

    // Any arity: with two candidate contracts the comparison is unfoundable.
    for arity in [1u32, 2, 3] {
        let file = file_with(vec![make_call_ref("foo", &target_hash, arity)]);
        assert!(
            check_arity_mismatch(&file, &store, &batch).is_empty(),
            "ambiguous batch fallback must skip (arity {arity})"
        );
    }
}
