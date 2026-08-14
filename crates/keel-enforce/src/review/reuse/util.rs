//! Shared scoring and exemption helpers for reuse advisories.

use std::collections::HashSet;

use keel_core::types::{GraphNode, NodeKind};
use keel_parsers::resolver::Definition;

use super::super::diff::DiffScan;
use super::{ReuseAdvisory, ReuseEvidenceKind};
use crate::file_class::FileClass;
use crate::semantic::identifier_words;
use crate::violations_util::parse_signature;

/// Construct a fully explained advisory for a candidate pair.
pub(super) fn advisory(
    kind: ReuseEvidenceKind,
    new: &Definition,
    existing: &GraphNode,
    confidence: f64,
    evidence: Vec<String>,
) -> ReuseAdvisory {
    ReuseAdvisory {
        code: "W010".to_string(),
        severity: "WARNING".to_string(),
        kind,
        new_symbol: new.name.clone(),
        new_signature: new.signature.clone(),
        new_file: new.file_path.clone(),
        new_line: new.line_start,
        existing_symbol: existing.name.clone(),
        existing_signature: existing.signature.clone(),
        existing_file: existing.file_path.clone(),
        existing_line: existing.line_start,
        confidence,
        evidence,
        fix_hint: format!(
            "Reuse `{}` when its behavior fits; otherwise document the intentional semantic difference",
            existing.name
        ),
    }
}

/// Return whether two signatures have the same arity and return shape.
pub(super) fn signature_compatible(new: &Definition, existing: &GraphNode) -> bool {
    match (
        parse_signature(&new.signature),
        parse_signature(&existing.signature),
    ) {
        (Some(left), Some(right)) => {
            left.arity == right.arity && left.has_return == right.has_return
        }
        _ => false,
    }
}

/// Describe the signature evidence included in an advisory.
pub(super) fn signature_evidence(new: &Definition) -> String {
    match parse_signature(&new.signature) {
        Some(signature) => format!(
            "compatible signature shape: arity={} return={}",
            signature.arity, signature.has_return
        ),
        None => "compatible signature shape".to_string(),
    }
}

/// Exclude parsed definitions where structural comparison is misleading.
pub(super) fn exempt_new(definition: &Definition, scan: &DiffScan) -> bool {
    definition.kind != NodeKind::Function
        || definition.in_test_context
        || definition.in_trait_context
        || definition.is_associated
        || definition.is_decorated
        || FileClass::classify(&definition.file_path) != FileClass::Source
        || scan
            .head_indices
            .iter()
            .find(|index| index.file_path == definition.file_path)
            .is_some_and(|index| !index.external_endpoints.is_empty())
}

/// Exclude stored definitions where structural comparison is misleading.
pub(super) fn exempt_existing(node: &GraphNode) -> bool {
    node.kind != NodeKind::Function
        || node.in_test_context
        || node.is_associated
        || !node.external_endpoints.is_empty()
        || FileClass::classify(&node.file_path) != FileClass::Source
}

/// Tokenize names, documentation, and paths into domain vocabulary.
pub(super) fn domain_words(text: &str) -> HashSet<String> {
    identifier_words(text, 3).into_iter().collect()
}

/// Compute set overlap, returning zero for empty evidence sets.
pub(super) fn jaccard(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(right).count();
    let union = left.union(right).count();
    intersection as f64 / union as f64
}

/// Keep role scores stable across output formats and platforms.
pub(super) fn role_round_score(score: f64) -> f64 {
    (score * 100.0).round() / 100.0
}
