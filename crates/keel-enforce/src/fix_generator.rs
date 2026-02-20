use crate::types::{FixAction, FixPlan, Violation};
use keel_core::store::GraphStore;
use keel_core::types::EdgeDirection;
use std::path::Path;

/// Generate fix plans from a set of violations.
///
/// Currently supports plan-only mode (no --apply).
/// Priority: E001, E004, E005 (caller-propagation), E002/E003 (template stubs).
pub fn generate_fix_plans(violations: &[&Violation], store: &dyn GraphStore) -> Vec<FixPlan> {
    let mut plans = Vec::new();
    for v in violations {
        if let Some(plan) = generate_plan_for_violation(v, store) {
            plans.push(plan);
        }
    }
    plans
}

fn generate_plan_for_violation(v: &Violation, store: &dyn GraphStore) -> Option<FixPlan> {
    match v.code.as_str() {
        "E001" => generate_broken_caller_fix(v, store),
        "E004" => generate_removed_function_fix(v, store),
        "E005" => generate_arity_mismatch_fix(v, store),
        "E002" => generate_type_hint_fix(v),
        "E003" => generate_docstring_fix(v),
        _ => None,
    }
}

/// E001: broken_caller — signature changed, callers need updating.
fn generate_broken_caller_fix(v: &Violation, store: &dyn GraphStore) -> Option<FixPlan> {
    let node = store.get_node(&v.hash)?;
    let callers = store.get_edges(node.id, EdgeDirection::Incoming);

    let mut actions = Vec::new();
    for edge in &callers {
        if let Some(caller_node) = store.get_node_by_id(edge.source_id) {
            actions.push(FixAction {
                file: caller_node.file_path.clone(),
                line: edge.line,
                old_text: format!("{}(...) // old signature", node.name),
                new_text: format!("{}(...) // update to: {}", node.name, node.signature),
                description: format!("Update call to `{}` in `{}`", node.name, caller_node.name),
            });
        }
    }

    Some(FixPlan {
        code: v.code.clone(),
        hash: v.hash.clone(),
        category: v.category.clone(),
        target_name: node.name.clone(),
        cause: format!(
            "Signature changed to `{}`; {} caller(s) need updating",
            node.signature,
            callers.len(),
        ),
        actions,
    })
}

/// E004: function_removed — function no longer exists, callers need updating.
fn generate_removed_function_fix(v: &Violation, _store: &dyn GraphStore) -> Option<FixPlan> {
    // For removed functions, we use the affected nodes from the violation
    let actions: Vec<FixAction> = v
        .affected
        .iter()
        .map(|a| FixAction {
            file: a.file.clone(),
            line: a.line,
            old_text: format!("call to removed function (hash={})", v.hash),
            new_text: "// TODO: replace with alternative or restore function".to_string(),
            description: format!("Caller `{}` references removed function", a.name),
        })
        .collect();

    Some(FixPlan {
        code: v.code.clone(),
        hash: v.hash.clone(),
        category: v.category.clone(),
        target_name: v.message.clone(),
        cause: format!(
            "Function was removed; {} caller(s) still reference it",
            v.affected.len(),
        ),
        actions,
    })
}

/// E005: arity_mismatch — parameter count changed, callers need updating.
fn generate_arity_mismatch_fix(v: &Violation, store: &dyn GraphStore) -> Option<FixPlan> {
    let node = store.get_node(&v.hash)?;
    let callers = store.get_edges(node.id, EdgeDirection::Incoming);

    let mut actions = Vec::new();
    for edge in &callers {
        if let Some(caller_node) = store.get_node_by_id(edge.source_id) {
            actions.push(FixAction {
                file: caller_node.file_path.clone(),
                line: edge.line,
                old_text: format!("{}(...) // wrong arity", node.name),
                new_text: format!("{}(...) // match new sig: {}", node.name, node.signature),
                description: format!(
                    "Update arity of call to `{}` in `{}`",
                    node.name, caller_node.name,
                ),
            });
        }
    }

    Some(FixPlan {
        code: v.code.clone(),
        hash: v.hash.clone(),
        category: v.category.clone(),
        target_name: node.name.clone(),
        cause: format!(
            "Parameter count changed in `{}`; signature is now `{}`",
            node.name, node.signature,
        ),
        actions,
    })
}

/// Validate a fix plan: check that target files exist and lines are in range.
/// Returns a list of (action_index, error_message) for invalid actions.
pub fn validate_fix_plan(plan: &FixPlan, base_dir: &Path) -> Vec<(usize, String)> {
    let mut errors = Vec::new();
    for (i, action) in plan.actions.iter().enumerate() {
        let path = base_dir.join(&action.file);
        if !path.exists() {
            errors.push((i, format!("file not found: {}", action.file)));
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let line_count = content.lines().count() as u32;
                if action.line > line_count {
                    errors.push((
                        i,
                        format!(
                            "line {} exceeds file length ({} lines)",
                            action.line, line_count
                        ),
                    ));
                }
            }
            Err(e) => errors.push((i, format!("cannot read {}: {}", action.file, e))),
        }
    }
    errors
}

/// E002: missing_type_hints — generate stub type annotations.
fn generate_type_hint_fix(v: &Violation) -> Option<FixPlan> {
    Some(FixPlan {
        code: v.code.clone(),
        hash: v.hash.clone(),
        category: v.category.clone(),
        target_name: v.message.clone(),
        cause: "Function is missing type annotations".to_string(),
        actions: vec![FixAction {
            file: v.file.clone(),
            line: v.line,
            old_text: String::new(),
            new_text: "// TODO: Add type annotations to parameters and return type".to_string(),
            description: "Add type hints".to_string(),
        }],
    })
}

/// E003: missing_docstring — generate docstring template.
fn generate_docstring_fix(v: &Violation) -> Option<FixPlan> {
    Some(FixPlan {
        code: v.code.clone(),
        hash: v.hash.clone(),
        category: v.category.clone(),
        target_name: v.message.clone(),
        cause: "Function is missing a docstring".to_string(),
        actions: vec![FixAction {
            file: v.file.clone(),
            line: v.line,
            old_text: String::new(),
            new_text: "/// TODO: Add documentation describing this function's purpose".to_string(),
            description: "Add docstring".to_string(),
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AffectedNode, Violation};

    fn make_violation(code: &str, hash: &str) -> Violation {
        Violation {
            code: code.into(),
            severity: "ERROR".into(),
            category: format!("test_{}", code),
            message: format!("Test violation {}", code),
            file: "src/test.rs".into(),
            line: 10,
            hash: hash.into(),
            confidence: 0.9,
            resolution_tier: "tree-sitter".into(),
            fix_hint: Some("Fix it".into()),
            suppressed: false,
            suppress_hint: None,
            affected: vec![],
            suggested_module: None,
            existing: None,
        }
    }

    #[test]
    fn test_e002_generates_type_hint_stub() {
        let v = make_violation("E002", "h1");
        let plan = generate_type_hint_fix(&v).unwrap();
        assert_eq!(plan.code, "E002");
        assert_eq!(plan.actions.len(), 1);
        assert!(plan.actions[0].new_text.contains("type annotations"));
    }

    #[test]
    fn test_e003_generates_docstring_stub() {
        let v = make_violation("E003", "h1");
        let plan = generate_docstring_fix(&v).unwrap();
        assert_eq!(plan.code, "E003");
        assert_eq!(plan.actions.len(), 1);
        assert!(plan.actions[0].new_text.contains("documentation"));
    }

    #[test]
    fn test_e004_uses_affected_nodes() {
        let mut v = make_violation("E004", "h1");
        v.affected = vec![
            AffectedNode {
                hash: "a1".into(),
                name: "caller1".into(),
                file: "src/a.rs".into(),
                line: 20,
            },
            AffectedNode {
                hash: "a2".into(),
                name: "caller2".into(),
                file: "src/b.rs".into(),
                line: 30,
            },
        ];
        let plan = generate_removed_function_fix(
            &v,
            &keel_core::sqlite::SqliteGraphStore::in_memory().unwrap(),
        );
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.actions.len(), 2);
        assert!(plan.cause.contains("2 caller(s)"));
    }

    #[test]
    fn test_validate_fix_plan_missing_file() {
        let plan = FixPlan {
            code: "E001".into(),
            hash: "h1".into(),
            category: "test".into(),
            target_name: "foo".into(),
            cause: "test".into(),
            actions: vec![FixAction {
                file: "nonexistent_file.rs".into(),
                line: 1,
                old_text: String::new(),
                new_text: "// fix".into(),
                description: "test".into(),
            }],
        };
        let errors = validate_fix_plan(&plan, Path::new("/tmp"));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].1.contains("file not found"));
    }

    #[test]
    fn test_unsupported_code_returns_none() {
        let v = make_violation("W001", "h1");
        let store = keel_core::sqlite::SqliteGraphStore::in_memory().unwrap();
        let plan = generate_plan_for_violation(&v, &store);
        assert!(plan.is_none());
    }
}
