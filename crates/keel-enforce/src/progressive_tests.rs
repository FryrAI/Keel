use super::*;
use keel_core::types::NodeKind;
use keel_parsers::resolver::Definition;

fn undocumented_def(name: &str, file: &str) -> Definition {
    Definition {
        complexity: 1,
        name: name.to_string(),
        kind: NodeKind::Function,
        signature: format!("fn {name}()"),
        file_path: file.to_string(),
        line_start: 10,
        line_end: 20,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        body_text: "do_work()".to_string(),
        in_test_context: false,
        in_trait_context: false,
        is_associated: false,
        is_auto_invoked: false,
        is_decorated: false,
        has_keep_marker: false,
        is_macro: false,
    }
}

fn file_with(def: Definition) -> FileIndex {
    FileIndex {
        file_path: def.file_path.clone(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }
}

/// A stored node whose hash matches the definition exactly (untouched).
fn matching_node(def: &Definition) -> GraphNode {
    GraphNode {
        complexity: 0,
        id: 1,
        hash: keel_core::hash::compute_hash(
            &def.signature,
            &def.body_for_hash(),
            def.docstring.as_deref().unwrap_or(""),
        ),
        kind: NodeKind::Function,
        name: def.name.clone(),
        signature: def.signature.clone(),
        file_path: def.file_path.clone(),
        line_start: def.line_start,
        line_end: def.line_end,
        docstring: def.docstring.clone(),
        is_public: def.is_public,
        type_hints_present: def.type_hints_present,
        has_docstring: def.docstring.is_some(),
        is_associated: def.is_associated,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

fn e003_violation(def: &Definition) -> Violation {
    Violation {
        code: "E003".to_string(),
        severity: "ERROR".to_string(),
        category: "missing_docstring".to_string(),
        message: format!("Public function `{}` has no docstring", def.name),
        file: def.file_path.clone(),
        line: def.line_start,
        hash: String::new(),
        confidence: 1.0,
        resolution_tier: "tree-sitter".to_string(),
        fix_hint: Some("Add a documentation comment".to_string()),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }
}

#[test]
fn untouched_function_downgrades_to_warning() {
    let def = undocumented_def("legacy", "src/old.rs");
    let node = matching_node(&def);
    let file = file_with(def.clone());

    let out = apply_progressive_adoption(vec![e003_violation(&def)], &file, &[node]);
    assert_eq!(out[0].severity, "WARNING");
    assert!(out[0].fix_hint.as_deref().unwrap().contains("pre-existing"));
}

#[test]
fn modified_function_stays_error() {
    let def = undocumented_def("edited", "src/old.rs");
    let mut node = matching_node(&def);
    node.hash = "differentHash".to_string(); // stored state differs → touched
    let file = file_with(def.clone());

    let out = apply_progressive_adoption(vec![e003_violation(&def)], &file, &[node]);
    assert_eq!(out[0].severity, "ERROR");
}

#[test]
fn new_function_stays_error() {
    let def = undocumented_def("brand_new", "src/new.rs");
    let file = file_with(def.clone());

    let out = apply_progressive_adoption(vec![e003_violation(&def)], &file, &[]);
    assert_eq!(out[0].severity, "ERROR");
}

#[test]
fn structural_codes_never_downgrade() {
    let def = undocumented_def("legacy", "src/old.rs");
    let node = matching_node(&def);
    let file = file_with(def.clone());

    let mut v = e003_violation(&def);
    v.code = "E001".to_string();
    v.category = "broken_caller".to_string();
    let out = apply_progressive_adoption(vec![v], &file, &[node]);
    assert_eq!(out[0].severity, "ERROR", "E001 must never be grandfathered");
}
