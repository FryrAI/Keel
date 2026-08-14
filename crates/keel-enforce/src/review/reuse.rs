//! Reuse advisories for newly added functions in `keel review`.
//!
//! Replacement evidence is the high-confidence path: a changed caller invokes
//! the new function at the relative line where the parsed base blob called an
//! existing one. Parsed base calls are combined with unchanged stored edges,
//! so the result is stable whether the live graph was last mapped at base or
//! head.
//! General role overlap compares caller modules, callee names, signature shape
//! and domain vocabulary. Neither path is a violation or a gate.

mod graph;
mod util;

use std::collections::{BTreeMap, HashSet};

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, GraphNode};
use keel_parsers::resolver::Definition;
use serde::{Deserialize, Serialize};

use super::diff::DiffScan;
use graph::{BaseGraph, HeadGraph, Role, SymbolKey};
use util::{
    advisory, domain_words, exempt_existing, exempt_new, jaccard, role_round_score,
    signature_compatible, signature_evidence,
};

const MAX_ADVISORIES: usize = 20;
const MAX_PER_NEW_SYMBOL: usize = 3;
const MIN_ROLE_SCORE: f64 = 0.72;
const MAX_REPLACEMENT_LINE_DRIFT: u32 = 3;

/// Why an existing function was nominated for reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReuseEvidenceKind {
    Replacement,
    RoleOverlap,
}

/// One explainable, advisory link from a new function to an existing one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReuseAdvisory {
    /// Stable advisory code. Kept outside `new_violations`, so it cannot gate.
    pub code: String,
    /// Warning-shaped presentation without violation/gate semantics.
    pub severity: String,
    pub kind: ReuseEvidenceKind,
    pub new_symbol: String,
    pub new_signature: String,
    pub new_file: String,
    pub new_line: u32,
    pub existing_symbol: String,
    pub existing_signature: String,
    pub existing_file: String,
    pub existing_line: u32,
    pub confidence: f64,
    pub evidence: Vec<String>,
    pub fix_hint: String,
}

/// Find replacement and graph-role reuse candidates for newly added functions.
pub fn detect(store: &dyn GraphStore, scan: &DiffScan) -> Vec<ReuseAdvisory> {
    let head = HeadGraph::build(scan);
    if head.added.is_empty() {
        return Vec::new();
    }
    let base = BaseGraph::load(store, scan);
    let mut out = Vec::new();
    let mut replacement_pairs = HashSet::new();

    for new in &head.added {
        if exempt_new(new, scan) || base.by_name.contains_key(&new.name) {
            continue;
        }
        let new_key = SymbolKey::of_definition(new);
        let replacements = replacement_advisories(scan, &head, &base, new, &new_key);
        replacement_pairs.extend(replacements.iter().map(advisory_pair));
        out.extend(replacements);
    }

    for new in &head.added {
        if exempt_new(new, scan) || base.by_name.contains_key(&new.name) {
            continue;
        }
        let new_key = SymbolKey::of_definition(new);
        let mut overlaps = role_advisories(store, scan, &head, &base, new, &new_key);
        overlaps.retain(|a| !replacement_pairs.contains(&advisory_pair(a)));
        overlaps.truncate(MAX_PER_NEW_SYMBOL);
        out.extend(overlaps);
    }

    out.sort_by(|a, b| {
        evidence_rank(a.kind)
            .cmp(&evidence_rank(b.kind))
            .then_with(|| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.new_file.cmp(&b.new_file))
            .then_with(|| a.new_line.cmp(&b.new_line))
            .then_with(|| a.existing_file.cmp(&b.existing_file))
    });
    out.truncate(MAX_ADVISORIES);
    out
}

fn evidence_rank(kind: ReuseEvidenceKind) -> u8 {
    match kind {
        ReuseEvidenceKind::Replacement => 0,
        ReuseEvidenceKind::RoleOverlap => 1,
    }
}

fn advisory_pair(advisory: &ReuseAdvisory) -> (String, u32, String, u32) {
    (
        advisory.new_file.clone(),
        advisory.new_line,
        advisory.existing_file.clone(),
        advisory.existing_line,
    )
}

fn replacement_advisories(
    scan: &DiffScan,
    head: &HeadGraph<'_>,
    base: &BaseGraph,
    new: &Definition,
    new_key: &SymbolKey,
) -> Vec<ReuseAdvisory> {
    let mut matches: BTreeMap<u64, (u32, String, u32, u32)> = BTreeMap::new();
    for call in head.callers.get(new_key).into_iter().flatten() {
        let base_caller = base.key_for_head_owner(scan, &call.owner);
        let head_calls: HashSet<&str> = head
            .calls
            .get(&call.owner)
            .into_iter()
            .flatten()
            .map(|(name, _, _, _)| name.as_str())
            .collect();
        for (called_name, base_line, base_offset, _) in
            base.calls.get(&base_caller).into_iter().flatten()
        {
            let drift = base_offset.abs_diff(call.offset);
            if drift > MAX_REPLACEMENT_LINE_DRIFT {
                continue;
            }
            let Some(candidate_ids) = base.by_name.get(called_name) else {
                continue;
            };
            if candidate_ids.len() != 1 {
                continue;
            }
            let Some(candidate) = base.by_id.get(&candidate_ids[0]) else {
                continue;
            };
            if head_calls.contains(candidate.name.as_str())
                || !signature_compatible(new, candidate)
                || exempt_existing(candidate)
            {
                continue;
            }
            matches
                .entry(candidate.id)
                .and_modify(|best| {
                    if drift < best.0 {
                        *best = (drift, call.owner.name.clone(), call.line, *base_line);
                    }
                })
                .or_insert((drift, call.owner.name.clone(), call.line, *base_line));
        }
    }

    matches
        .into_iter()
        .filter_map(|(id, (drift, caller, head_line, base_line))| {
            let candidate = base.by_id.get(&id)?;
            let confidence = if drift == 0 { 0.92 } else { 0.86 };
            Some(advisory(
                ReuseEvidenceKind::Replacement,
                new,
                candidate,
                confidence,
                vec![
                    format!("`{caller}` replaced a base call at relative drift {drift} (base line {base_line}, head line {head_line})"),
                    signature_evidence(new),
                ],
            ))
        })
        .collect()
}

fn role_advisories(
    store: &dyn GraphStore,
    scan: &DiffScan,
    head: &HeadGraph<'_>,
    base: &BaseGraph,
    new: &Definition,
    new_key: &SymbolKey,
) -> Vec<ReuseAdvisory> {
    let mut new_role = new_role(scan, head, new_key);
    // Role comparison is about project-graph position. Method calls and
    // language/runtime helpers such as `is_empty`, `round`, and `Some` are
    // syntactic references, not resolved project callees; treating them as
    // graph evidence makes unrelated leaf helpers look identical.
    new_role
        .callee_names
        .retain(|name| base.by_name.get(name).is_some_and(|ids| ids.len() == 1));
    let candidate_ids = structural_candidates(store, scan, head, base, new_key, &new_role);
    let new_domain = domain_words(&format!(
        "{} {} {}",
        new.name,
        new.docstring.as_deref().unwrap_or(""),
        new.file_path
    ));
    let mut advisories = Vec::new();
    for id in candidate_ids {
        let Some(candidate) = base.by_id.get(&id) else {
            continue;
        };
        if !signature_compatible(new, candidate) || exempt_existing(candidate) {
            continue;
        }
        let old_role = stored_role(store, base, candidate);
        let caller_score = jaccard(&new_role.caller_files, &old_role.caller_files);
        let callee_score = jaccard(&new_role.callee_names, &old_role.callee_names);
        let old_domain = domain_words(&format!(
            "{} {} {}",
            candidate.name,
            candidate.docstring.as_deref().unwrap_or(""),
            candidate.file_path
        ));
        let domain_score = jaccard(&new_domain, &old_domain);
        let score = role_round_score(
            caller_score * 0.35 + callee_score * 0.30 + 0.20 + domain_score * 0.15,
        );
        let graph_signals = usize::from(caller_score > 0.0) + usize::from(callee_score > 0.0);
        let sparse = new_role.caller_files.len().max(old_role.caller_files.len()) < 2
            && new_role.callee_names.len().max(old_role.callee_names.len()) < 2;
        if score < MIN_ROLE_SCORE || graph_signals == 0 || (sparse && graph_signals < 2) {
            continue;
        }
        let mut evidence = vec![signature_evidence(new)];
        if caller_score > 0.0 {
            evidence.push(format!("caller-module overlap={caller_score:.2}"));
        }
        if callee_score > 0.0 {
            evidence.push(format!("callee-role overlap={callee_score:.2}"));
        }
        if domain_score > 0.0 {
            evidence.push(format!("domain overlap={domain_score:.2}"));
        }
        advisories.push(advisory(
            ReuseEvidenceKind::RoleOverlap,
            new,
            candidate,
            score,
            evidence,
        ));
    }
    advisories.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.existing_file.cmp(&b.existing_file))
            .then_with(|| a.existing_line.cmp(&b.existing_line))
    });
    advisories
}

fn structural_candidates(
    store: &dyn GraphStore,
    scan: &DiffScan,
    head: &HeadGraph<'_>,
    base: &BaseGraph,
    new_key: &SymbolKey,
    role: &Role,
) -> HashSet<u64> {
    let mut ids = HashSet::new();
    for caller in head.callers.get(new_key).into_iter().flatten() {
        let base_caller = base.key_for_head_owner(scan, &caller.owner);
        for (called_name, _, _, _) in base.calls.get(&base_caller).into_iter().flatten() {
            if let Some(candidate_ids) = base.by_name.get(called_name) {
                if candidate_ids.len() == 1 {
                    ids.insert(candidate_ids[0]);
                }
            }
        }
    }
    for callee in &role.callee_names {
        for call in base.callers_by_name.get(callee).into_iter().flatten() {
            if let Some(candidate) = base.node_for_key(&call.owner) {
                ids.insert(candidate.id);
            }
        }
        for target_id in base.by_name.get(callee).into_iter().flatten() {
            for edge in store
                .get_edges(*target_id, EdgeDirection::Incoming)
                .into_iter()
                .filter(|edge| edge.kind == EdgeKind::Calls)
            {
                if base
                    .by_id
                    .get(&edge.source_id)
                    .is_some_and(|source| !base.changed_files.contains(&source.file_path))
                {
                    ids.insert(edge.source_id);
                }
            }
        }
    }
    ids
}

fn new_role(scan: &DiffScan, head: &HeadGraph<'_>, key: &SymbolKey) -> Role {
    let caller_files = head
        .callers
        .get(key)
        .into_iter()
        .flatten()
        .map(|call| {
            scan.renames
                .get(&call.owner.file)
                .unwrap_or(&call.owner.file)
                .clone()
        })
        .collect();
    let callee_names = head
        .calls
        .get(key)
        .into_iter()
        .flatten()
        .filter(|(_, _, _, eligible)| *eligible)
        .map(|(name, _, _, _)| name.clone())
        .collect();
    Role {
        caller_files,
        callee_names,
    }
}

fn stored_role(store: &dyn GraphStore, base: &BaseGraph, node: &GraphNode) -> Role {
    let mut role = Role::default();
    for edge in store.get_edges(node.id, EdgeDirection::Both) {
        if edge.kind != EdgeKind::Calls {
            continue;
        }
        if edge.target_id == node.id {
            if let Some(caller) = base.by_id.get(&edge.source_id) {
                if !base.changed_files.contains(&caller.file_path) {
                    role.caller_files.insert(caller.file_path.clone());
                }
            }
        } else if edge.source_id == node.id && !base.changed_files.contains(&node.file_path) {
            if let Some(callee) = base.by_id.get(&edge.target_id) {
                role.callee_names.insert(callee.name.clone());
            }
        }
    }
    if base
        .by_name
        .get(&node.name)
        .is_some_and(|ids| ids == &[node.id])
    {
        for call in base.callers_by_name.get(&node.name).into_iter().flatten() {
            role.caller_files.insert(call.owner.file.clone());
        }
    }
    let key = SymbolKey {
        file: node.file_path.clone(),
        name: node.name.clone(),
    };
    if base.changed_files.contains(&node.file_path) {
        role.callee_names.extend(
            base.calls
                .get(&key)
                .into_iter()
                .flatten()
                .filter(|(_, _, _, eligible)| *eligible)
                .map(|(name, _, _, _)| name.clone()),
        );
    }
    role.callee_names
        .retain(|name| base.by_name.get(name).is_some_and(|ids| ids.len() == 1));
    role
}

#[cfg(test)]
#[path = "reuse_tests.rs"]
mod tests;
