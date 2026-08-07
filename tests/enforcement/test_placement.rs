// Tests for W001 placement scoring (Spec 006 - Enforcement Engine)
//
// W001 checks function name prefixes against module responsibility profiles.
// Since module profiles require direct DB insertion (no public API), these tests
// focus on verifiable behaviors through the public API.
use keel_core::hash::compute_hash;
use keel_core::store::GraphStore;
use keel_core::types::{GraphNode, ModuleProfile, NodeChange, NodeKind};
use keel_enforce::violations::check_placement;
use keel_parsers::resolver::{Definition, FileIndex};

use crate::common::in_memory_store;

/// A module profile whose only prefix is `prefix`, at the given module_id/path.
fn make_profile(module_id: u64, path: &str, prefix: &str) -> ModuleProfile {
    ModuleProfile {
        module_id,
        path: path.to_string(),
        function_count: 1,
        class_count: 0,
        line_count: 10,
        function_name_prefixes: vec![prefix.to_string()],
        primary_types: vec![],
        import_sources: vec![],
        export_targets: vec![],
        external_endpoint_count: 0,
        responsibility_keywords: vec![],
    }
}

fn make_module_node(id: u64, file: &str) -> GraphNode {
    GraphNode {
        complexity: 0,
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

fn make_func_def(name: &str, file: &str) -> Definition {
    Definition {
        complexity: 1,
        name: name.to_string(),
        kind: NodeKind::Function,
        signature: format!("def {name}()"),
        file_path: file.to_string(),
        line_start: 1,
        line_end: 5,
        docstring: Some("doc".to_string()),
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
fn test_w001_no_modules_no_warning() {
    let store = in_memory_store();
    // No modules in the store at all
    let def = make_func_def("validate_email", "utils.py");
    let file = make_file("utils.py", vec![def]);

    let violations = check_placement(&file, &store);
    assert!(violations.is_empty());
}

#[test]
fn test_w001_no_profile_no_warning() {
    let mut store = in_memory_store();
    // Module exists but has no profile
    let mod_node = make_module_node(1, "validators.py");
    store.update_nodes(vec![NodeChange::Add(mod_node)]).unwrap();

    let def = make_func_def("validate_email", "utils.py");
    let file = make_file("utils.py", vec![def]);

    let violations = check_placement(&file, &store);
    // No module profiles → no W001 (check_placement queries get_module_profile which returns None)
    assert!(violations.is_empty());
}

#[test]
fn test_w001_same_file_module_skipped() {
    let mut store = in_memory_store();
    // Even if module exists in same file, it's skipped
    let mod_node = make_module_node(1, "parsers.py");
    store.update_nodes(vec![NodeChange::Add(mod_node)]).unwrap();

    let def = make_func_def("parse_json", "parsers.py");
    let file = make_file("parsers.py", vec![def]);

    let violations = check_placement(&file, &store);
    assert!(violations.is_empty());
}

#[test]
fn test_w001_empty_prefix_no_warning() {
    let mut store = in_memory_store();
    let mod_node = make_module_node(1, "handlers.py");
    store.update_nodes(vec![NodeChange::Add(mod_node)]).unwrap();

    // Single lowercase word → extract_prefix returns empty string → skip
    let def = make_func_def("x", "other.py");
    let file = make_file("other.py", vec![def]);

    let violations = check_placement(&file, &store);
    assert!(violations.is_empty());
}

#[test]
fn test_w001_class_not_checked() {
    let mut store = in_memory_store();
    let mod_node = make_module_node(1, "validators.py");
    store.update_nodes(vec![NodeChange::Add(mod_node)]).unwrap();

    // Classes are skipped by check_placement (only functions)
    let class_def = Definition {
        complexity: 1,
        name: "ValidateEmail".to_string(),
        kind: NodeKind::Class,
        signature: "class ValidateEmail".to_string(),
        file_path: "utils.py".to_string(),
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
    let file = make_file("utils.py", vec![class_def]);

    let violations = check_placement(&file, &store);
    assert!(violations.is_empty());
}

#[test]
fn test_w001_violation_structure() {
    // Verify W001 violation has correct structure if it fires
    // This test documents the expected format even though we can't
    // trigger it without module profiles in the store.
    // W001 violations should always be WARNING severity.
    use keel_enforce::types::Violation;

    let v = Violation {
        code: "W001".to_string(),
        severity: "WARNING".to_string(),
        category: "placement".to_string(),
        message: "Function `validate_email` may belong in module `validators.py`".to_string(),
        file: "utils.py".to_string(),
        line: 1,
        hash: String::new(),
        confidence: 0.6,
        resolution_tier: "heuristic".to_string(),
        fix_hint: Some("Consider moving `validate_email` to `validators.py`".to_string()),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: Some("validators.py".to_string()),
        existing: None,
    };

    assert_eq!(v.code, "W001");
    assert_eq!(v.severity, "WARNING");
    assert!(v.suggested_module.is_some());
    assert!(v.confidence > 0.0 && v.confidence <= 1.0);
}

#[test]
fn test_w001_multi_segment_prefix_fires() {
    // "get_user_name" -> prefix "get_user" (2 segments) matches a module
    // profile keyed on "get_user" -> should fire.
    let mut store = in_memory_store();
    let mod_node = make_module_node(1, "user_service.py");
    store.update_nodes(vec![NodeChange::Add(mod_node)]).unwrap();
    store
        .upsert_module_profiles(vec![make_profile(1, "user_service.py", "get_user")])
        .unwrap();

    let def = make_func_def("get_user_name", "utils.py");
    let file = make_file("utils.py", vec![def]);

    let violations = check_placement(&file, &store);
    assert_eq!(violations.len(), 1);
    assert_eq!(
        violations[0].suggested_module,
        Some("user_service.py".to_string())
    );
}

#[test]
fn test_w001_single_segment_names_never_fire() {
    // "run"/"main"/"default" are single-word names — extract_prefix always
    // returns empty for them, so W001 must never fire regardless of what
    // module profiles exist.
    let mut store = in_memory_store();
    let mod_node = make_module_node(1, "main_service.py");
    store.update_nodes(vec![NodeChange::Add(mod_node)]).unwrap();
    store
        .upsert_module_profiles(vec![make_profile(1, "main_service.py", "run")])
        .unwrap();

    for name in ["run", "main", "default"] {
        let def = make_func_def(name, "utils.py");
        let file = make_file("utils.py", vec![def]);
        let violations = check_placement(&file, &store);
        assert!(
            violations.is_empty(),
            "`{name}` should never fire W001 (single-segment name)"
        );
    }
}

#[test]
fn test_w001_two_segment_name_no_longer_over_matches() {
    // Regression guard for the de-noise fix: "make_relative" used to extract
    // just "make" and over-match against any module whose profile happened
    // to list "make" as a prefix. A two-segment name has no room to produce
    // a genuine 2-segment prefix (it would just be the whole name), so this
    // must no longer fire even though a matching "make" profile exists.
    let mut store = in_memory_store();
    let mod_node = make_module_node(1, "path_utils.py");
    store.update_nodes(vec![NodeChange::Add(mod_node)]).unwrap();
    store
        .upsert_module_profiles(vec![make_profile(1, "path_utils.py", "make")])
        .unwrap();

    let def = make_func_def("make_relative", "utils.py");
    let file = make_file("utils.py", vec![def]);
    let violations = check_placement(&file, &store);
    assert!(violations.is_empty());
}
