// Tests for LLM output token budget management (Spec 008 - Output Formats)
//
// Note: The current LlmFormatter does not implement token budgeting yet.
// These tests verify the current behavior (all violations output) and
// document the expected behavior when token budgeting is implemented.
use keel_enforce::types::*;
use keel_output::llm::LlmFormatter;
use keel_output::OutputFormatter;

fn make_violation(code: &str, idx: usize) -> Violation {
    Violation {
        code: code.into(),
        severity: if code.starts_with('E') {
            "ERROR"
        } else {
            "WARNING"
        }
        .into(),
        category: format!("cat_{code}"),
        message: format!("Violation {idx}: test message for {code}"),
        file: format!("src/file_{idx}.py"),
        line: idx as u32,
        hash: format!("hash{idx:08}"),
        confidence: 0.85,
        resolution_tier: "tree-sitter".into(),
        fix_hint: Some(format!("Fix violation {idx}")),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }
}

fn compile_with_many_violations(error_count: usize, warning_count: usize) -> CompileResult {
    let errors: Vec<Violation> = (0..error_count)
        .map(|i| make_violation("E001", i))
        .collect();
    let warnings: Vec<Violation> = (0..warning_count)
        .map(|i| make_violation("W001", error_count + i))
        .collect();
    let files: Vec<String> = (0..(error_count + warning_count))
        .map(|i| format!("src/file_{i}.py"))
        .collect();

    CompileResult {
        version: "0.1.0".into(),
        command: "compile".into(),
        status: if error_count > 0 {
            "error"
        } else if warning_count > 0 {
            "warning"
        } else {
            "ok"
        }
        .into(),
        files_analyzed: files,
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
fn test_llm_token_budget_respected() {
    // Current behavior: all violations output. Verify output is finite and structured.
    let fmt = LlmFormatter;
    let result = compile_with_many_violations(20, 30);
    let out = fmt.format_compile(&result);

    // Output should be non-empty with structured content
    assert!(!out.is_empty());
    assert!(out.contains("COMPILE"));
    assert!(out.contains("errors=20"));
    assert!(out.contains("warnings=30"));
}

#[test]
fn test_llm_token_budget_prioritizes_errors() {
    // Errors appear before warnings in the output
    let fmt = LlmFormatter;
    let result = compile_with_many_violations(5, 10);
    let out = fmt.format_compile(&result);

    // Find first error and first warning position
    let first_error = out.find("E001").unwrap();
    let first_warning = out.find("W001").unwrap();
    assert!(
        first_error < first_warning,
        "Errors must appear before warnings"
    );
}

#[test]
fn test_llm_token_budget_truncation_notice() {
    // With many violations, output should still contain all of them currently
    let fmt = LlmFormatter;
    let result = compile_with_many_violations(50, 0);
    let out = fmt.format_compile(&result);

    // All 50 errors should produce FIX: lines
    let fix_count = out.matches("FIX:").count();
    assert_eq!(fix_count, 50);
}

#[test]
fn test_llm_no_token_budget() {
    // Without budget constraint, all violations included
    let fmt = LlmFormatter;
    let result = compile_with_many_violations(10, 10);
    let out = fmt.format_compile(&result);

    // Count violation lines: each has "E001" or "W001" in the code line
    let e001_count = out.matches("E001 cat_E001").count();
    let w001_count = out.matches("W001 cat_W001").count();
    assert_eq!(e001_count, 10);
    assert_eq!(w001_count, 10);
}

#[test]
fn test_llm_token_budget_accounts_for_fix_hints() {
    // fix_hints are included in output for every violation
    let fmt = LlmFormatter;
    let result = compile_with_many_violations(5, 0);
    let out = fmt.format_compile(&result);

    for i in 0..5 {
        assert!(
            out.contains(&format!("FIX: Fix violation {i}")),
            "Fix hint for violation {i} should be present"
        );
    }
}

#[test]
fn test_llm_default_token_budget() {
    // Default behavior: no truncation, all violations output
    let fmt = LlmFormatter;
    let result = compile_with_many_violations(3, 2);
    let out = fmt.format_compile(&result);

    assert!(out.contains("errors=3"));
    assert!(out.contains("warnings=2"));
    // All fix hints present
    assert_eq!(out.matches("FIX:").count(), 5);
}
