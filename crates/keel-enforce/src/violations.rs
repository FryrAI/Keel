use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};
use keel_parsers::resolver::FileIndex;

use crate::types::{AffectedNode, Violation};

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
    check_broken_callers_with_cache(file, store, &nodes)
}

/// E001 implementation using pre-fetched nodes to avoid redundant queries.
pub fn check_broken_callers_with_cache(
    file: &FileIndex,
    store: &dyn GraphStore,
    existing_nodes: &[keel_core::types::GraphNode],
) -> Vec<Violation> {
    let mut violations = Vec::new();

    for def in &file.definitions {
        // Find existing node by name in same file
        let existing = existing_nodes.iter().find(|n| n.name == def.name);

        let Some(existing) = existing else { continue };

        // Compute expected hash from current definition
        let new_hash = keel_core::hash::compute_hash(
            &def.signature,
            &def.body_text,
            def.docstring.as_deref().unwrap_or(""),
        );
        // Also check disambiguated hash (map may have used it for collisions)
        let new_hash_disambiguated = keel_core::hash::compute_hash_disambiguated(
            &def.signature,
            &def.body_text,
            def.docstring.as_deref().unwrap_or(""),
            &file.file_path,
        );

        if existing.hash == new_hash || existing.hash == new_hash_disambiguated {
            continue; // No change
        }

        // Body/docstring-only changes cannot break callers
        if existing.signature == def.signature {
            continue;
        }

        // Signature changed — find all callers with their edge confidence
        let incoming = store.get_edges(existing.id, EdgeDirection::Incoming);
        let caller_edges: Vec<_> = incoming
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .collect();

        // Split into high-confidence (direct calls) and low-confidence (dynamic dispatch)
        let confident_callers: Vec<_> = caller_edges
            .iter()
            .filter(|e| e.confidence >= 0.80)
            .filter_map(|e| store.get_node_by_id(e.source_id))
            .collect();
        let uncertain_callers: Vec<_> = caller_edges
            .iter()
            .filter(|e| e.confidence < 0.80)
            .filter_map(|e| store.get_node_by_id(e.source_id))
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
                .filter(|e| e.confidence < 0.80)
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
                    "Check if dynamic dispatch callers of `{}` need updating",
                    def.name
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
            message: format!("Public function `{}` lacks type annotations", def.name),
            file: file.file_path.clone(),
            line: def.line_start,
            hash: keel_core::hash::compute_hash(
                &def.signature,
                &def.body_text,
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
            hash: keel_core::hash::compute_hash(
                &def.signature,
                &def.body_text,
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
