use std::collections::HashSet;

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};
use keel_parsers::resolver::FileIndex;

use crate::types::{AffectedNode, Violation};
use crate::violations_util::{is_stub_file, is_test_file, normalize_signature};

// Re-export E004, E005, W001, W002 checkers so engine.rs keeps using violations::*
pub use crate::violations_extended::{
    check_arity_mismatch, check_duplicate_names, check_placement, check_removed_functions,
    check_removed_functions_with_cache,
};

/// Check E001: broken_caller — caller references a function whose hash changed.
/// Compares current definitions against the graph store to detect hash changes,
/// then finds callers that reference the old hash.
///
/// If `cached_nodes` is provided, uses them instead of querying the store.
pub fn check_broken_callers(file: &FileIndex, store: &dyn GraphStore) -> Vec<Violation> {
    let nodes = store.get_nodes_in_file(&file.file_path);
    let empty: HashSet<&str> = HashSet::new();
    check_broken_callers_with_cache(file, store, &nodes, &empty)
}

/// E001 implementation using pre-fetched nodes to avoid redundant queries.
/// When `batch_files` is non-empty, callers whose source file is in the batch
/// are skipped — the user modified both the function and its caller, so the
/// caller was likely updated. Callers NOT in the batch still fire normally.
pub fn check_broken_callers_with_cache(
    file: &FileIndex,
    store: &dyn GraphStore,
    existing_nodes: &[keel_core::types::GraphNode],
    batch_files: &HashSet<&str>,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    for def in &file.definitions {
        // Find existing node by name in same file
        let existing = existing_nodes.iter().find(|n| n.name == def.name);

        let Some(existing) = existing else { continue };

        // Body/docstring-only changes cannot break callers. Compare signatures
        // whitespace-normalized (a pure reformat — rustfmt/prettier line wrap —
        // must not read as a signature change) FIRST, so this common case skips
        // the definition_hashes normalize+hash below entirely. When signatures
        // match, E001 never fires regardless of the hash, so the hashing was
        // pure waste here.
        if normalize_signature(&existing.signature) == normalize_signature(&def.signature) {
            continue;
        }

        // Signature changed. Confirm it is a real content change (not a hash
        // collision) before treating callers as broken. `definition_hashes` is
        // the single source of truth for the plain + disambiguated pair (and
        // applies issue-#36 body normalization).
        let (new_hash, new_hash_disambiguated) =
            crate::violations_util::definition_hashes(def, &file.file_path);
        if existing.hash == new_hash || existing.hash == new_hash_disambiguated {
            continue; // No change
        }

        // Signature changed — find all callers with their edge confidence
        let incoming = store.get_edges(existing.id, EdgeDirection::Incoming);
        let caller_edges: Vec<_> = incoming
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .collect();

        // Split into high-confidence (direct calls) and low-confidence (dynamic dispatch).
        // Skip callers whose source file is in the compile batch — the user modified
        // both files, so the caller was likely updated to match the new signature.
        let confident_callers: Vec<_> = caller_edges
            .iter()
            .filter(|e| e.confidence >= keel_core::confidence::ERROR_TIER_THRESHOLD)
            .filter_map(|e| store.get_node_by_id(e.source_id))
            .filter(|c| !batch_files.contains(c.file_path.as_str()))
            .collect();
        let uncertain_callers: Vec<_> = caller_edges
            .iter()
            .filter(|e| e.confidence < keel_core::confidence::ERROR_TIER_THRESHOLD)
            .filter_map(|e| store.get_node_by_id(e.source_id))
            .filter(|c| !batch_files.contains(c.file_path.as_str()))
            .collect();

        if confident_callers.is_empty() && uncertain_callers.is_empty() {
            continue;
        }

        // High-confidence callers: ERROR (structural breakage is certain)
        if !confident_callers.is_empty() {
            let affected: Vec<AffectedNode> = confident_callers
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
                    confident_callers.len()
                ),
                file: file.file_path.clone(),
                line: def.line_start,
                hash: existing.hash.clone(),
                confidence: 0.92,
                resolution_tier: "tree-sitter".to_string(),
                fix_hint: Some(format!(
                    "Update callers of `{}` to match new signature: {}",
                    def.name, def.signature
                )),
                suppressed: false,
                suppress_hint: None,
                affected,
                suggested_module: None,
                existing: None,
            });
        }

        // Low-confidence callers (dynamic dispatch): WARNING, not ERROR
        if !uncertain_callers.is_empty() {
            let affected: Vec<AffectedNode> = uncertain_callers
                .iter()
                .map(|c| AffectedNode {
                    hash: c.hash.clone(),
                    name: c.name.clone(),
                    file: c.file_path.clone(),
                    line: c.line_start,
                })
                .collect();
            let min_confidence = caller_edges
                .iter()
                .filter(|e| e.confidence < keel_core::confidence::ERROR_TIER_THRESHOLD)
                .map(|e| e.confidence)
                .fold(1.0f64, f64::min);
            violations.push(Violation {
                code: "E001".to_string(),
                severity: "WARNING".to_string(),
                category: "broken_caller".to_string(),
                message: format!(
                    "Signature of `{}` changed; {} dynamic dispatch caller(s) may need updating",
                    def.name,
                    uncertain_callers.len()
                ),
                file: file.file_path.clone(),
                line: def.line_start,
                hash: existing.hash.clone(),
                confidence: min_confidence,
                resolution_tier: "tree-sitter".to_string(),
                fix_hint: Some(format!(
                    "Check if dynamic dispatch callers of `{}` need updating for new signature: {}",
                    def.name, def.signature
                )),
                suppressed: false,
                suppress_hint: None,
                affected,
                suggested_module: None,
                existing: None,
            });
        }
    }

    violations
}

/// Check E002: missing_type_hints — function parameters/return lack type annotations.
/// Only for Python (and JS with JSDoc). TS/Go/Rust are typed by nature.
/// Skips test files — test helpers rarely need full type annotations.
pub fn check_missing_type_hints(file: &FileIndex) -> Vec<Violation> {
    let mut violations = Vec::new();

    // Skip test files (helpers rarely need full annotations) and stub files
    // (signature-only by definition).
    if is_test_file(&file.file_path) || is_stub_file(&file.file_path) {
        return violations;
    }

    for def in &file.definitions {
        if def.kind != NodeKind::Function {
            continue;
        }
        // Test-context symbols (#[cfg(test)] modules, #[test] functions) are
        // harness plumbing — exempt like test files are.
        if def.in_test_context {
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
            message: format!("Public function `{}` lacks type annotations", def.name),
            file: file.file_path.clone(),
            line: def.line_start,
            hash: keel_core::hash::compute_hash(
                &def.signature,
                &def.body_for_hash(),
                def.docstring.as_deref().unwrap_or(""),
            ),
            confidence: 1.0,
            resolution_tier: "tree-sitter".to_string(),
            fix_hint: Some({
                let is_js = file.file_path.ends_with(".js")
                    || file.file_path.ends_with(".jsx")
                    || file.file_path.ends_with(".mjs")
                    || file.file_path.ends_with(".cjs");
                if is_js {
                    format!("Add JSDoc @param/@returns annotations to `{}`", def.name)
                } else {
                    format!(
                        "Add type annotations to all parameters and return type of `{}`",
                        def.name
                    )
                }
            }),
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
/// Skips test files — test functions are self-documenting by name.
pub fn check_missing_docstring(file: &FileIndex) -> Vec<Violation> {
    let mut violations = Vec::new();

    // Skip test files (self-documenting by name) and stub files (there is
    // no body to document — E003 on a .pyi stub is pure noise).
    if is_test_file(&file.file_path) || is_stub_file(&file.file_path) {
        return violations;
    }

    for def in &file.definitions {
        if def.kind != NodeKind::Function {
            continue;
        }
        // Test-context symbols (#[cfg(test)] modules, #[test] functions) are
        // self-documenting by name — exempt like test files are.
        if def.in_test_context {
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
            hash: keel_core::hash::compute_hash(
                &def.signature,
                &def.body_for_hash(),
                def.docstring.as_deref().unwrap_or(""),
            ),
            confidence: 1.0,
            resolution_tier: "tree-sitter".to_string(),
            fix_hint: Some(format!("Add a documentation comment to `{}`", def.name)),
            suppressed: false,
            suppress_hint: None,
            affected: vec![],
            suggested_module: None,
            existing: None,
        });
    }

    violations
}
