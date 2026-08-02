//! Snapshot/delta tests — chiefly that the diff identity is line-independent.

use super::*;
use crate::types::{CompileInfo, Violation};

fn violation(code: &str, hash: &str, file: &str, line: u32) -> Violation {
    Violation {
        code: code.to_string(),
        severity: if code.starts_with('E') {
            "ERROR".to_string()
        } else {
            "WARNING".to_string()
        },
        category: "test".to_string(),
        message: format!("{code} at {file}:{line}"),
        file: file.to_string(),
        line,
        hash: hash.to_string(),
        confidence: 1.0,
        resolution_tier: "tier1".to_string(),
        fix_hint: None,
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }
}

fn result(errors: Vec<Violation>, warnings: Vec<Violation>) -> CompileResult {
    CompileResult {
        version: "0.0.0".into(),
        command: "compile".into(),
        status: if errors.is_empty() { "ok" } else { "error" }.into(),
        files_analyzed: vec!["src/lib.rs".into()],
        errors,
        warnings,
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    }
}

#[test]
fn a_pure_line_shift_is_not_a_new_violation() {
    // Same violation, ten lines further down the file (someone added an
    // import block above it). The hash is AST-derived, so it is unchanged.
    let before = ViolationSnapshot::from_compile_result(&result(
        vec![violation("E003", "abc12345678", "src/lib.rs", 10)],
        vec![violation("W005", "def12345678", "src/lib.rs", 40)],
    ));
    let after = result(
        vec![violation("E003", "abc12345678", "src/lib.rs", 20)],
        vec![violation("W005", "def12345678", "src/lib.rs", 50)],
    );

    let delta = compute_delta(&before, &after);
    assert!(delta.new_errors.is_empty(), "{:?}", delta.new_errors);
    assert!(delta.new_warnings.is_empty(), "{:?}", delta.new_warnings);
    assert!(delta.resolved_errors.is_empty());
    assert!(delta.resolved_warnings.is_empty());
    assert_eq!(delta.net_errors, 0);
    assert_eq!(delta.net_warnings, 0);
}

#[test]
fn a_changed_hash_is_a_new_violation() {
    let before = ViolationSnapshot::from_compile_result(&result(
        vec![violation("E003", "abc12345678", "src/lib.rs", 10)],
        vec![],
    ));
    // The function itself changed (new hash) and still has no docstring.
    let after = result(
        vec![violation("E003", "zzz12345678", "src/lib.rs", 10)],
        vec![],
    );

    let delta = compute_delta(&before, &after);
    assert_eq!(delta.new_errors.len(), 1);
    assert_eq!(delta.new_errors[0].hash, "zzz12345678");
    assert_eq!(delta.resolved_errors.len(), 1);
    assert_eq!(delta.net_errors, 0);
}

#[test]
fn the_display_line_survives_into_the_delta() {
    let before = ViolationSnapshot::from_compile_result(&result(vec![], vec![]));
    let after = result(
        vec![violation("E002", "abc12345678", "src/lib.rs", 77)],
        vec![],
    );

    let delta = compute_delta(&before, &after);
    assert_eq!(delta.new_errors.len(), 1);
    assert_eq!(delta.new_errors[0].line, 77, "line is kept for display");
}

#[test]
fn differences_come_out_in_a_deterministic_order() {
    let before = ViolationSnapshot::from_compile_result(&result(vec![], vec![]));
    let after = result(
        vec![
            violation("E003", "ccc", "src/c.rs", 1),
            violation("E002", "aaa", "src/a.rs", 1),
            violation("E003", "bbb", "src/b.rs", 1),
        ],
        vec![],
    );

    let first = compute_delta(&before, &after);
    let again = compute_delta(&before, &after);
    let codes: Vec<&str> = first.new_errors.iter().map(|k| k.code.as_str()).collect();
    assert_eq!(codes, vec!["E002", "E003", "E003"]);
    assert_eq!(
        codes,
        again
            .new_errors
            .iter()
            .map(|k| k.code.as_str())
            .collect::<Vec<_>>()
    );
}

/// A finding on a definition the graph has no node for carries no hash — every
/// W006 in a file created since the last `keel map`, for instance. With
/// `(code, hash, file)` alone, five of them in one file were ONE key: the delta
/// counted one, and resolving four showed as nothing resolved.
#[test]
fn hash_less_findings_in_one_file_are_counted_separately() {
    let before = ViolationSnapshot::from_compile_result(&result(vec![], vec![]));
    let after = result(
        vec![],
        vec![
            violation("W006", "", "src/lib.rs", 20),
            violation("W006", "", "src/lib.rs", 61),
        ],
    );

    let delta = compute_delta(&before, &after);
    assert_eq!(
        delta.new_warnings.len(),
        2,
        "two duplicate implementations are two findings: {:?}",
        delta.new_warnings
    );
    assert_eq!(delta.net_warnings, 2);

    // And fixing exactly one of them resolves exactly one.
    let both = ViolationSnapshot::from_compile_result(&after);
    let one_left = result(vec![], vec![violation("W006", "", "src/lib.rs", 61)]);
    let delta = compute_delta(&both, &one_left);
    assert!(delta.new_warnings.is_empty());
    assert_eq!(delta.resolved_warnings.len(), 1);
    assert_eq!(delta.resolved_warnings[0].line, 20);
}

#[test]
fn resolved_violations_are_reported_from_the_previous_side() {
    let before = ViolationSnapshot::from_compile_result(&result(
        vec![violation("E002", "abc12345678", "src/lib.rs", 3)],
        vec![],
    ));
    let after = result(vec![], vec![]);

    let delta = compute_delta(&before, &after);
    assert_eq!(delta.resolved_errors.len(), 1);
    assert_eq!(delta.resolved_errors[0].line, 3);
    assert_eq!(delta.net_errors, -1);
}
