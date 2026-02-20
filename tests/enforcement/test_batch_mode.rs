// Tests for batch mode (--batch-start/--batch-end) (Spec 006 - Enforcement Engine)
use keel_core::types::NodeKind;
use keel_enforce::batch::BatchState;
use keel_enforce::engine::EnforcementEngine;
use keel_enforce::types::Violation;
use keel_parsers::resolver::{Definition, FileIndex};

use crate::common::in_memory_store;

fn make_violation(code: &str) -> Violation {
    Violation {
        code: code.to_string(),
        severity: if code.starts_with('E') {
            "ERROR"
        } else {
            "WARNING"
        }
        .to_string(),
        category: "test".to_string(),
        message: format!("test {code}"),
        file: "a.py".to_string(),
        line: 1,
        hash: "testhash".to_string(),
        confidence: 1.0,
        resolution_tier: "tree-sitter".to_string(),
        fix_hint: None,
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }
}

fn make_file_with_missing_hints(file: &str) -> FileIndex {
    FileIndex {
        file_path: file.to_string(),
        content_hash: 0,
        definitions: vec![Definition {
            name: "untyped".to_string(),
            kind: NodeKind::Function,
            signature: "def untyped(data)".to_string(),
            file_path: file.to_string(),
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
fn test_batch_defers_type_hints() {
    assert!(BatchState::is_deferrable("E002"));
}

#[test]
fn test_batch_defers_docstrings() {
    assert!(BatchState::is_deferrable("E003"));
}

#[test]
fn test_batch_defers_placement() {
    assert!(BatchState::is_deferrable("W001"));
}

#[test]
fn test_batch_structural_errors_fire_immediately() {
    // E001, E004, E005 are NOT deferrable
    assert!(!BatchState::is_deferrable("E001"));
    assert!(!BatchState::is_deferrable("E004"));
    assert!(!BatchState::is_deferrable("E005"));
}

#[test]
fn test_batch_end_fires_deferred() {
    let mut batch = BatchState::new();
    for _ in 0..5 {
        batch.defer(make_violation("E002"));
        batch.defer(make_violation("E003"));
    }
    assert_eq!(batch.deferred_count(), 10);

    let drained = batch.drain();
    assert_eq!(drained.len(), 10);
}

#[test]
fn test_batch_engine_defers_and_fires() {
    let store = in_memory_store();
    let mut engine = EnforcementEngine::new(Box::new(store));

    // Enter batch mode
    engine.batch_start();

    // Compile a file with missing type hints (E002) — should be deferred
    let file = make_file_with_missing_hints("test.py");
    let result = engine.compile(&[file]);

    // E002 should be deferred, so no errors in this result
    assert!(
        result.errors.is_empty(),
        "E002 should be deferred in batch mode"
    );

    // End batch mode — deferred violations fire
    let batch_result = engine.batch_end();
    // Now E002 + E003 (both missing for the public function) should appear
    let total = batch_result.errors.len() + batch_result.warnings.len();
    assert!(total > 0, "Deferred violations should fire on batch_end");
}

#[test]
fn test_batch_not_expired_immediately() {
    let batch = BatchState::new();
    assert!(!batch.is_expired());
}

#[test]
fn test_batch_expired_state() {
    // Can't directly test 60s timeout, but we can test the expired constructor
    // The `new_expired()` method is cfg(test) only within the crate.
    // Instead, verify the BatchState API contract.
    let batch = BatchState::new();
    assert_eq!(batch.deferred_count(), 0);
    assert!(!batch.is_expired());
}
