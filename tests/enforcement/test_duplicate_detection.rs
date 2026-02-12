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
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
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
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
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

    store.update_nodes(vec![
        NodeChange::Add(mod_a),
        NodeChange::Add(fn_a),
        NodeChange::Add(mod_b),
        NodeChange::Add(fn_b),
    ]).unwrap();

    // Check from module_a's perspective
    let def = make_func_def("process", "module_a.py", 1);
    let file = FileIndex {
        file_path: "module_a.py".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

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
    store.update_nodes(vec![NodeChange::Add(mod_a), NodeChange::Add(fn_a)]).unwrap();

    let def = make_func_def("different_name", "module_b.py", 1);
    let file = FileIndex {
        file_path: "module_b.py".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

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
    store.update_nodes(vec![
        NodeChange::Add(mod_a), NodeChange::Add(fn_a),
        NodeChange::Add(mod_b), NodeChange::Add(fn_b),
    ]).unwrap();

    let def = make_func_def("dupe", "a.py", 1);
    let file = FileIndex {
        file_path: "a.py".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

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
    store.update_nodes(vec![
        NodeChange::Add(mod_a), NodeChange::Add(fn_a),
        NodeChange::Add(mod_b), NodeChange::Add(fn_b),
    ]).unwrap();

    let def = make_func_def("process", "a.py", 5);
    let file = FileIndex {
        file_path: "a.py".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

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
    store.update_nodes(vec![
        NodeChange::Add(mod_a), NodeChange::Add(fn_a),
        NodeChange::Add(mod_b), NodeChange::Add(fn_b),
    ]).unwrap();

    // Check from test file — test files are skipped entirely
    let def = make_func_def("process", "test_main.py", 1);
    let file = FileIndex {
        file_path: "test_main.py".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

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
    store.update_nodes(vec![
        NodeChange::Add(mod_a), NodeChange::Add(fn_a),
        NodeChange::Add(mod_b), NodeChange::Add(fn_b),
    ]).unwrap();

    let def = make_func_def("handler", "a.py", 1);
    let file = FileIndex {
        file_path: "a.py".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let violations = check_duplicate_names(&file, &store);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].fix_hint.is_some());
    assert!(violations[0].fix_hint.as_ref().unwrap().contains("handler"));
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
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
    };
    store.update_nodes(vec![NodeChange::Add(mod_a), NodeChange::Add(class_a)]).unwrap();

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
    };
    let file = FileIndex {
        file_path: "b.py".to_string(),
        content_hash: 0,
        definitions: vec![class_def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let violations = check_duplicate_names(&file, &store);
    assert!(violations.is_empty());
}
