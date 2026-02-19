use crate::types::{NameAlternative, NameResult, NameSuggestion};
use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, NodeKind};

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
    let desc_words = extract_keywords(description);
    let modules = store.get_all_modules();

    // Score each module
    let mut scored: Vec<(f64, keel_core::types::GraphNode)> = modules
        .into_iter()
        .filter(|m| {
            if let Some(filter) = module_filter {
                m.file_path.contains(filter)
            } else {
                true
            }
        })
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
            version: "0.1.0".to_string(),
            command: "name".to_string(),
            description: description.to_string(),
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
        version: "0.1.0".to_string(),
        command: "name".to_string(),
        description: description.to_string(),
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

/// Detect naming convention from sibling function names.
fn detect_convention(names: &[&str]) -> NamingConvention {
    if names.is_empty() {
        return NamingConvention::SnakeCase { prefix: None };
    }

    let snake_count = names.iter().filter(|n| n.contains('_')).count();
    let camel_count = names
        .iter()
        .filter(|n| !n.contains('_') && n.chars().any(|c| c.is_uppercase()))
        .count();

    // Detect common prefix
    let prefix = detect_common_prefix(names);

    if snake_count >= camel_count {
        NamingConvention::SnakeCase { prefix }
    } else {
        NamingConvention::CamelCase { prefix }
    }
}

/// Detect common prefix in function names.
fn detect_common_prefix(names: &[&str]) -> Option<String> {
    if names.len() < 2 {
        return None;
    }

    // For snake_case: find common prefix before first underscore
    let prefixes: Vec<&str> = names.iter().filter_map(|n| n.split('_').next()).collect();

    if prefixes.is_empty() {
        return None;
    }

    let first = prefixes[0];
    let matching = prefixes.iter().filter(|p| **p == first).count();

    // If majority share a prefix, report it
    if matching * 2 >= names.len() && !first.is_empty() {
        Some(format!("{}_", first))
    } else {
        None
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

/// Generate a suggested name from description keywords and convention.
fn generate_name(desc_words: &[String], convention: &NamingConvention) -> String {
    let filtered: Vec<&str> = desc_words
        .iter()
        .take(4) // Max 4 words in name
        .map(|s| s.as_str())
        .collect();

    match convention {
        NamingConvention::SnakeCase { prefix } => {
            let base = filtered.join("_");
            if let Some(p) = prefix {
                format!("{}{}", p, base)
            } else {
                base
            }
        }
        NamingConvention::CamelCase { prefix } => {
            let base: String = filtered
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    if i == 0 {
                        w.to_string()
                    } else {
                        let mut c = w.chars();
                        match c.next() {
                            None => String::new(),
                            Some(first) => first.to_uppercase().to_string() + c.as_str(),
                        }
                    }
                })
                .collect();
            if let Some(p) = prefix {
                format!("{}{}", p, base)
            } else {
                base
            }
        }
    }
}

#[derive(Debug)]
enum NamingConvention {
    SnakeCase { prefix: Option<String> },
    CamelCase { prefix: Option<String> },
}

impl std::fmt::Display for NamingConvention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NamingConvention::SnakeCase { prefix } => {
                write!(f, "snake_case")?;
                if let Some(p) = prefix {
                    write!(f, ", prefix: {}", p)?;
                }
                Ok(())
            }
            NamingConvention::CamelCase { prefix } => {
                write!(f, "camelCase")?;
                if let Some(p) = prefix {
                    write!(f, ", prefix: {}", p)?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[path = "naming_tests.rs"]
mod tests;
