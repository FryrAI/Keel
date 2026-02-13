// Tests for LLM-friendly output format (Spec 008 - Output Formats)
use keel_enforce::types::*;
use keel_output::llm::LlmFormatter;
use keel_output::OutputFormatter;

fn clean_compile() -> CompileResult {
    CompileResult {
        version: "0.1.0".into(),
        command: "compile".into(),
        status: "ok".into(),
        files_analyzed: vec!["src/main.rs".into()],
        errors: vec![],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    }
}

fn make_violation(code: &str, severity: &str, category: &str, file: &str) -> Violation {
    Violation {
        code: code.into(),
        severity: severity.into(),
        category: category.into(),
        message: format!("Test violation {code}"),
        file: file.into(),
        line: 10,
        hash: "hash1234567".into(),
        confidence: 0.85,
        resolution_tier: "tree-sitter".into(),
        fix_hint: Some(format!("Fix for {code}")),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }
}

fn compile_with_error() -> CompileResult {
    CompileResult {
        version: "0.1.0".into(),
        command: "compile".into(),
        status: "error".into(),
        files_analyzed: vec!["src/lib.rs".into()],
        errors: vec![Violation {
            code: "E001".into(),
            severity: "ERROR".into(),
            category: "broken_caller".into(),
            message: "Signature of `foo` changed".into(),
            file: "src/lib.rs".into(),
            line: 10,
            hash: "abc12345678".into(),
            confidence: 0.92,
            resolution_tier: "tree-sitter".into(),
            fix_hint: Some("Update callers of `foo`".into()),
            suppressed: false,
            suppress_hint: None,
            affected: vec![AffectedNode {
                hash: "def11111111".into(),
                name: "bar".into(),
                file: "src/bar.rs".into(),
                line: 20,
            }],
            suggested_module: None,
            existing: None,
        }],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 1,
            edges_updated: 0,
            hashes_changed: vec!["abc12345678".into()],
        },
    }
}

#[test]
fn test_llm_format_structure() {
    let fmt = LlmFormatter::new();
    let out = fmt.format_compile(&compile_with_error());

    // LLM format uses COMPILE header with counts
    assert!(out.contains("COMPILE"));
    assert!(out.contains("files=1"));
    assert!(out.contains("errors=1"));
    assert!(out.contains("warnings=0"));
}

#[test]
fn test_llm_format_fix_hint_prominent() {
    let fmt = LlmFormatter::new(); // depth 1 shows FIX inline
    let out = fmt.format_compile(&compile_with_error());

    // fix_hint appears with FIX: prefix at all depths
    assert!(out.contains("FIX: Update callers of `foo`"));
}

#[test]
fn test_llm_format_includes_location() {
    let fmt = LlmFormatter::with_depths(1, 2); // depth 2 shows AFFECTED
    let out = fmt.format_compile(&compile_with_error());

    // Affected callers show file:line at depth 2
    assert!(out.contains("src/bar.rs:20"));
}

#[test]
fn test_llm_format_multiple_violations() {
    let fmt = LlmFormatter::new();
    let result = CompileResult {
        version: "0.1.0".into(),
        command: "compile".into(),
        status: "error".into(),
        files_analyzed: vec!["a.py".into(), "b.py".into(), "c.py".into()],
        errors: vec![
            make_violation("E001", "ERROR", "broken_caller", "a.py"),
            make_violation("E002", "ERROR", "missing_type_hints", "b.py"),
        ],
        warnings: vec![make_violation("W001", "WARNING", "placement", "c.py")],
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    };
    let out = fmt.format_compile(&result);

    // All violations appear
    assert!(out.contains("E001"));
    assert!(out.contains("E002"));
    assert!(out.contains("W001"));
    assert!(out.contains("errors=2"));
    assert!(out.contains("warnings=1"));
}

#[test]
fn test_llm_format_circuit_breaker_context() {
    // Circuit breaker context manifests as AFFECTED callers in the output (depth 2)
    let fmt = LlmFormatter::with_depths(1, 2);
    let result = CompileResult {
        version: "0.1.0".into(),
        command: "compile".into(),
        status: "error".into(),
        files_analyzed: vec!["src/lib.rs".into()],
        errors: vec![Violation {
            code: "E001".into(),
            severity: "ERROR".into(),
            category: "broken_caller".into(),
            message: "Signature changed".into(),
            file: "src/lib.rs".into(),
            line: 10,
            hash: "abc12345678".into(),
            confidence: 0.92,
            resolution_tier: "tree-sitter".into(),
            fix_hint: Some("Update callers".into()),
            suppressed: false,
            suppress_hint: None,
            affected: vec![
                AffectedNode {
                    hash: "cal11111111".into(),
                    name: "caller1".into(),
                    file: "src/a.rs".into(),
                    line: 5,
                },
                AffectedNode {
                    hash: "cal22222222".into(),
                    name: "caller2".into(),
                    file: "src/b.rs".into(),
                    line: 15,
                },
            ],
            suggested_module: None,
            existing: None,
        }],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    };
    let out = fmt.format_compile(&result);

    assert!(out.contains("callers=2"));
    assert!(out.contains("AFFECTED:"));
    assert!(out.contains("cal11111111@src/a.rs:5"));
    assert!(out.contains("cal22222222@src/b.rs:15"));
}

#[test]
fn test_llm_format_clean_compile() {
    let fmt = LlmFormatter::new();
    let out = fmt.format_compile(&clean_compile());

    // Clean compile = empty string for LLM (no noise)
    assert!(out.is_empty(), "Clean compile must produce empty output");
}

#[test]
fn test_llm_format_error_code_category() {
    let fmt = LlmFormatter::new();
    let result = CompileResult {
        version: "0.1.0".into(),
        command: "compile".into(),
        status: "error".into(),
        files_analyzed: vec!["a.py".into()],
        errors: vec![make_violation("E001", "ERROR", "broken_caller", "a.py")],
        warnings: vec![make_violation("W001", "WARNING", "placement", "b.py")],
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    };
    let out = fmt.format_compile(&result);

    // Category appears alongside code
    assert!(out.contains("E001 broken_caller"));
    assert!(out.contains("W001 placement"));
}
