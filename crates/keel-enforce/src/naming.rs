mod convention;
mod reuse;
mod semantic_candidates;

use crate::types::{NameAlternative, NameResult, NameSuggestion};
use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, NodeKind};

#[cfg(test)]
use convention::{detect_common_prefix, NamingConvention};
use convention::{detect_convention, generate_name};
use reuse::find_reuse_candidates;
use semantic_candidates::find_semantic_candidates;

/// Optional, candidate-only naming behaviors.
#[derive(Debug, Clone, Copy, Default)]
pub struct NameOptions {
    /// Expand domain concepts (for example `unix` ↔ `timestamp`). Results are
    /// hints only and are never consumed by W010, P003, or a gate.
    pub semantic_candidates: bool,
}

/// Existing symbols that may satisfy a requested intent.
///
/// This is candidate generation only: callers must not promote a lexical
/// match into an equivalence claim, violation, or gate.
pub(crate) fn reuse_candidates(
    store: &dyn GraphStore,
    description: &str,
) -> Vec<crate::types::ReuseCandidate> {
    let desc_words = extract_keywords(description);
    let modules = store.get_all_modules();
    find_reuse_candidates(store, &modules, &desc_words, None, Some("fn"))
}

/// Suggest a name and location for new code.
///
/// Scores modules by keyword overlap with the description, detects naming
/// conventions from siblings, and suggests insertion points.
pub fn suggest_name(
    store: &dyn GraphStore,
    description: &str,
    module_filter: Option<&str>,
    kind_filter: Option<&str>,
) -> NameResult {
    suggest_name_with_options(
        store,
        description,
        module_filter,
        kind_filter,
        NameOptions::default(),
    )
}

/// Suggest a name and location with explicitly enabled candidate generators.
pub fn suggest_name_with_options(
    store: &dyn GraphStore,
    description: &str,
    module_filter: Option<&str>,
    kind_filter: Option<&str>,
    options: NameOptions,
) -> NameResult {
    let desc_words = extract_keywords(description);
    let modules = store.get_all_modules();
    let mut reuse_candidates =
        find_reuse_candidates(store, &modules, &desc_words, module_filter, kind_filter);
    if options.semantic_candidates {
        reuse_candidates.extend(find_semantic_candidates(
            store,
            &modules,
            description,
            module_filter,
            kind_filter,
        ));
        let mut seen = std::collections::HashSet::new();
        reuse_candidates.retain(|candidate| seen.insert(candidate.hash.clone()));
        reuse_candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.file.cmp(&b.file))
                .then_with(|| a.line.cmp(&b.line))
        });
        reuse_candidates.truncate(5);
    }

    // Score each module
    let mut scored: Vec<(f64, keel_core::types::GraphNode)> = modules
        .iter()
        .filter(|m| {
            if let Some(filter) = module_filter {
                m.file_path.contains(filter)
            } else {
                true
            }
        })
        .cloned()
        .map(|m| {
            let profile = store.get_module_profile(m.id);
            let keyword_score = if let Some(ref p) = profile {
                compute_keyword_score(&desc_words, &p.responsibility_keywords)
            } else {
                0.0
            };
            // Fallback scoring when keyword match produces nothing
            let score = if keyword_score > 0.0 {
                keyword_score
            } else {
                compute_fallback_score(&desc_words, &m.file_path, store, m.id)
            };
            (score, m)
        })
        .filter(|(score, _)| *score > 0.0)
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // No matches at all, or all scores below confidence threshold
    if scored.is_empty() || scored[0].0 < 0.3 {
        return NameResult {
            version: env!("CARGO_PKG_VERSION").to_string(),
            command: "name".to_string(),
            description: description.to_string(),
            reuse_candidates,
            suggestions: vec![],
        };
    }

    let (best_score, ref best_module) = scored[0];
    let best_profile = store.get_module_profile(best_module.id);

    // Get sibling functions in the best module
    let nodes_in_file = store.get_nodes_in_file(&best_module.file_path);
    let sibling_fns: Vec<&keel_core::types::GraphNode> = nodes_in_file
        .iter()
        .filter(|n| {
            if let Some(kind) = kind_filter {
                match kind {
                    "fn" | "function" => matches!(n.kind, NodeKind::Function),
                    "class" => matches!(n.kind, NodeKind::Class),
                    _ => true,
                }
            } else {
                matches!(n.kind, NodeKind::Function)
            }
        })
        .collect();

    // Detect naming convention
    let sibling_names: Vec<&str> = sibling_fns.iter().map(|n| n.name.as_str()).collect();
    let convention = detect_convention(&sibling_names);
    let suggested_name = generate_name(&desc_words, &convention);

    // Find insertion point (function with best keyword overlap)
    let (insert_after, insert_line) = find_insertion_point(&sibling_fns, &desc_words);

    // Collect likely imports from siblings
    let likely_imports = collect_sibling_imports(store, &sibling_fns);

    // Build alternatives from next-best modules
    let alternatives: Vec<NameAlternative> = scored
        .iter()
        .skip(1)
        .take(3)
        .map(|(score, m)| {
            let kw = store
                .get_module_profile(m.id)
                .map(|p| p.responsibility_keywords.clone())
                .unwrap_or_default();
            NameAlternative {
                location: m.file_path.clone(),
                score: *score,
                keywords: kw,
            }
        })
        .collect();

    let keywords = best_profile
        .as_ref()
        .map(|p| p.responsibility_keywords.clone())
        .unwrap_or_default();

    NameResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "name".to_string(),
        description: description.to_string(),
        reuse_candidates,
        suggestions: vec![NameSuggestion {
            location: best_module.file_path.clone(),
            score: best_score,
            keywords,
            alternatives,
            insert_after: insert_after.map(|s| s.to_string()),
            insert_line,
            convention: convention.to_string(),
            suggested_name,
            likely_imports,
            siblings: sibling_names.iter().map(|s| s.to_string()).collect(),
        }],
    }
}

/// Extract keywords from a description string (lowercase, deduped).
fn extract_keywords(description: &str) -> Vec<String> {
    let stop_words = [
        "a", "an", "the", "and", "or", "for", "to", "in", "of", "with", "on",
    ];
    description
        .split_whitespace()
        .map(|w| {
            w.to_lowercase()
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        })
        .filter(|w| w.len() > 1 && !stop_words.contains(&w.as_str()))
        .collect()
}

/// Compute keyword overlap score between description and module keywords.
fn compute_keyword_score(desc_words: &[String], module_keywords: &[String]) -> f64 {
    if desc_words.is_empty() || module_keywords.is_empty() {
        return 0.0;
    }
    let matches = desc_words
        .iter()
        .filter(|w| {
            module_keywords
                .iter()
                .any(|k| k.contains(w.as_str()) || w.contains(k.as_str()))
        })
        .count();
    matches as f64 / desc_words.len() as f64
}

/// Fallback scoring when module_profiles have no keywords.
/// 65% weight on path segment match, 35% on function name match.
/// Path segments are a stronger signal: a file named `graph_data.py` is
/// almost certainly the right home for "export graph data" regardless of
/// which functions already live there.
fn compute_fallback_score(
    desc_words: &[String],
    file_path: &str,
    store: &dyn GraphStore,
    module_id: u64,
) -> f64 {
    if desc_words.is_empty() {
        return 0.0;
    }
    let path_score = compute_path_score(desc_words, file_path);
    let fn_score = compute_function_name_score(desc_words, store, module_id);
    let combined = path_score * 0.65 + fn_score * 0.35;
    // Only return if there's a meaningful match
    if combined > 0.05 {
        combined
    } else {
        0.0
    }
}

/// Match description words against file path segments.
fn compute_path_score(desc_words: &[String], file_path: &str) -> f64 {
    let segments: Vec<String> = file_path
        .replace('\\', "/")
        .split('/')
        .flat_map(|seg| {
            let seg = seg.rsplit_once('.').map(|(name, _)| name).unwrap_or(seg);
            seg.split(|c: char| c == '_' || c.is_uppercase())
                .filter(|w| !w.is_empty())
                .map(|w| w.to_lowercase())
                .collect::<Vec<_>>()
        })
        .collect();
    if segments.is_empty() {
        return 0.0;
    }
    let matches = desc_words
        .iter()
        .filter(|w| {
            segments
                .iter()
                .any(|s| s.contains(w.as_str()) || w.contains(s.as_str()))
        })
        .count();
    matches as f64 / desc_words.len() as f64
}

/// Match description words against function names in the module.
fn compute_function_name_score(
    desc_words: &[String],
    store: &dyn GraphStore,
    module_id: u64,
) -> f64 {
    if let Some(profile) = store.get_module_profile(module_id) {
        let nodes = store.get_nodes_in_file(&profile.path);
        let fn_words: Vec<String> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .flat_map(|n| {
                n.name
                    .split(|c: char| c == '_' || c.is_uppercase())
                    .filter(|w| !w.is_empty())
                    .map(|w| w.to_lowercase())
                    .collect::<Vec<_>>()
            })
            .collect();
        compute_keyword_score(desc_words, &fn_words)
    } else {
        0.0
    }
}

/// Find the best insertion point among sibling functions.
fn find_insertion_point<'a>(
    siblings: &[&'a keel_core::types::GraphNode],
    desc_words: &[String],
) -> (Option<&'a str>, Option<u32>) {
    if siblings.is_empty() {
        return (None, None);
    }

    // Score each sibling by keyword overlap with description
    let mut best_score = 0.0f64;
    let mut best_sibling: Option<&keel_core::types::GraphNode> = None;

    for &node in siblings {
        let name_words: Vec<String> = node
            .name
            .split(|c: char| c == '_' || c.is_uppercase())
            .map(|w| w.to_lowercase())
            .filter(|w| !w.is_empty())
            .collect();

        let score = compute_keyword_score(desc_words, &name_words);
        if score > best_score {
            best_score = score;
            best_sibling = Some(node);
        }
    }

    match best_sibling {
        Some(node) => (Some(&node.name), Some(node.line_end)),
        None => {
            // Default to after the last function
            let last = siblings.last().unwrap();
            (Some(&last.name), Some(last.line_end))
        }
    }
}

/// Collect imports used by sibling functions (unique, sorted).
fn collect_sibling_imports(
    store: &dyn GraphStore,
    siblings: &[&keel_core::types::GraphNode],
) -> Vec<String> {
    let mut imports = std::collections::BTreeSet::new();
    for &node in siblings {
        let edges = store.get_edges(node.id, EdgeDirection::Outgoing);
        for edge in edges {
            if matches!(edge.kind, keel_core::types::EdgeKind::Imports) {
                if let Some(target) = store.get_node_by_id(edge.target_id) {
                    imports.insert(target.name.clone());
                }
            }
        }
    }
    imports.into_iter().take(10).collect()
}

#[cfg(test)]
#[path = "naming_tests.rs"]
mod tests;
