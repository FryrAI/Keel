// Tests for clean compile behavior (Spec 006 - Enforcement Engine)
use keel_core::types::NodeKind;
use keel_enforce::engine::EnforcementEngine;
use keel_parsers::resolver::{Definition, FileIndex};

use crate::common::in_memory_store;

fn make_clean_file() -> FileIndex {
    FileIndex {
        file_path: "clean.py".to_string(),
        content_hash: 0,
        definitions: vec![Definition {
            name: "good_func".to_string(),
            kind: NodeKind::Function,
            signature: "def good_func(x: int) -> int".to_string(),
            file_path: "clean.py".to_string(),
            line_start: 1,
            line_end: 3,
            docstring: Some("A well-documented function.".to_string()),
            is_public: true,
            type_hints_present: true,
            body_text: "return x + 1".to_string(),
        }],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }
}

fn make_dirty_file() -> FileIndex {
    FileIndex {
        file_path: "dirty.py".to_string(),
        content_hash: 0,
        definitions: vec![Definition {
            name: "bad_func".to_string(),
            kind: NodeKind::Function,
            signature: "def bad_func(data)".to_string(),
            file_path: "dirty.py".to_string(),
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

#[test]
fn test_clean_compile_status_ok() {
    let store = in_memory_store();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[make_clean_file()]);

    assert_eq!(result.status, "ok");
    assert!(result.errors.is_empty());
    assert!(result.warnings.is_empty());
}

#[test]
fn test_clean_compile_exit_code_semantics() {
    // Status "ok" → exit code 0, empty stdout
    let store = in_memory_store();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[make_clean_file()]);

    assert_eq!(result.status, "ok");
    assert!(result.errors.is_empty());
    assert!(result.warnings.is_empty());
}

#[test]
fn test_violations_produce_error_status() {
    let store = in_memory_store();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[make_dirty_file()]);

    assert_eq!(result.status, "error");
    assert!(!result.errors.is_empty());
}

#[test]
fn test_compile_result_includes_files_analyzed() {
    let store = in_memory_store();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[make_clean_file()]);

    assert_eq!(result.files_analyzed, vec!["clean.py"]);
}

#[test]
fn test_compile_result_version_and_command() {
    let store = in_memory_store();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[make_clean_file()]);

    assert_eq!(result.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(result.command, "compile");
}
