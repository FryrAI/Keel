// Tests for progressive adoption (new vs existing code) (Spec 006 - Enforcement Engine)
use keel_core::types::NodeKind;
use keel_enforce::engine::EnforcementEngine;
use keel_enforce::types::Violation;
use keel_parsers::resolver::{Definition, FileIndex};

use crate::common::in_memory_store;

fn make_file_no_hints(name: &str) -> FileIndex {
    FileIndex {
        file_path: format!("{name}.py"),
        content_hash: 0,
        definitions: vec![Definition {
            name: name.to_string(),
            kind: NodeKind::Function,
            signature: format!("def {name}(data)"),
            file_path: format!("{name}.py"),
            line_start: 1,
            line_end: 3,
            docstring: None,
            is_public: true,
            type_hints_present: false,
            body_text: "return data".to_string(),
        }],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }
}

fn make_clean_file(name: &str) -> FileIndex {
    FileIndex {
        file_path: format!("{name}.py"),
        content_hash: 0,
        definitions: vec![Definition {
            name: name.to_string(),
            kind: NodeKind::Function,
            signature: format!("def {name}(data: str) -> str"),
            file_path: format!("{name}.py"),
            line_start: 1,
            line_end: 3,
            docstring: Some("Documented.".to_string()),
            is_public: true,
            type_hints_present: true,
            body_text: "return data".to_string(),
        }],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }
}

#[test]
fn test_new_code_produces_error() {
    let store = in_memory_store();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[make_file_no_hints("new_func")]);
    assert!(!result.errors.is_empty());
    let has_e002 = result.errors.iter().any(|v| v.code == "E002");
    let has_e003 = result.errors.iter().any(|v| v.code == "E003");
    assert!(has_e002 || has_e003);
}

#[test]
fn test_clean_code_no_violations() {
    let store = in_memory_store();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[make_clean_file("good_func")]);
    assert!(result.errors.is_empty());
}

#[test]
fn test_multiple_files_independent_violations() {
    let store = in_memory_store();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let dirty = make_file_no_hints("dirty_func");
    let clean = make_clean_file("clean_func");
    let result = engine.compile(&[dirty, clean]);

    let dirty_violations: Vec<&Violation> = result
        .errors
        .iter()
        .filter(|v| v.file.contains("dirty"))
        .collect();
    let clean_violations: Vec<&Violation> = result
        .errors
        .iter()
        .filter(|v| v.file.contains("clean"))
        .collect();

    assert!(!dirty_violations.is_empty());
    assert!(clean_violations.is_empty());
}

#[test]
fn test_compile_twice_same_file() {
    let store = in_memory_store();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let file1 = make_file_no_hints("func_a");
    let result1 = engine.compile(&[file1]);
    assert!(!result1.errors.is_empty());

    let file2 = make_file_no_hints("func_a");
    let result2 = engine.compile(&[file2]);
    assert!(!result2.errors.is_empty());
}

#[test]
fn test_error_severity_is_error_for_new_code() {
    let store = in_memory_store();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[make_file_no_hints("new_code")]);
    for v in &result.errors {
        assert_eq!(v.severity, "ERROR");
    }
}
