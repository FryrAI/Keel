//! Unit tests for the pieces of `keel compile` that have no other surface.

use super::*;
use keel_enforce::types::{
    CompileDelta, CompileInfo, CompileResult, PressureLevel, Violation, ViolationKey,
};

const FILE: &str = "src/dup.ts";

fn violation(code: &str, hash: &str, line: u32, message: &str) -> Violation {
    Violation {
        code: code.to_string(),
        severity: "WARNING".to_string(),
        category: "test".to_string(),
        message: message.to_string(),
        file: FILE.to_string(),
        line,
        hash: hash.to_string(),
        confidence: 0.85,
        resolution_tier: "heuristic".to_string(),
        fix_hint: None,
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }
}

fn key(v: &Violation) -> ViolationKey {
    ViolationKey::from_violation(v)
}

fn result(warnings: Vec<Violation>) -> CompileResult {
    CompileResult {
        version: "0.0.0".into(),
        command: "compile".into(),
        status: "warning".into(),
        files_analyzed: vec![FILE.into()],
        errors: vec![],
        warnings,
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    }
}

fn delta(new_warnings: Vec<ViolationKey>) -> CompileDelta {
    let count = new_warnings.len() as i32;
    CompileDelta {
        new_errors: vec![],
        resolved_errors: vec![],
        new_warnings,
        resolved_warnings: vec![],
        net_errors: 0,
        net_warnings: count,
        pressure: PressureLevel::Low,
        total_errors: 0,
        total_warnings: 0,
    }
}

/// The annotations `--delta --format github` posts must be exactly the
/// violations the delta calls new.
///
/// A W006 on a definition the graph has no node for — every duplicate in a file
/// created since the last `keel map` — carries no hash, so re-selecting by
/// `(code, hash, file)` alone matched all three copies here against the one new
/// key, and a PR that added one duplicate got the two it inherited annotated as
/// its doing.
#[test]
fn only_the_new_hash_less_violation_is_annotated() {
    let warnings = vec![
        violation("W006", "", 2, "Body of `dupA` is identical to `orig`"),
        violation("W006", "", 8, "Body of `dupB` is identical to `orig`"),
        violation("W006", "", 14, "Body of `dupC` is identical to `orig`"),
    ];
    let new_key = key(&warnings[2]);
    let out = github_delta_annotations(&result(warnings), &delta(vec![new_key]));

    let lines: Vec<&str> = out.lines().filter(|l| l.starts_with("::")).collect();
    assert_eq!(lines.len(), 1, "only the added duplicate is new: {out}");
    assert!(lines[0].contains("dupC"), "{}", lines[0]);
    assert!(lines[0].contains(",line=14"), "{}", lines[0]);
}

/// The other half of the key rule: a violation that DOES carry a hash stays
/// line-independent, so a delta key recorded before the file shifted still
/// selects it.
#[test]
fn a_hash_bearing_violation_is_selected_across_a_line_shift() {
    let warnings = vec![violation("W005", "abc12345678", 40, "`orphan` is unused")];
    let mut shifted = key(&warnings[0]);
    shifted.line = 10; // where it sat before someone added imports above it

    let out = github_delta_annotations(&result(warnings), &delta(vec![shifted]));
    let lines: Vec<&str> = out.lines().filter(|l| l.starts_with("::")).collect();
    assert_eq!(
        lines.len(),
        1,
        "a pure line shift is the same finding: {out}"
    );
    assert!(lines[0].contains(",line=40"), "{}", lines[0]);
}
