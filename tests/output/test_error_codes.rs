// Tests for error code formatting in output (Spec 008 - Output Formats)
use keel_enforce::types::*;
use keel_output::human::HumanFormatter;
use keel_output::json::JsonFormatter;
use keel_output::OutputFormatter;

fn make_violation(code: &str, severity: &str, category: &str, fix_hint: Option<&str>) -> Violation {
    Violation {
        code: code.into(),
        severity: severity.into(),
        category: category.into(),
        message: format!("Test {code} violation"),
        file: "src/test.py".into(),
        line: 10,
        hash: "testhash1234".into(),
        confidence: 0.85,
        resolution_tier: "tree-sitter".into(),
        fix_hint: fix_hint.map(String::from),
        suppressed: code == "S001",
        suppress_hint: if code == "S001" {
            Some("Suppressed via keel suppress".into())
        } else {
            None
        },
        affected: vec![],
        suggested_module: if code == "W001" {
            Some("validators.py".into())
        } else {
            None
        },
        existing: if code == "W002" {
            Some(ExistingNode {
                hash: "dup12345678".into(),
                file: "src/other.py".into(),
                line: 20,
            })
        } else {
            None
        },
    }
}

fn wrap_in_compile(v: Violation) -> CompileResult {
    let is_error = v.severity == "ERROR";
    CompileResult {
        version: "0.1.0".into(),
        command: "compile".into(),
        status: if is_error { "error" } else { "warning" }.into(),
        files_analyzed: vec![v.file.clone()],
        errors: if is_error { vec![v.clone()] } else { vec![] },
        warnings: if !is_error { vec![v] } else { vec![] },
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    }
}

#[test]
fn test_error_code_e001_format() {
    let v = make_violation("E001", "ERROR", "broken_caller", Some("Update callers"));
    let fmt = HumanFormatter;
    let out = fmt.format_compile(&wrap_in_compile(v));

    assert!(out.contains("error[E001]"));
    assert!(out.contains("1 error(s)"));
}

#[test]
fn test_error_code_e002_format() {
    let v = make_violation(
        "E002",
        "ERROR",
        "missing_type_hints",
        Some("Add type hints"),
    );
    let fmt = HumanFormatter;
    let out = fmt.format_compile(&wrap_in_compile(v));

    assert!(out.contains("error[E002]"));
}

#[test]
fn test_error_code_e003_format() {
    let v = make_violation("E003", "ERROR", "missing_docstring", Some("Add docstring"));
    let fmt = HumanFormatter;
    let out = fmt.format_compile(&wrap_in_compile(v));

    assert!(out.contains("error[E003]"));
}

#[test]
fn test_error_code_e004_format() {
    let v = make_violation(
        "E004",
        "ERROR",
        "function_removed",
        Some("Restore function"),
    );
    let fmt = HumanFormatter;
    let out = fmt.format_compile(&wrap_in_compile(v));

    assert!(out.contains("error[E004]"));
}

#[test]
fn test_error_code_e005_format() {
    let v = make_violation("E005", "ERROR", "arity_mismatch", Some("Update call site"));
    let fmt = HumanFormatter;
    let out = fmt.format_compile(&wrap_in_compile(v));

    assert!(out.contains("error[E005]"));
}

#[test]
fn test_error_code_w001_format() {
    let v = make_violation("W001", "WARNING", "placement", Some("Move function"));
    let fmt = HumanFormatter;
    let out = fmt.format_compile(&wrap_in_compile(v));

    assert!(out.contains("warning[W001]"));
    assert!(out.contains("suggested module: validators.py"));
}

#[test]
fn test_error_code_w002_format() {
    let v = make_violation("W002", "WARNING", "duplicate_name", None);
    let fmt = HumanFormatter;
    let out = fmt.format_compile(&wrap_in_compile(v));

    assert!(out.contains("warning[W002]"));
    assert!(out.contains("also at: src/other.py:20"));
}

#[test]
fn test_error_code_s001_format() {
    let v = make_violation("S001", "INFO", "suppressed", None);
    let fmt = HumanFormatter;
    let result = CompileResult {
        version: "0.1.0".into(),
        command: "compile".into(),
        status: "ok".into(),
        files_analyzed: vec!["src/test.py".into()],
        errors: vec![],
        warnings: vec![v],
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    };
    let out = fmt.format_compile(&result);

    assert!(out.contains("info[S001]"));
    assert!(out.contains("Suppressed via keel suppress"));
}

#[test]
fn test_every_error_has_fix_hint() {
    let error_codes = vec![
        ("E001", "broken_caller"),
        ("E002", "missing_type_hints"),
        ("E003", "missing_docstring"),
        ("E004", "function_removed"),
        ("E005", "arity_mismatch"),
    ];

    let fmt = JsonFormatter;
    for (code, category) in &error_codes {
        let v = make_violation(code, "ERROR", category, Some(&format!("Fix for {code}")));
        let result = wrap_in_compile(v);
        let json = fmt.format_compile(&result);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let fix = parsed["errors"][0]["fix_hint"].as_str();
        assert!(
            fix.is_some() && !fix.unwrap().is_empty(),
            "{code} must have a non-empty fix_hint"
        );
    }
}

#[test]
fn test_all_violations_have_confidence() {
    let all_codes = vec![
        ("E001", "ERROR", "broken_caller"),
        ("E002", "ERROR", "missing_type_hints"),
        ("E003", "ERROR", "missing_docstring"),
        ("E004", "ERROR", "function_removed"),
        ("E005", "ERROR", "arity_mismatch"),
        ("W001", "WARNING", "placement"),
        ("W002", "WARNING", "duplicate_name"),
    ];

    let fmt = JsonFormatter;
    for (code, severity, category) in &all_codes {
        let v = make_violation(code, severity, category, Some("fix"));
        let result = wrap_in_compile(v);
        let json = fmt.format_compile(&result);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let key = if *severity == "ERROR" {
            "errors"
        } else {
            "warnings"
        };
        let conf = parsed[key][0]["confidence"].as_f64().unwrap();
        assert!(
            (0.0..=1.0).contains(&conf),
            "{code} confidence {conf} must be in [0.0, 1.0]"
        );
    }
}
