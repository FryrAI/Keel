// Tests for W002 duplicate function name detection (Spec 006 - Enforcement Engine)
use keel_core::hash::compute_hash;
use keel_core::store::GraphStore;
use keel_core::types::{GraphNode, NodeChange, NodeKind};
use keel_enforce::violations::check_duplicate_names;
use keel_parsers::resolver::{Definition, FileIndex};

use crate::common::in_memory_store;

fn make_module_node(id: u64, file: &str) -> GraphNode {
    GraphNode {
        id,
        hash: compute_hash(&format!("module:{file}"), "", ""),
        kind: NodeKind::Module,
        name: file.to_string(),
        signature: String::new(),
        file_path: file.to_string(),
        line_start: 1,
        line_end: 100,
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

fn make_func_node(id: u64, name: &str, file: &str, line: u32) -> GraphNode {
    GraphNode {
        id,
        hash: compute_hash(&format!("def {name}()"), "pass", ""),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: format!("def {name}()"),
        file_path: file.to_string(),
        line_start: line,
        line_end: line + 5,
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

fn make_func_def(name: &str, file: &str, line: u32) -> Definition {
    Definition {
        name: name.to_string(),
        kind: NodeKind::Function,
        signature: format!("def {name}()"),
        file_path: file.to_string(),
        line_start: line,
        line_end: line + 5,
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
    }
}

fn make_file(file: &str, defs: Vec<Definition>) -> FileIndex {
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

#[test]
fn test_w002_duplicate_name_across_modules() {
    let mut store = in_memory_store();

    // Module A with process()
    let mod_a = make_module_node(1, "module_a.py");
    let fn_a = make_func_node(2, "process", "module_a.py", 1);

    // Module B also with process()
    let mod_b = make_module_node(3, "module_b.py");
    let fn_b = make_func_node(4, "process", "module_b.py", 1);

    store
        .update_nodes(vec![
            NodeChange::Add(mod_a),
            NodeChange::Add(fn_a),
            NodeChange::Add(mod_b),
            NodeChange::Add(fn_b),
        ])
        .unwrap();

    // Check from module_a's perspective
    let def = make_func_def("process", "module_a.py", 1);
    let file = make_file("module_a.py", vec![def]);

    let violations = check_duplicate_names(&file, &store);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].code, "W002");
    assert_eq!(violations[0].severity, "WARNING");
    assert_eq!(violations[0].category, "duplicate_name");
    assert!(violations[0].existing.is_some());
    assert_eq!(violations[0].existing.as_ref().unwrap().file, "module_b.py");
}

#[test]
fn test_w002_no_duplicate_no_warning() {
    let mut store = in_memory_store();

    let mod_a = make_module_node(1, "module_a.py");
    let fn_a = make_func_node(2, "unique_name", "module_a.py", 1);
    store
        .update_nodes(vec![NodeChange::Add(mod_a), NodeChange::Add(fn_a)])
        .unwrap();

    let def = make_func_def("different_name", "module_b.py", 1);
    let file = make_file("module_b.py", vec![def]);

    let violations = check_duplicate_names(&file, &store);
    assert!(violations.is_empty());
}

#[test]
fn test_w002_severity_is_warning() {
    let mut store = in_memory_store();

    let mod_a = make_module_node(1, "a.py");
    let fn_a = make_func_node(2, "dupe", "a.py", 1);
    let mod_b = make_module_node(3, "b.py");
    let fn_b = make_func_node(4, "dupe", "b.py", 1);
    store
        .update_nodes(vec![
            NodeChange::Add(mod_a),
            NodeChange::Add(fn_a),
            NodeChange::Add(mod_b),
            NodeChange::Add(fn_b),
        ])
        .unwrap();

    let def = make_func_def("dupe", "a.py", 1);
    let file = make_file("a.py", vec![def]);

    let violations = check_duplicate_names(&file, &store);
    assert!(!violations.is_empty());
    assert_eq!(violations[0].severity, "WARNING");
}

#[test]
fn test_w002_includes_all_locations() {
    let mut store = in_memory_store();

    let mod_a = make_module_node(1, "a.py");
    let fn_a = make_func_node(2, "process", "a.py", 5);
    let mod_b = make_module_node(3, "b.py");
    let fn_b = make_func_node(4, "process", "b.py", 10);
    store
        .update_nodes(vec![
            NodeChange::Add(mod_a),
            NodeChange::Add(fn_a),
            NodeChange::Add(mod_b),
            NodeChange::Add(fn_b),
        ])
        .unwrap();

    let def = make_func_def("process", "a.py", 5);
    let file = make_file("a.py", vec![def]);

    let violations = check_duplicate_names(&file, &store);
    assert_eq!(violations.len(), 1);
    let existing = violations[0].existing.as_ref().unwrap();
    assert_eq!(existing.file, "b.py");
    assert_eq!(existing.line, 10);
}

#[test]
fn test_w002_test_files_excluded() {
    let mut store = in_memory_store();

    // Normal file has process()
    let mod_a = make_module_node(1, "main.py");
    let fn_a = make_func_node(2, "process", "main.py", 1);
    // Test file also has process()
    let mod_b = make_module_node(3, "test_main.py");
    let fn_b = make_func_node(4, "process", "test_main.py", 1);
    store
        .update_nodes(vec![
            NodeChange::Add(mod_a),
            NodeChange::Add(fn_a),
            NodeChange::Add(mod_b),
            NodeChange::Add(fn_b),
        ])
        .unwrap();

    // Check from test file — test files are skipped entirely
    let def = make_func_def("process", "test_main.py", 1);
    let file = make_file("test_main.py", vec![def]);

    let violations = check_duplicate_names(&file, &store);
    assert!(violations.is_empty());
}

#[test]
fn test_w002_fix_hint_present() {
    let mut store = in_memory_store();

    let mod_a = make_module_node(1, "a.py");
    let fn_a = make_func_node(2, "handler", "a.py", 1);
    let mod_b = make_module_node(3, "b.py");
    let fn_b = make_func_node(4, "handler", "b.py", 1);
    store
        .update_nodes(vec![
            NodeChange::Add(mod_a),
            NodeChange::Add(fn_a),
            NodeChange::Add(mod_b),
            NodeChange::Add(fn_b),
        ])
        .unwrap();

    let def = make_func_def("handler", "a.py", 1);
    let file = make_file("a.py", vec![def]);

    let violations = check_duplicate_names(&file, &store);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].fix_hint.is_some());
    assert!(violations[0].fix_hint.as_ref().unwrap().contains("handler"));
}

#[test]
fn test_w002_same_name_different_signature_no_warning() {
    // Idiomatic namespacing: every module defines its own `run()` with a
    // different shape (e.g. one CLI subcommand's `run(args)` vs another's
    // `run(args, ctx)`). Same name, different signature -> not a duplicate,
    // must stay silent.
    let mut store = in_memory_store();

    let mod_a = make_module_node(1, "cmd_build.py");
    let fn_a = make_func_node(2, "run", "cmd_build.py", 1); // "def run()"
    let mod_b = make_module_node(3, "cmd_deploy.py");
    let mut fn_b = make_func_node(4, "run", "cmd_deploy.py", 1);
    fn_b.signature = "def run(env)".to_string(); // different shape
    store
        .update_nodes(vec![
            NodeChange::Add(mod_a),
            NodeChange::Add(fn_a),
            NodeChange::Add(mod_b),
            NodeChange::Add(fn_b),
        ])
        .unwrap();

    let def = make_func_def("run", "cmd_build.py", 1); // "def run()", matches fn_a's shape
    let file = make_file("cmd_build.py", vec![def]);

    let violations = check_duplicate_names(&file, &store);
    assert!(
        violations.is_empty(),
        "same name with a different signature elsewhere should not fire W002"
    );
}

#[test]
fn test_w002_whitespace_normalized_signature_still_fires() {
    // The signature comparison should ignore pure reformatting (rustfmt/
    // prettier-style wrapping), same as E001's normalize_signature reuse.
    let mut store = in_memory_store();

    let mod_a = make_module_node(1, "a.py");
    let mut fn_a = make_func_node(2, "process", "a.py", 1);
    fn_a.signature = "fn process(x: i32, y: i32) -> bool".to_string();
    let mod_b = make_module_node(3, "b.py");
    let mut fn_b = make_func_node(4, "process", "b.py", 1);
    fn_b.signature = "fn process(\n    x: i32,\n    y: i32\n) -> bool".to_string();
    store
        .update_nodes(vec![
            NodeChange::Add(mod_a),
            NodeChange::Add(fn_a),
            NodeChange::Add(mod_b),
            NodeChange::Add(fn_b),
        ])
        .unwrap();

    let mut def = make_func_def("process", "a.py", 1);
    def.signature = "fn process(x: i32, y: i32) -> bool".to_string();
    let file = make_file("a.py", vec![def]);

    let violations = check_duplicate_names(&file, &store);
    assert_eq!(
        violations.len(),
        1,
        "whitespace-only signature differences should still count as duplicates"
    );
}

#[test]
fn test_w002_trait_style_trio_silent() {
    // Three identically-named, identically-shaped sites (e.g. every
    // LanguageResolver impl defining its own `parse_file`, or every type's
    // `Default`/`fmt` impl) is conformance to a shared contract, not an
    // accidental duplicate -- the pair-only rule must stay silent here even
    // though name+signature both match everywhere.
    let mut store = in_memory_store();

    // Each impl has a distinct body in practice (different language, same
    // shared signature), so give each node a distinct hash -- otherwise all
    // three would collide on the same `hash` (its UNIQUE key in the store)
    // and the later inserts would silently overwrite the earlier ones.
    let mod_a = make_module_node(1, "ts_resolver.py");
    let mut fn_a = make_func_node(2, "parse_file", "ts_resolver.py", 1);
    fn_a.hash = "hashts0000a".to_string();
    let mod_b = make_module_node(3, "py_resolver.py");
    let mut fn_b = make_func_node(4, "parse_file", "py_resolver.py", 1);
    fn_b.hash = "hashpy0000b".to_string();
    let mod_c = make_module_node(5, "go_resolver.py");
    let mut fn_c = make_func_node(6, "parse_file", "go_resolver.py", 1);
    fn_c.hash = "hashgo0000c".to_string();
    store
        .update_nodes(vec![
            NodeChange::Add(mod_a),
            NodeChange::Add(fn_a),
            NodeChange::Add(mod_b),
            NodeChange::Add(fn_b),
            NodeChange::Add(mod_c),
            NodeChange::Add(fn_c),
        ])
        .unwrap();

    let def = make_func_def("parse_file", "ts_resolver.py", 1);
    let file = make_file("ts_resolver.py", vec![def]);

    let violations = check_duplicate_names(&file, &store);
    assert!(
        violations.is_empty(),
        "3+ identically-shaped sites is trait/interface conformance, not a duplicate"
    );
}

#[test]
fn test_w002_class_not_reported() {
    let mut store = in_memory_store();

    // Only function duplicates trigger W002, not classes
    let mod_a = make_module_node(1, "a.py");
    let class_a = GraphNode {
        id: 2,
        hash: compute_hash("class Model", "pass", ""),
        kind: NodeKind::Class,
        name: "Model".to_string(),
        signature: "class Model".to_string(),
        file_path: "a.py".to_string(),
        line_start: 1,
        line_end: 10,
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
    store
        .update_nodes(vec![NodeChange::Add(mod_a), NodeChange::Add(class_a)])
        .unwrap();

    // File with class definition (not function)
    let class_def = Definition {
        name: "Model".to_string(),
        kind: NodeKind::Class,
        signature: "class Model".to_string(),
        file_path: "b.py".to_string(),
        line_start: 1,
        line_end: 10,
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
    let file = make_file("b.py", vec![class_def]);

    let violations = check_duplicate_names(&file, &store);
    assert!(violations.is_empty());
}

#[test]
fn test_w002_stored_associated_node_no_warning() {
    // The cross-compile regression (issue #46): a genuine free function in the
    // file being compiled must not collide with a stored associated method of
    // the same name+signature that was persisted by an earlier `keel map`. The
    // persisted `is_associated` flag is the only thing that can tell them apart
    // — the current file's def has no visibility into the other type's `impl`.
    let mut store = in_memory_store();

    let mod_a = make_module_node(1, "expiry.py");
    // Stored node is an associated method: `impl Session { fn is_expired() }`.
    let mut fn_a = make_func_node(2, "is_expired", "session.py", 1);
    fn_a.is_associated = true;
    store
        .update_nodes(vec![NodeChange::Add(mod_a), NodeChange::Add(fn_a)])
        .unwrap();

    // Current file has a genuinely free `is_expired()` with the same shape.
    let def = make_func_def("is_expired", "expiry.py", 1);
    let file = make_file("expiry.py", vec![def]);

    let violations = check_duplicate_names(&file, &store);
    assert!(
        violations.is_empty(),
        "a free function must not collide with a stored associated method of the same name"
    );
}

#[test]
fn test_w002_stored_free_node_still_fires() {
    // Control for the fix above: when the stored node is itself a free
    // function (is_associated: false), the genuine duplicate must STILL fire.
    // Guards against the new filter over-suppressing every stored node.
    let mut store = in_memory_store();

    let mod_a = make_module_node(1, "module_a.py");
    let fn_a = make_func_node(2, "process", "module_a.py", 1); // free, default false
    store
        .update_nodes(vec![NodeChange::Add(mod_a), NodeChange::Add(fn_a)])
        .unwrap();

    let def = make_func_def("process", "module_b.py", 1);
    let file = make_file("module_b.py", vec![def]);

    let violations = check_duplicate_names(&file, &store);
    assert_eq!(
        violations.len(),
        1,
        "a free-vs-free duplicate must still fire after the associated-node filter"
    );
    assert_eq!(violations[0].code, "W002");
}
