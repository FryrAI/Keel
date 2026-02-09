use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};
use keel_parsers::resolver::FileIndex;

use crate::types::{AffectedNode, Violation};

/// Check E001: broken_caller — caller references a function whose hash changed.
/// Compares current definitions against the graph store to detect hash changes,
/// then finds callers that reference the old hash.
pub fn check_broken_callers(
    file: &FileIndex,
    store: &dyn GraphStore,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    for def in &file.definitions {
        // Find existing node by name in same file
        let existing_nodes = store.get_nodes_in_file(&file.file_path);
        let existing = existing_nodes.iter().find(|n| n.name == def.name);

        let Some(existing) = existing else { continue };

        // Compute expected hash from current definition
        let new_hash = keel_core::hash::compute_hash(&def.signature, &def.body_text, def.docstring.as_deref().unwrap_or(""));

        if existing.hash == new_hash {
            continue; // No change
        }

        // Hash changed — find all callers
        let incoming = store.get_edges(existing.id, EdgeDirection::Incoming);
        let callers: Vec<_> = incoming
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .filter_map(|e| store.get_node_by_id(e.source_id))
            .collect();

        if callers.is_empty() {
            continue;
        }

        let affected: Vec<AffectedNode> = callers
            .iter()
            .map(|c| AffectedNode {
                hash: c.hash.clone(),
                name: c.name.clone(),
                file: c.file_path.clone(),
                line: c.line_start,
            })
            .collect();

        violations.push(Violation {
            code: "E001".to_string(),
            severity: "ERROR".to_string(),
            category: "broken_caller".to_string(),
            message: format!(
                "Signature of `{}` changed; {} caller(s) need updating",
                def.name,
                callers.len()
            ),
            file: file.file_path.clone(),
            line: def.line_start,
            hash: existing.hash.clone(),
            confidence: 0.92,
            resolution_tier: "tree-sitter".to_string(),
            fix_hint: Some(format!(
                "Update callers of `{}` to match new signature",
                def.name
            )),
            suppressed: false,
            suppress_hint: None,
            affected,
            suggested_module: None,
            existing: None,
        });
    }

    violations
}

/// Check E002: missing_type_hints — function parameters/return lack type annotations.
/// Only for Python (and JS with JSDoc). TS/Go/Rust are typed by nature.
pub fn check_missing_type_hints(file: &FileIndex) -> Vec<Violation> {
    let mut violations = Vec::new();

    for def in &file.definitions {
        if def.kind != NodeKind::Function {
            continue;
        }
        if def.type_hints_present {
            continue;
        }
        if !def.is_public {
            continue; // Only enforce on public API
        }

        violations.push(Violation {
            code: "E002".to_string(),
            severity: "ERROR".to_string(),
            category: "missing_type_hints".to_string(),
            message: format!(
                "Public function `{}` lacks type annotations",
                def.name
            ),
            file: file.file_path.clone(),
            line: def.line_start,
            hash: String::new(), // Computed after graph update
            confidence: 1.0,
            resolution_tier: "tree-sitter".to_string(),
            fix_hint: Some(format!(
                "Add type annotations to all parameters and return type of `{}`",
                def.name
            )),
            suppressed: false,
            suppress_hint: None,
            affected: vec![],
            suggested_module: None,
            existing: None,
        });
    }

    violations
}

/// Check E003: missing_docstring — public function has no docstring.
pub fn check_missing_docstring(file: &FileIndex) -> Vec<Violation> {
    let mut violations = Vec::new();

    for def in &file.definitions {
        if def.kind != NodeKind::Function {
            continue;
        }
        if def.docstring.is_some() {
            continue;
        }
        if !def.is_public {
            continue;
        }

        violations.push(Violation {
            code: "E003".to_string(),
            severity: "ERROR".to_string(),
            category: "missing_docstring".to_string(),
            message: format!("Public function `{}` has no docstring", def.name),
            file: file.file_path.clone(),
            line: def.line_start,
            hash: String::new(),
            confidence: 1.0,
            resolution_tier: "tree-sitter".to_string(),
            fix_hint: Some(format!(
                "Add a documentation comment to `{}`",
                def.name
            )),
            suppressed: false,
            suppress_hint: None,
            affected: vec![],
            suggested_module: None,
            existing: None,
        });
    }

    violations
}

/// Check E004: function_removed — a function was removed but callers still exist.
/// Compares existing nodes in the store against current file definitions.
pub fn check_removed_functions(
    file: &FileIndex,
    store: &dyn GraphStore,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    let stored_nodes = store.get_nodes_in_file(&file.file_path);
    let current_names: std::collections::HashSet<&str> =
        file.definitions.iter().map(|d| d.name.as_str()).collect();

    for node in &stored_nodes {
        if node.kind != NodeKind::Function {
            continue;
        }
        if current_names.contains(node.name.as_str()) {
            continue;
        }

        // Function was removed — check for callers
        let incoming = store.get_edges(node.id, EdgeDirection::Incoming);
        let callers: Vec<_> = incoming
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .filter_map(|e| store.get_node_by_id(e.source_id))
            .collect();

        if callers.is_empty() {
            continue; // No callers, safe to remove
        }

        let affected: Vec<AffectedNode> = callers
            .iter()
            .map(|c| AffectedNode {
                hash: c.hash.clone(),
                name: c.name.clone(),
                file: c.file_path.clone(),
                line: c.line_start,
            })
            .collect();

        violations.push(Violation {
            code: "E004".to_string(),
            severity: "ERROR".to_string(),
            category: "function_removed".to_string(),
            message: format!(
                "Function `{}` was removed but has {} caller(s)",
                node.name,
                callers.len()
            ),
            file: file.file_path.clone(),
            line: node.line_start,
            hash: node.hash.clone(),
            confidence: 0.95,
            resolution_tier: "tree-sitter".to_string(),
            fix_hint: Some(format!(
                "Remove or update callers of `{}` before deleting it",
                node.name
            )),
            suppressed: false,
            suppress_hint: None,
            affected,
            suggested_module: None,
            existing: None,
        });
    }

    violations
}

/// Check E005: arity_mismatch — caller passes wrong number of arguments.
/// Compares reference argument counts against definition parameter counts.
pub fn check_arity_mismatch(
    file: &FileIndex,
    store: &dyn GraphStore,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    for reference in &file.references {
        if reference.kind != keel_parsers::resolver::ReferenceKind::Call {
            continue;
        }
        let Some(target_hash) = &reference.resolved_to else { continue };

        let Some(target_node) = store.get_node(target_hash) else { continue };

        // Count params from signature (rough heuristic: count commas + 1)
        let expected_arity = count_params(&target_node.signature);
        let call_arity = count_call_args(&reference.name);

        if expected_arity > 0 && call_arity > 0 && expected_arity != call_arity {
            violations.push(Violation {
                code: "E005".to_string(),
                severity: "ERROR".to_string(),
                category: "arity_mismatch".to_string(),
                message: format!(
                    "Call to `{}` passes {} arg(s) but function expects {}",
                    target_node.name, call_arity, expected_arity
                ),
                file: file.file_path.clone(),
                line: reference.line,
                hash: target_node.hash.clone(),
                confidence: 0.85,
                resolution_tier: "tree-sitter".to_string(),
                fix_hint: Some(format!(
                    "Update call to `{}` to pass {} argument(s)",
                    target_node.name, expected_arity
                )),
                suppressed: false,
                suppress_hint: None,
                affected: vec![],
                suggested_module: None,
                existing: None,
            });
        }
    }

    violations
}

/// Check W001: placement — function may be in wrong module.
/// Compares function name prefixes against module responsibility keywords.
pub fn check_placement(
    file: &FileIndex,
    store: &dyn GraphStore,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    let all_modules = store.get_all_modules();

    for def in &file.definitions {
        if def.kind != NodeKind::Function {
            continue;
        }

        // Check if this function's name prefix matches any other module better
        let fn_prefix = extract_prefix(&def.name);
        if fn_prefix.is_empty() {
            continue;
        }

        for module in &all_modules {
            if module.file_path == file.file_path {
                continue; // Same module, skip
            }
            let Some(profile) = store.get_module_profile(module.id) else {
                continue;
            };

            let matches_other = profile
                .function_name_prefixes
                .iter()
                .any(|p| p == &fn_prefix);
            if !matches_other {
                continue;
            }

            violations.push(Violation {
                code: "W001".to_string(),
                severity: "WARNING".to_string(),
                category: "placement".to_string(),
                message: format!(
                    "Function `{}` may belong in module `{}`",
                    def.name, profile.path
                ),
                file: file.file_path.clone(),
                line: def.line_start,
                hash: String::new(),
                confidence: 0.6,
                resolution_tier: "heuristic".to_string(),
                fix_hint: Some(format!(
                    "Consider moving `{}` to `{}`",
                    def.name, profile.path
                )),
                suppressed: false,
                suppress_hint: None,
                affected: vec![],
                suggested_module: Some(profile.path.clone()),
                existing: None,
            });
            break; // One suggestion per function
        }
    }

    violations
}

/// Check W002: duplicate_name — same function name in multiple modules.
pub fn check_duplicate_names(
    file: &FileIndex,
    store: &dyn GraphStore,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    for def in &file.definitions {
        if def.kind != NodeKind::Function {
            continue;
        }

        let all_modules = store.get_all_modules();
        for module in &all_modules {
            let nodes = store.get_nodes_in_file(&module.file_path);
            for node in &nodes {
                if node.name == def.name
                    && node.file_path != file.file_path
                    && node.kind == NodeKind::Function
                {
                    violations.push(Violation {
                        code: "W002".to_string(),
                        severity: "WARNING".to_string(),
                        category: "duplicate_name".to_string(),
                        message: format!(
                            "Function `{}` also exists in `{}`",
                            def.name, node.file_path
                        ),
                        file: file.file_path.clone(),
                        line: def.line_start,
                        hash: String::new(),
                        confidence: 0.7,
                        resolution_tier: "heuristic".to_string(),
                        fix_hint: Some(format!(
                            "Rename one of the `{}` functions to avoid ambiguity",
                            def.name
                        )),
                        suppressed: false,
                        suppress_hint: None,
                        affected: vec![],
                        suggested_module: None,
                        existing: Some(crate::types::ExistingNode {
                            hash: node.hash.clone(),
                            file: node.file_path.clone(),
                            line: node.line_start,
                        }),
                    });
                    break; // One per definition
                }
            }
        }
    }

    violations
}

/// Count parameters from a signature string. Returns 0 if unable to parse.
fn count_params(sig: &str) -> usize {
    let Some(start) = sig.find('(') else { return 0 };
    let Some(end) = sig.find(')') else { return 0 };
    let params = &sig[start + 1..end].trim();
    if params.is_empty() {
        return 0;
    }
    params.split(',').count()
}

/// Count args in a call expression. Rough heuristic — returns 0 if cannot parse.
fn count_call_args(name: &str) -> usize {
    // In practice, the parser provides arg count. This is a fallback.
    let Some(start) = name.find('(') else { return 0 };
    let Some(end) = name.rfind(')') else { return 0 };
    let args = &name[start + 1..end].trim();
    if args.is_empty() {
        return 0;
    }
    args.split(',').count()
}

/// Extract a name prefix (e.g., "handle" from "handleRequest").
fn extract_prefix(name: &str) -> String {
    // Split on camelCase or snake_case boundary
    if let Some(pos) = name.find('_') {
        return name[..pos].to_string();
    }
    // camelCase: find first lowercase->uppercase transition
    let chars: Vec<char> = name.chars().collect();
    for i in 1..chars.len() {
        if chars[i].is_uppercase() {
            return chars[..i].iter().collect();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_params() {
        assert_eq!(count_params("fn foo()"), 0);
        assert_eq!(count_params("fn foo(a: i32)"), 1);
        assert_eq!(count_params("fn foo(a: i32, b: str)"), 2);
        assert_eq!(count_params("def bar(x, y, z)"), 3);
    }

    // E005 edge cases: zero params, many params, edge patterns
    #[test]
    fn test_count_params_zero() {
        assert_eq!(count_params("fn foo()"), 0);
        assert_eq!(count_params("def bar()"), 0);
        assert_eq!(count_params("func Baz()"), 0);
    }

    #[test]
    fn test_count_params_no_parens() {
        assert_eq!(count_params("fn foo"), 0);
        assert_eq!(count_params(""), 0);
    }

    #[test]
    fn test_count_params_many() {
        assert_eq!(count_params("fn f(a: i32, b: i32, c: i32, d: i32)"), 4);
        assert_eq!(count_params("def g(a, b, c, d, e)"), 5);
    }

    #[test]
    fn test_count_params_self_receiver() {
        // Rust method with self
        assert_eq!(count_params("fn method(&self, x: i32)"), 2);
    }

    #[test]
    fn test_count_call_args_empty() {
        assert_eq!(count_call_args("foo()"), 0);
    }

    #[test]
    fn test_count_call_args_no_parens() {
        assert_eq!(count_call_args("foo"), 0);
    }

    #[test]
    fn test_count_call_args_multiple() {
        assert_eq!(count_call_args("foo(a, b, c)"), 3);
    }

    #[test]
    fn test_extract_prefix() {
        assert_eq!(extract_prefix("handleRequest"), "handle");
        assert_eq!(extract_prefix("process_order"), "process");
        assert_eq!(extract_prefix("x"), "");
    }

    #[test]
    fn test_extract_prefix_all_lowercase() {
        assert_eq!(extract_prefix("process"), "");
    }

    #[test]
    fn test_extract_prefix_snake_case_multi() {
        assert_eq!(extract_prefix("get_user_name"), "get");
    }
}
