use keel_enforce::types::{FixApplyResult, FixResult};

/// Formats fix plans showing violations, causes, and suggested code changes for each call site.
pub fn format_fix(result: &FixResult) -> String {
    if result.plans.is_empty() {
        return "FIX 0 violations — nothing to fix\n".to_string();
    }

    let mut files: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for plan in &result.plans {
        for action in &plan.actions {
            files.insert(&action.file);
        }
    }

    let mut out = format!(
        "FIX {} violations in {} files\n",
        result.violations_addressed,
        files.len(),
    );

    for plan in &result.plans {
        out.push_str(&format!(
            "\nVIOLATION {} hash={} {} on `{}`\n",
            plan.code, plan.hash, plan.category, plan.target_name,
        ));
        out.push_str(&format!("  CAUSE: {}\n", plan.cause));
        if !plan.actions.is_empty() {
            out.push_str(&format!("  CALLERS: {}\n", plan.actions.len()));
        }
        for action in &plan.actions {
            out.push_str(&format!("  FIX {}:{}\n", action.file, action.line));
            if !action.old_text.is_empty() {
                out.push_str(&format!("    - {}\n", action.old_text));
            }
            if !action.new_text.is_empty() {
                out.push_str(&format!("    + {}\n", action.new_text));
            }
        }
    }

    out
}

/// Format the result of `fix --apply` for LLM output.
pub fn format_fix_apply(result: &FixApplyResult) -> String {
    let mut out = format!(
        "FIX-APPLY applied={} failed={} files={} recompile={}\n",
        result.actions_applied,
        result.actions_failed,
        result.files_modified.len(),
        if result.recompile_clean {
            "CLEAN"
        } else {
            "DIRTY"
        },
    );

    for d in &result.details {
        out.push_str(&format!(
            "  {} {}:{}",
            d.status.to_uppercase(),
            d.file,
            d.line
        ));
        if let Some(ref err) = d.error {
            out.push_str(&format!(" err={}", err));
        }
        out.push('\n');
    }

    if !result.recompile_clean {
        out.push_str(&format!(
            "RECOMPILE errors={} — run `keel compile` for details\n",
            result.recompile_errors,
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_enforce::types::*;

    #[test]
    fn test_empty_fix() {
        let result = FixResult {
            version: env!("CARGO_PKG_VERSION").into(),
            command: "fix".into(),
            violations_addressed: 0,
            files_affected: 0,
            plans: vec![],
        };
        assert!(format_fix(&result).contains("0 violations"));
    }

    #[test]
    fn test_fix_with_plan() {
        let result = FixResult {
            version: env!("CARGO_PKG_VERSION").into(),
            command: "fix".into(),
            violations_addressed: 1,
            files_affected: 1,
            plans: vec![FixPlan {
                code: "E001".into(),
                hash: "abc123".into(),
                category: "broken_caller".into(),
                target_name: "validateToken".into(),
                cause: "Signature changed from (token) to (token, opts)".into(),
                actions: vec![FixAction {
                    file: "src/middleware.rs".into(),
                    line: 42,
                    old_text: "validateToken(req.token)".into(),
                    new_text: "validateToken(req.token, Options::default())".into(),
                    description: "Update call site".into(),
                }],
            }],
        };
        let out = format_fix(&result);
        assert!(out.contains("FIX 1 violations in 1 files"));
        assert!(out.contains("VIOLATION E001 hash=abc123"));
        assert!(out.contains("CAUSE: Signature changed"));
        assert!(out.contains("- validateToken(req.token)"));
        assert!(out.contains("+ validateToken(req.token, Options::default())"));
    }

    #[test]
    fn test_fix_apply_clean() {
        let result = FixApplyResult {
            version: env!("CARGO_PKG_VERSION").into(),
            command: "fix --apply".into(),
            actions_applied: 2,
            actions_failed: 0,
            files_modified: vec!["src/a.rs".into()],
            recompile_clean: true,
            recompile_errors: 0,
            details: vec![
                FixApplyDetail {
                    file: "src/a.rs".into(),
                    line: 10,
                    status: "applied".into(),
                    error: None,
                },
                FixApplyDetail {
                    file: "src/a.rs".into(),
                    line: 20,
                    status: "applied".into(),
                    error: None,
                },
            ],
        };
        let out = format_fix_apply(&result);
        assert!(out.contains("FIX-APPLY applied=2 failed=0 files=1 recompile=CLEAN"));
        assert!(out.contains("APPLIED src/a.rs:10"));
    }

    #[test]
    fn test_fix_apply_with_failure() {
        let result = FixApplyResult {
            version: env!("CARGO_PKG_VERSION").into(),
            command: "fix --apply".into(),
            actions_applied: 1,
            actions_failed: 1,
            files_modified: vec!["src/a.rs".into()],
            recompile_clean: false,
            recompile_errors: 2,
            details: vec![
                FixApplyDetail {
                    file: "src/a.rs".into(),
                    line: 10,
                    status: "applied".into(),
                    error: None,
                },
                FixApplyDetail {
                    file: "src/missing.rs".into(),
                    line: 5,
                    status: "failed".into(),
                    error: Some("file not found: src/missing.rs".into()),
                },
            ],
        };
        let out = format_fix_apply(&result);
        assert!(out.contains("recompile=DIRTY"));
        assert!(out.contains("FAILED src/missing.rs:5 err=file not found"));
        assert!(out.contains("RECOMPILE errors=2"));
    }
}
