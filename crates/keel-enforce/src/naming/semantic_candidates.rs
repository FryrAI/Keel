//! Opt-in, deterministic semantic concept expansion for `keel name`.
//!
//! This is deliberately a candidate generator, not a detector. Its output is
//! never read by W010, P003, compile, review gates, or `validate-plan --strict`.

use std::collections::BTreeSet;

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, GraphNode, NodeKind, SYMBOL_DEP_KINDS};

use crate::file_class::FileClass;
use crate::semantic::identifier_words;
use crate::types::{ReuseCandidate, ReuseCandidateSource};

use super::reuse::matches_kind;

const MAX_SEMANTIC_CANDIDATES: usize = 5;
const MIN_SEMANTIC_SCORE: f64 = 0.60;

/// Rank symbols sharing explicit domain concepts with the requested intent.
pub(super) fn find_semantic_candidates(
    store: &dyn GraphStore,
    modules: &[GraphNode],
    description: &str,
    module_filter: Option<&str>,
    kind_filter: Option<&str>,
) -> Vec<ReuseCandidate> {
    let requested = semantic_concepts(description);
    if requested.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for module in modules {
        if module_filter.is_some_and(|filter| !module.file_path.contains(filter))
            || !FileClass::classify(&module.file_path).grades_size_and_naming()
        {
            continue;
        }
        let module_concepts = semantic_concepts(&module.file_path);
        let module_coverage = semantic_coverage(&requested, &module_concepts);
        for node in store.get_nodes_in_file(&module.file_path) {
            if node.kind == NodeKind::Module
                || node.in_test_context
                || !matches_kind(&node, kind_filter)
            {
                continue;
            }
            let symbol_concepts = semantic_concepts(&format!(
                "{} {} {}",
                node.name,
                node.signature,
                node.docstring.as_deref().unwrap_or("")
            ));
            let symbol_coverage = semantic_coverage(&requested, &symbol_concepts);
            if symbol_coverage == 0.0 {
                continue;
            }
            let score = semantic_round(symbol_coverage * 0.80 + module_coverage * 0.20);
            if score < MIN_SEMANTIC_SCORE {
                continue;
            }
            out.push(semantic_candidate(
                store,
                node,
                score,
                requested.intersection(&symbol_concepts).cloned().collect(),
            ));
        }
    }
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.callers.cmp(&a.callers))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out.truncate(MAX_SEMANTIC_CANDIDATES);
    out
}

fn semantic_candidate(
    store: &dyn GraphStore,
    node: GraphNode,
    score: f64,
    concepts: Vec<String>,
) -> ReuseCandidate {
    let edges = store.get_edges(node.id, EdgeDirection::Both);
    ReuseCandidate {
        name: node.name,
        hash: node.hash,
        source: ReuseCandidateSource::Semantic,
        signature: node.signature,
        file: node.file_path,
        line: node.line_start,
        score,
        callers: edges
            .iter()
            .filter(|edge| edge.target_id == node.id && SYMBOL_DEP_KINDS.contains(&edge.kind))
            .count() as u32,
        callees: edges
            .iter()
            .filter(|edge| edge.source_id == node.id && edge.kind == EdgeKind::Calls)
            .count() as u32,
        evidence: vec![format!(
            "semantic concept overlap: {} (candidate only; never warning/gate)",
            concepts.join(", ")
        )],
    }
}

fn semantic_concepts(text: &str) -> BTreeSet<String> {
    identifier_words(text, 1)
        .into_iter()
        .filter_map(|word| canonical_concept(&word).map(str::to_string))
        .collect()
}

fn canonical_concept(word: &str) -> Option<&'static str> {
    const GROUPS: &[(&str, &[&str])] = &[
        (
            "timestamp",
            &["time", "timestamp", "datetime", "epoch", "unix", "seconds"],
        ),
        (
            "transform",
            &[
                "parse",
                "convert",
                "decode",
                "encode",
                "serialize",
                "deserialize",
            ],
        ),
        (
            "identity",
            &[
                "auth",
                "authenticate",
                "authorization",
                "jwt",
                "token",
                "session",
            ],
        ),
        (
            "configuration",
            &[
                "config",
                "configuration",
                "settings",
                "options",
                "preferences",
            ],
        ),
        (
            "network",
            &[
                "http", "https", "request", "response", "fetch", "download", "upload",
            ],
        ),
        (
            "storage",
            &["database", "db", "sql", "persist", "store", "repository"],
        ),
        (
            "validation",
            &["validate", "validation", "verify", "check", "guard"],
        ),
        (
            "deduplication",
            &["dedupe", "dedup", "duplicate", "unique", "canonicalize"],
        ),
    ];
    GROUPS
        .iter()
        .find_map(|(concept, words)| words.contains(&word).then_some(*concept))
}

fn semantic_coverage(requested: &BTreeSet<String>, candidate: &BTreeSet<String>) -> f64 {
    if requested.is_empty() {
        return 0.0;
    }
    requested.intersection(candidate).count() as f64 / requested.len() as f64
}

fn semantic_round(score: f64) -> f64 {
    (score * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::semantic_concepts;

    #[test]
    fn unix_and_timestamp_share_a_concept() {
        assert_eq!(
            semantic_concepts("convert unix seconds"),
            semantic_concepts("parseTimestamp datetime")
        );
    }
}
