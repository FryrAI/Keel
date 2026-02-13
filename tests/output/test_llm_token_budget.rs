// Tests for LLM output token budget management (Spec 008 - Output Formats)
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
fn test_llm_depth1_has_backpressure() {
    let fmt = LlmFormatter::new(); // depth 1
    let result = compile_with_many_violations(20, 30);
    let out = fmt.format_compile(&result);
    assert!(out.contains("PRESSURE=HIGH"));
    assert!(out.contains("BUDGET=stop_generating"));
}

#[test]
fn test_llm_depth1_groups_by_file() {
    let fmt = LlmFormatter::new();
    let result = compile_with_many_violations(5, 0);
    let out = fmt.format_compile(&result);
    // Each file should be grouped
    assert!(out.contains("FILE src/file_0.py"));
}

#[test]
fn test_llm_depth2_shows_all_violations() {
    let fmt = LlmFormatter::with_depths(1, 2); // compile_depth=2
    let result = compile_with_many_violations(50, 0);
    let out = fmt.format_compile(&result);
    // Depth 2 = full detail, all violations shown
    let fix_count = out.matches("FIX:").count();
    assert_eq!(fix_count, 50);
}

#[test]
fn test_llm_depth2_errors_before_warnings() {
    let fmt = LlmFormatter::with_depths(1, 2);
    let result = compile_with_many_violations(5, 10);
    let out = fmt.format_compile(&result);
    let first_error = out.find("E001").unwrap();
    let first_warning = out.find("W001").unwrap();
    assert!(first_error < first_warning, "Errors must appear before warnings");
}

#[test]
fn test_llm_depth0_counts_only() {
    let fmt = LlmFormatter::with_depths(1, 0); // compile_depth=0
    let result = compile_with_many_violations(3, 2);
    let out = fmt.format_compile(&result);
    assert_eq!(out.lines().count(), 1);
    assert!(out.contains("errors=3"));
    assert!(out.contains("warnings=2"));
    assert!(out.contains("PRESSURE=MED"));
}

#[test]
fn test_llm_depth2_includes_fix_hints() {
    let fmt = LlmFormatter::with_depths(1, 2);
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
fn test_llm_pressure_levels() {
    // LOW: 0-2 errors
    let fmt = LlmFormatter::with_depths(1, 0);
    let out = fmt.format_compile(&compile_with_many_violations(1, 0));
    assert!(out.contains("PRESSURE=LOW"));
    assert!(out.contains("BUDGET=keep_going"));

    // MED: 3-5 errors
    let out = fmt.format_compile(&compile_with_many_violations(4, 0));
    assert!(out.contains("PRESSURE=MED"));
    assert!(out.contains("BUDGET=fix_before_adding_more"));

    // HIGH: 6+ errors
    let out = fmt.format_compile(&compile_with_many_violations(10, 0));
    assert!(out.contains("PRESSURE=HIGH"));
    assert!(out.contains("BUDGET=stop_generating"));
}
