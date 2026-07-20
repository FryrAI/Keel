//! Shared fixtures for enforcement test modules.
//!
//! `ECON_BODY` is load-bearing: it must clear the W006 duplicate-detection
//! length threshold (`MIN_DUPLICATE_BODY_LEN`), so W005/W006 tests in
//! different files share this single definition instead of drifting copies.

use keel_core::types::{GraphNode, NodeKind};
use keel_parsers::resolver::{Definition, FileIndex, Reference, ReferenceKind};

/// A body long enough to clear the W006 duplicate threshold.
pub(crate) const ECON_BODY: &str =
    "let total = items.iter().map(|i| i.price * i.qty).sum::<u64>();\n\
                                    apply_discount(total, customer.tier)";

/// A function definition at lines 10-20 with [`ECON_BODY`].
pub(crate) fn definition(name: &str, file: &str, is_public: bool) -> Definition {
    Definition {
        name: name.to_string(),
        kind: NodeKind::Function,
        signature: format!("fn {name}()"),
        file_path: file.to_string(),
        line_start: 10,
        line_end: 20,
        docstring: None,
        is_public,
        type_hints_present: true,
        body_text: ECON_BODY.to_string(),
        in_test_context: false,
        in_trait_context: false,
        is_associated: false,
        is_auto_invoked: false,
    }
}

/// A private function definition flagged as living in a test context (a
/// `#[cfg(test)]` module or `#[test]` function).
pub(crate) fn test_context_definition(name: &str, file: &str) -> Definition {
    Definition {
        in_test_context: true,
        ..definition(name, file, false)
    }
}

/// A private function definition flagged as auto-invoked — what a language's
/// Tier-2 pass sets for a runtime/harness entrypoint (Go `init`/`main`/
/// `TestMain`). Exempt from W005 without any language-from-path re-derivation.
pub(crate) fn auto_invoked_definition(name: &str, file: &str) -> Definition {
    Definition {
        is_auto_invoked: true,
        ..definition(name, file, false)
    }
}

/// A `FileIndex` carrying only definitions.
pub(crate) fn file_index(file: &str, definitions: Vec<Definition>) -> FileIndex {
    file_index_with_refs(file, definitions, vec![])
}

/// A `FileIndex` with definitions and references.
pub(crate) fn file_index_with_refs(
    file: &str,
    definitions: Vec<Definition>,
    references: Vec<Reference>,
) -> FileIndex {
    FileIndex {
        file_path: file.to_string(),
        content_hash: 0,
        definitions,
        references,
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }
}

/// A call reference at line 5.
pub(crate) fn call_ref(name: &str, file: &str) -> Reference {
    Reference {
        name: name.to_string(),
        file_path: file.to_string(),
        line: 5,
        kind: ReferenceKind::Call,
        resolved_to: None,
    }
}

/// A stored private-function node at lines 10-20.
pub(crate) fn function_node(id: u64, hash: &str, name: &str, file: &str) -> GraphNode {
    GraphNode {
        id,
        hash: hash.to_string(),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: format!("fn {name}()"),
        file_path: file.to_string(),
        line_start: 10,
        line_end: 20,
        docstring: None,
        is_public: false,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

/// A stored node whose hash matches `def` under `definition_hashes` — i.e.
/// the graph's record of this exact, untouched definition.
pub(crate) fn node_for_definition(id: u64, def: &Definition) -> GraphNode {
    let (hash, _) = crate::violations_util::definition_hashes(def, &def.file_path);
    let mut node = function_node(id, &hash, &def.name, &def.file_path);
    node.is_public = def.is_public;
    node.line_start = def.line_start;
    node.line_end = def.line_end;
    node
}
