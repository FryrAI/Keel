use keel_enforce::types::{AffectedNode, CompileInfo, CompileResult, ExistingNode, Violation};

/// Create a CompileResult representing a clean compile (zero errors, zero warnings).
///
/// When compile passes cleanly, keel outputs empty stdout and exit 0.
pub fn create_clean_compile() -> CompileResult {
    CompileResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "compile".to_string(),
        status: "ok".to_string(),
        files_analyzed: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
        errors: vec![],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 5,
            edges_updated: 3,
            hashes_changed: vec![],
        },
    }
}

/// Create a CompileResult with 2 E001 (broken_caller) errors.
pub fn create_compile_with_errors() -> CompileResult {
    CompileResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "compile".to_string(),
        status: "error".to_string(),
        files_analyzed: vec![
            "src/api.rs".to_string(),
            "src/auth.rs".to_string(),
        ],
        errors: vec![
            Violation {
                code: "E001".to_string(),
                severity: "ERROR".to_string(),
                category: "broken_caller".to_string(),
                message: "Function `authenticate` signature changed; caller `handle_request` passes incompatible arguments".to_string(),
                file: "src/api.rs".to_string(),
                line: 10,
                hash: "fn_api_00009".to_string(),
                confidence: 0.95,
                resolution_tier: "tier1".to_string(),
                fix_hint: Some("Update call at src/api.rs:10 to match new signature: fn authenticate(token: &str, scope: &str) -> Result<User, AuthError>".to_string()),
                suppressed: false,
                suppress_hint: Some("Add `// keel:suppress E001 fn_api_00009` above the call site".to_string()),
                affected: vec![
                    AffectedNode {
                        hash: "fn_auth_00001".to_string(),
                        name: "authenticate".to_string(),
                        file: "src/auth.rs".to_string(),
                        line: 5,
                    },
                ],
                suggested_module: None,
                existing: None,
            },
            Violation {
                code: "E001".to_string(),
                severity: "ERROR".to_string(),
                category: "broken_caller".to_string(),
                message: "Function `validate_token` was removed; caller `authenticate` still references it".to_string(),
                file: "src/auth.rs".to_string(),
                line: 15,
                hash: "fn_auth_00001".to_string(),
                confidence: 1.0,
                resolution_tier: "tier1".to_string(),
                fix_hint: Some("Remove or replace call to `validate_token` at src/auth.rs:15".to_string()),
                suppressed: false,
                suppress_hint: Some("Add `// keel:suppress E001 fn_auth_00001` above the call site".to_string()),
                affected: vec![
                    AffectedNode {
                        hash: "fn_auth_00003".to_string(),
                        name: "validate_token".to_string(),
                        file: "src/auth.rs".to_string(),
                        line: 42,
                    },
                ],
                suggested_module: None,
                existing: None,
            },
        ],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 2,
            edges_updated: 1,
            hashes_changed: vec![
                "fn_auth_00001".to_string(),
            ],
        },
    }
}

/// Create a CompileResult with 1 W001 (placement) and 1 W002 (duplicate_name) warning.
pub fn create_compile_with_warnings() -> CompileResult {
    CompileResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "compile".to_string(),
        status: "warning".to_string(),
        files_analyzed: vec![
            "src/users.rs".to_string(),
            "src/utils.rs".to_string(),
        ],
        errors: vec![],
        warnings: vec![
            Violation {
                code: "W001".to_string(),
                severity: "WARNING".to_string(),
                category: "placement".to_string(),
                message: "Function `hash_password` in src/utils.rs may belong in src/auth.rs based on responsibility analysis".to_string(),
                file: "src/utils.rs".to_string(),
                line: 5,
                hash: "fn_util_00017".to_string(),
                confidence: 0.72,
                resolution_tier: "tier1".to_string(),
                fix_hint: None,
                suppressed: false,
                suppress_hint: Some("Add `// keel:suppress W001 fn_util_00017` to acknowledge placement".to_string()),
                affected: vec![],
                suggested_module: Some("src/auth.rs".to_string()),
                existing: None,
            },
            Violation {
                code: "W002".to_string(),
                severity: "WARNING".to_string(),
                category: "duplicate_name".to_string(),
                message: "Function `query` in src/db.rs has same name as function in src/cache.rs".to_string(),
                file: "src/db.rs".to_string(),
                line: 17,
                hash: "fn_db_000014".to_string(),
                confidence: 0.85,
                resolution_tier: "tier1".to_string(),
                fix_hint: None,
                suppressed: false,
                suppress_hint: Some("Add `// keel:suppress W002 fn_db_000014` if intentional".to_string()),
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
    }
}

/// Create a CompileResult with a mix of errors, warnings, and info.
pub fn create_compile_mixed() -> CompileResult {
    CompileResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "compile".to_string(),
        status: "error".to_string(),
        files_analyzed: vec![
            "src/api.rs".to_string(),
            "src/auth.rs".to_string(),
            "src/users.rs".to_string(),
            "src/db.rs".to_string(),
        ],
        errors: vec![
            Violation {
                code: "E005".to_string(),
                severity: "ERROR".to_string(),
                category: "arity_mismatch".to_string(),
                message: "Function `create_user` expects 3 arguments but caller passes 2".to_string(),
                file: "src/api.rs".to_string(),
                line: 25,
                hash: "fn_api_00009".to_string(),
                confidence: 1.0,
                resolution_tier: "tier1".to_string(),
                fix_hint: Some("Add missing argument `role: &str` to call at src/api.rs:25".to_string()),
                suppressed: false,
                suppress_hint: Some("Add `// keel:suppress E005 fn_api_00009` above the call".to_string()),
                affected: vec![
                    AffectedNode {
                        hash: "fn_user_00006".to_string(),
                        name: "create_user".to_string(),
                        file: "src/users.rs".to_string(),
                        line: 27,
                    },
                ],
                suggested_module: None,
                existing: None,
            },
        ],
        warnings: vec![
            Violation {
                code: "W001".to_string(),
                severity: "WARNING".to_string(),
                category: "placement".to_string(),
                message: "Function `connect` in src/db.rs may belong in src/pool.rs based on responsibility analysis".to_string(),
                file: "src/db.rs".to_string(),
                line: 5,
                hash: "fn_db_000013".to_string(),
                confidence: 0.60,
                resolution_tier: "tier1".to_string(),
                fix_hint: None,
                suppressed: false,
                suppress_hint: Some("Add `// keel:suppress W001 fn_db_000013` to acknowledge placement".to_string()),
                affected: vec![],
                suggested_module: Some("src/pool.rs".to_string()),
                existing: None,
            },
        ],
        info: CompileInfo {
            nodes_updated: 8,
            edges_updated: 12,
            hashes_changed: vec![
                "fn_user_00006".to_string(),
                "fn_api_00009".to_string(),
            ],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_compile_has_no_violations() {
        let result = create_clean_compile();
        assert_eq!(result.status, "ok");
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_compile_with_errors_has_two_e001() {
        let result = create_compile_with_errors();
        assert_eq!(result.status, "error");
        assert_eq!(result.errors.len(), 2);
        assert!(result.errors.iter().all(|e| e.code == "E001"));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_compile_with_warnings_has_w001_and_w002() {
        let result = create_compile_with_warnings();
        assert_eq!(result.status, "warning");
        assert!(result.errors.is_empty());
        assert_eq!(result.warnings.len(), 2);
        assert_eq!(result.warnings[0].code, "W001");
        assert_eq!(result.warnings[1].code, "W002");
    }

    #[test]
    fn test_compile_mixed_has_errors_and_warnings() {
        let result = create_compile_mixed();
        assert_eq!(result.status, "error");
        assert!(!result.errors.is_empty());
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_all_errors_have_fix_hints() {
        let result = create_compile_with_errors();
        for error in &result.errors {
            assert!(
                error.fix_hint.is_some(),
                "ERROR {} should have a fix_hint",
                error.code
            );
        }
    }

    #[test]
    fn test_all_violations_have_confidence() {
        let result = create_compile_mixed();
        for v in result.errors.iter().chain(result.warnings.iter()) {
            assert!(
                v.confidence >= 0.0 && v.confidence <= 1.0,
                "Violation {} has invalid confidence: {}",
                v.code,
                v.confidence
            );
        }
    }
}
