use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};
use keel_parsers::resolver::FileIndex;

use crate::types::{AffectedNode, Violation};
use crate::violations_util::{count_call_args, count_params, extract_prefix, is_test_file};

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

        // Only compare when both sides were parseable (count_params/count_call_args
        // return 0 both for genuinely zero params AND for unparseable signatures).
        // We flag mismatches when at least one side has params, which means the
        // zero on the other side is a real zero (not a parse failure).
        if (expected_arity > 0 || call_arity > 0) && expected_arity != call_arity {
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
/// Uses indexed SQL query instead of scanning all modules. O(F) not O(F*M).
pub fn check_placement(
    file: &FileIndex,
    store: &dyn GraphStore,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    for def in &file.definitions {
        if def.kind != NodeKind::Function {
            continue;
        }

        let fn_prefix = extract_prefix(&def.name);
        if fn_prefix.is_empty() {
            continue;
        }

        // Single SQL query per function — finds modules with matching prefix
        let matching = store.find_modules_by_prefix(&fn_prefix, &file.file_path);
        if let Some(profile) = matching.first() {
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
        }
    }

    violations
}

/// Check W002: duplicate_name — same function name in multiple modules.
/// Uses indexed SQL query instead of triple-nested loop. O(F) not O(F*M*N).
pub fn check_duplicate_names(
    file: &FileIndex,
    store: &dyn GraphStore,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    // Skip W002 entirely if the current file is a test file
    if is_test_file(&file.file_path) {
        return violations;
    }

    for def in &file.definitions {
        if def.kind != NodeKind::Function {
            continue;
        }

        // Single SQL query per function — finds same-named functions elsewhere
        let duplicates = store.find_nodes_by_name(&def.name, "function", &file.file_path);
        for node in &duplicates {
            // Skip test files in results
            if is_test_file(&node.file_path) {
                continue;
            }
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

    violations
}

