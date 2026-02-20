// Tests for compile command JSON output schema (Spec 008 - Output Formats)
use keel_enforce::types::*;
use keel_output::json::JsonFormatter;
use keel_output::OutputFormatter;

fn clean_compile() -> CompileResult {
    CompileResult {
        version: env!("CARGO_PKG_VERSION").into(),
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

fn compile_with_violations() -> CompileResult {
    CompileResult {
        version: env!("CARGO_PKG_VERSION").into(),
        command: "compile".into(),
        status: "error".into(),
        files_analyzed: vec!["src/lib.rs".into(), "src/utils.rs".into()],
        errors: vec![Violation {
            code: "E001".into(),
            severity: "ERROR".into(),
            category: "broken_caller".into(),
            message: "Signature of `foo` changed; 1 caller(s) need updating".into(),
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
        warnings: vec![Violation {
            code: "W001".into(),
            severity: "WARNING".into(),
            category: "placement".into(),
            message: "Function `validate` may belong in `validators.py`".into(),
            file: "src/utils.rs".into(),
            line: 5,
            hash: "warn1111111".into(),
            confidence: 0.6,
            resolution_tier: "heuristic".into(),
            fix_hint: Some("Consider moving `validate` to `validators.py`".into()),
            suppressed: false,
            suppress_hint: None,
            affected: vec![],
            suggested_module: Some("validators.py".into()),
            existing: None,
        }],
        info: CompileInfo {
            nodes_updated: 2,
            edges_updated: 1,
            hashes_changed: vec!["abc12345678".into()],
        },
    }
}

#[test]
fn test_compile_json_has_violations_array() {
    let fmt = JsonFormatter;
    let out = fmt.format_compile(&compile_with_violations());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert!(parsed["errors"].is_array());
    assert!(parsed["warnings"].is_array());
    assert_eq!(parsed["errors"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["warnings"].as_array().unwrap().len(), 1);
}

#[test]
fn test_compile_json_violation_fields() {
    let fmt = JsonFormatter;
    let out = fmt.format_compile(&compile_with_violations());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    let error = &parsed["errors"][0];
    assert_eq!(error["code"], "E001");
    assert_eq!(error["severity"], "ERROR");
    assert!(error["message"].as_str().unwrap().contains("foo"));
    assert_eq!(error["fix_hint"], "Update callers of `foo`");
}

#[test]
fn test_compile_json_violation_location() {
    let fmt = JsonFormatter;
    let out = fmt.format_compile(&compile_with_violations());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    let error = &parsed["errors"][0];
    assert_eq!(error["file"], "src/lib.rs");
    assert_eq!(error["line"], 10);
}

#[test]
fn test_compile_json_violation_metadata() {
    let fmt = JsonFormatter;
    let out = fmt.format_compile(&compile_with_violations());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    let error = &parsed["errors"][0];
    assert_eq!(error["confidence"], 0.92);
    assert_eq!(error["resolution_tier"], "tree-sitter");
    assert_eq!(error["hash"], "abc12345678");
}

#[test]
fn test_compile_json_summary() {
    let fmt = JsonFormatter;
    let out = fmt.format_compile(&compile_with_violations());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(parsed["status"], "error");
    assert_eq!(parsed["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(parsed["command"], "compile");
    assert_eq!(parsed["files_analyzed"].as_array().unwrap().len(), 2);
}

#[test]
fn test_compile_json_empty_violations() {
    let fmt = JsonFormatter;
    let out = fmt.format_compile(&clean_compile());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["errors"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["warnings"].as_array().unwrap().len(), 0);
}

#[test]
fn test_compile_json_validates_schema() {
    let fmt = JsonFormatter;
    let original = compile_with_violations();
    let json = fmt.format_compile(&original);
    let deserialized: CompileResult = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.status, original.status);
    assert_eq!(deserialized.errors.len(), original.errors.len());
    assert_eq!(deserialized.warnings.len(), original.warnings.len());
    assert_eq!(deserialized.errors[0].code, "E001");
    assert_eq!(deserialized.warnings[0].code, "W001");
    assert_eq!(deserialized.errors[0].affected.len(), 1);
    assert_eq!(
        deserialized.warnings[0].suggested_module,
        Some("validators.py".into())
    );
}
