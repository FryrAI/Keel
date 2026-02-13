/// Contract tests for compile output JSON schema compliance.
use keel_enforce::types::{
    AffectedNode, CompileInfo, CompileResult, ExistingNode, Violation,
};

use super::test_schema_helpers::validate_against_schema;

#[test]
fn compile_output_matches_schema() {
    let result = CompileResult {
        version: "0.1.0".to_string(),
        command: "compile".to_string(),
        status: "ok".to_string(),
        files_analyzed: vec!["src/main.rs".to_string()],
        errors: vec![],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    };

    let json_value = serde_json::to_value(&result).unwrap();
    let schema_str = include_str!("../schemas/compile_output.schema.json");
    validate_against_schema(&json_value, schema_str);
}

#[test]
fn compile_output_with_errors_matches_schema() {
    let result = CompileResult {
        version: "0.1.0".to_string(),
        command: "compile".to_string(),
        status: "error".to_string(),
        files_analyzed: vec!["src/api.rs".to_string(), "src/auth.rs".to_string()],
        errors: vec![
            Violation {
                code: "E001".to_string(),
                severity: "ERROR".to_string(),
                category: "broken_caller".to_string(),
                message: "Function `authenticate` signature changed".to_string(),
                file: "src/api.rs".to_string(),
                line: 10,
                hash: "fn_api_00009".to_string(),
                confidence: 0.95,
                resolution_tier: "tier1".to_string(),
                fix_hint: Some("Update call at src/api.rs:10".to_string()),
                suppressed: false,
                suppress_hint: Some("Add suppress comment".to_string()),
                affected: vec![AffectedNode {
                    hash: "fn_auth_00001".to_string(),
                    name: "authenticate".to_string(),
                    file: "src/auth.rs".to_string(),
                    line: 5,
                }],
                suggested_module: None,
                existing: None,
            },
            Violation {
                code: "E005".to_string(),
                severity: "ERROR".to_string(),
                category: "arity_mismatch".to_string(),
                message: "Function expects 3 args, caller passes 2".to_string(),
                file: "src/api.rs".to_string(),
                line: 25,
                hash: "fn_api_00010".to_string(),
                confidence: 1.0,
                resolution_tier: "tier1".to_string(),
                fix_hint: Some("Add missing argument".to_string()),
                suppressed: false,
                suppress_hint: None,
                affected: vec![],
                suggested_module: None,
                existing: None,
            },
        ],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 2,
            edges_updated: 1,
            hashes_changed: vec!["fn_auth_00001".to_string()],
        },
    };

    let json_value = serde_json::to_value(&result).unwrap();
    let schema_str = include_str!("../schemas/compile_output.schema.json");
    validate_against_schema(&json_value, schema_str);
}

#[test]
fn compile_output_with_warnings_matches_schema() {
    let result = CompileResult {
        version: "0.1.0".to_string(),
        command: "compile".to_string(),
        status: "warning".to_string(),
        files_analyzed: vec!["src/utils.rs".to_string(), "src/db.rs".to_string()],
        errors: vec![],
        warnings: vec![
            Violation {
                code: "W001".to_string(),
                severity: "WARNING".to_string(),
                category: "placement".to_string(),
                message: "Function may belong in another module".to_string(),
                file: "src/utils.rs".to_string(),
                line: 5,
                hash: "fn_util_00017".to_string(),
                confidence: 0.72,
                resolution_tier: "tier1".to_string(),
                fix_hint: None,
                suppressed: false,
                suppress_hint: Some("Add suppress comment".to_string()),
                affected: vec![],
                suggested_module: Some("src/auth.rs".to_string()),
                existing: None,
            },
            Violation {
                code: "W002".to_string(),
                severity: "WARNING".to_string(),
                category: "duplicate_name".to_string(),
                message: "Duplicate function name across modules".to_string(),
                file: "src/db.rs".to_string(),
                line: 17,
                hash: "fn_db_000014".to_string(),
                confidence: 0.85,
                resolution_tier: "tier1".to_string(),
                fix_hint: None,
                suppressed: false,
                suppress_hint: None,
                affected: vec![],
                suggested_module: None,
                existing: Some(ExistingNode {
                    hash: "fn_cache_0042".to_string(),
                    file: "src/cache.rs".to_string(),
                    line: 30,
                }),
            },
        ],
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    };

    let json_value = serde_json::to_value(&result).unwrap();
    let schema_str = include_str!("../schemas/compile_output.schema.json");
    validate_against_schema(&json_value, schema_str);
}
