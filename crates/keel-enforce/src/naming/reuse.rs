//! Reuse-first candidate generation for `keel name`.
//!
//! Candidates are deterministic hints, never equivalence claims. The scorer
//! combines symbol-owned text with module responsibility and requires the
//! former, so a well-named `utils` module does not nominate every child.

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, GraphNode, NodeKind, SYMBOL_DEP_KINDS};

use crate::file_class::FileClass;
use crate::semantic::identifier_words;
use crate::types::{ReuseCandidate, ReuseCandidateSource};

use super::{compute_keyword_score, compute_path_score};

const MAX_REUSE_CANDIDATES: usize = 5;
const MIN_REUSE_SCORE: f64 = 0.25;

/// Rank existing hand-written symbols that may already implement `desc_words`.
pub(super) fn find_reuse_candidates(
    store: &dyn GraphStore,
    modules: &[GraphNode],
    desc_words: &[String],
    module_filter: Option<&str>,
    kind_filter: Option<&str>,
) -> Vec<ReuseCandidate> {
    let mut candidates = Vec::new();
    for module in modules {
        if module_filter.is_some_and(|filter| !module.file_path.contains(filter))
            || !FileClass::classify(&module.file_path).grades_size_and_naming()
        {
            continue;
        }
        let profile = store.get_module_profile(module.id);
        let responsibility_words = profile
            .as_ref()
            .map(|p| p.responsibility_keywords.as_slice())
            .unwrap_or_default();
        let module_score = compute_keyword_score(desc_words, responsibility_words)
            .max(compute_path_score(desc_words, &module.file_path));

        for node in store.get_nodes_in_file(&module.file_path) {
            if node.kind == NodeKind::Module
                || node.in_test_context
                || !matches_kind(&node, kind_filter)
            {
                continue;
            }
            let name_words = words_in(&node.name);
            let signature_words = words_in(&node.signature);
            let doc_words = words_in(node.docstring.as_deref().unwrap_or(""));
            let name_score = compute_keyword_score(desc_words, &name_words);
            let contract_score = compute_keyword_score(desc_words, &signature_words)
                .max(compute_keyword_score(desc_words, &doc_words));
            if name_score == 0.0 && contract_score == 0.0 {
                continue;
            }

            let score =
                round_score(name_score * 0.50 + contract_score * 0.30 + module_score * 0.20);
            if score < MIN_REUSE_SCORE {
                continue;
            }
            candidates.push(candidate(
                store,
                node,
                desc_words,
                name_words,
                signature_words,
                doc_words,
                name_score,
                contract_score,
                module_score,
            ));
        }
    }

    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.callers.cmp(&a.callers))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    candidates.truncate(MAX_REUSE_CANDIDATES);
    candidates
}

#[allow(clippy::too_many_arguments)]
fn candidate(
    store: &dyn GraphStore,
    node: GraphNode,
    desc_words: &[String],
    name_words: Vec<String>,
    signature_words: Vec<String>,
    doc_words: Vec<String>,
    name_score: f64,
    contract_score: f64,
    module_score: f64,
) -> ReuseCandidate {
    let edges = store.get_edges(node.id, EdgeDirection::Both);
    let callers = edges
        .iter()
        .filter(|e| e.target_id == node.id && SYMBOL_DEP_KINDS.contains(&e.kind))
        .count() as u32;
    let callees = edges
        .iter()
        .filter(|e| e.source_id == node.id && e.kind == EdgeKind::Calls)
        .count() as u32;
    let mut evidence = Vec::new();
    if name_score > 0.0 {
        evidence.push(format!(
            "name overlap: {}",
            overlapping_words(desc_words, &name_words).join(", ")
        ));
    }
    if contract_score > 0.0 {
        let mut contract_words = signature_words;
        contract_words.extend(doc_words);
        evidence.push(format!(
            "contract overlap: {}",
            overlapping_words(desc_words, &contract_words).join(", ")
        ));
    }
    if module_score > 0.0 {
        evidence.push(format!("module responsibility: {}", node.file_path));
    }
    ReuseCandidate {
        name: node.name,
        hash: node.hash,
        source: ReuseCandidateSource::Lexical,
        signature: node.signature,
        file: node.file_path,
        line: node.line_start,
        score: round_score(name_score * 0.50 + contract_score * 0.30 + module_score * 0.20),
        callers,
        callees,
        evidence,
    }
}

/// Apply the shared `keel name` kind filter to a stored candidate.
pub(super) fn matches_kind(node: &GraphNode, kind_filter: Option<&str>) -> bool {
    match kind_filter {
        Some("fn" | "function") | None => node.kind == NodeKind::Function,
        Some("class") => node.kind == NodeKind::Class,
        Some(_) => true,
    }
}

fn words_in(text: &str) -> Vec<String> {
    identifier_words(text, 2).into_iter().collect()
}

fn overlapping_words(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .filter(|word| {
            right
                .iter()
                .any(|candidate| candidate.contains(word.as_str()) || word.contains(candidate))
        })
        .cloned()
        .collect()
}

fn round_score(score: f64) -> f64 {
    (score * 100.0).round() / 100.0
}
