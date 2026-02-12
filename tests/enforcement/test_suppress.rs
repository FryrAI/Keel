// Tests for violation suppression (Spec 006 - Enforcement Engine)
use keel_enforce::engine::EnforcementEngine;
use keel_enforce::suppress::SuppressionManager;
use keel_enforce::types::Violation;
use keel_parsers::resolver::{Definition, FileIndex};
use keel_core::types::NodeKind;

use crate::common::in_memory_store;

fn test_violation(code: &str) -> Violation {
    Violation {
        code: code.to_string(),
        severity: "ERROR".to_string(),
        category: "test".to_string(),
        message: format!("test violation {code}"),
        file: "a.py".to_string(),
        line: 1,
        hash: "abc".to_string(),
        confidence: 1.0,
        resolution_tier: "tree-sitter".to_string(),
        fix_hint: Some("fix it".to_string()),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }
}

#[test]
fn test_suppress_changes_to_s001() {
    let mut mgr = SuppressionManager::new();
    mgr.suppress("E002");

    let v = test_violation("E002");
    let result = mgr.apply(v);

    assert_eq!(result.code, "S001");
    assert_eq!(result.severity, "INFO");
    assert!(result.suppressed);
}

#[test]
fn test_suppressed_emits_s001_with_hint() {
    let mut mgr = SuppressionManager::new();
    mgr.suppress("E003");

    let v = test_violation("E003");
    let result = mgr.apply(v);

    assert_eq!(result.code, "S001");
    assert!(result.suppress_hint.is_some());
    let hint = result.suppress_hint.unwrap();
    assert!(hint.contains("E003"));
    assert!(hint.contains("Suppressed"));
}

#[test]
fn test_unsuppressed_passthrough() {
    let mgr = SuppressionManager::new();
    let v = test_violation("E001");
    let result = mgr.apply(v);

    assert_eq!(result.code, "E001");
    assert_eq!(result.severity, "ERROR");
    assert!(!result.suppressed);
    assert!(result.suppress_hint.is_none());
}

#[test]
fn test_suppress_multiple_codes() {
    let mut mgr = SuppressionManager::new();
    mgr.suppress("E002");
    mgr.suppress("E003");

    assert!(mgr.is_suppressed("E002"));
    assert!(mgr.is_suppressed("E003"));
    assert!(!mgr.is_suppressed("E001"));
    assert_eq!(mgr.count(), 2);
}

#[test]
fn test_suppress_via_engine() {
    let store = in_memory_store();
    let mut engine = EnforcementEngine::new(Box::new(store));
    engine.suppress("E002");

    // Compile a file with missing type hints
    let file = FileIndex {
        file_path: "test.py".to_string(),
        content_hash: 0,
        definitions: vec![Definition {
            name: "untyped".to_string(),
            kind: NodeKind::Function,
            signature: "def untyped(data)".to_string(),
            file_path: "test.py".to_string(),
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
    };

    let result = engine.compile(&[file]);
    // E002 should be suppressed to S001 (INFO), not in errors
    let e002_in_errors = result.errors.iter().any(|v| v.code == "E002");
    assert!(!e002_in_errors, "E002 should be suppressed");
}

#[test]
fn test_suppress_only_affects_specified_code() {
    let mut mgr = SuppressionManager::new();
    mgr.suppress("E002");

    let v_e001 = test_violation("E001");
    let v_e002 = test_violation("E002");

    let r1 = mgr.apply(v_e001);
    let r2 = mgr.apply(v_e002);

    assert_eq!(r1.code, "E001"); // Unchanged
    assert_eq!(r2.code, "S001"); // Suppressed
}

#[test]
fn test_suppressed_violation_severity_is_info() {
    let mut mgr = SuppressionManager::new();
    mgr.suppress("W001");

    let v = Violation {
        code: "W001".to_string(),
        severity: "WARNING".to_string(),
        category: "placement".to_string(),
        message: "misplaced".to_string(),
        file: "a.py".to_string(),
        line: 1,
        hash: String::new(),
        confidence: 0.6,
        resolution_tier: "heuristic".to_string(),
        fix_hint: None,
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    };

    let result = mgr.apply(v);
    assert_eq!(result.severity, "INFO");
    assert!(result.suppressed);
}
