use std::collections::HashMap;
use std::path::PathBuf;

use crate::resolver::{Import, ParseResult, ResolvedEdge};

/// Resolve a star import by looking up the target module in the cache.
///
/// Confidence levels:
/// - 0.65: callee found in target's `__all__`
/// - 0.50: callee is public in target but not in `__all__`
/// - 0.40: callee found in multiple star import sources (ambiguous)
/// - 0.40: target module itself has star imports (chain)
pub fn resolve_star_import(
    cache: &HashMap<PathBuf, ParseResult>,
    caller_imports: &[Import],
    callee_name: &str,
    primary_import: &Import,
) -> Option<ResolvedEdge> {
    let star_imports: Vec<&Import> = caller_imports
        .iter()
        .filter(|imp| imp.imported_names.contains(&"*".to_string()))
        .collect();

    let mut matches: Vec<(&Import, f64)> = Vec::new();
    for star_imp in &star_imports {
        if let Some((confidence, _)) = find_in_star_target(cache, &star_imp.source, callee_name) {
            matches.push((star_imp, confidence));
        }
    }

    if matches.len() > 1 {
        let best = matches
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())?;
        return Some(ResolvedEdge {
            target_file: best.0.source.clone(),
            target_name: callee_name.to_string(),
            confidence: 0.40,
            resolution_tier: "tier2_heuristic".into(),
        });
    }

    if let Some((matched_imp, confidence)) = matches.into_iter().next() {
        return Some(ResolvedEdge {
            target_file: matched_imp.source.clone(),
            target_name: callee_name.to_string(),
            confidence,
            resolution_tier: "tier2_heuristic".into(),
        });
    }

    // Fallback: no cache hit, return generic star import edge
    Some(ResolvedEdge {
        target_file: primary_import.source.clone(),
        target_name: callee_name.to_string(),
        confidence: 0.50,
        resolution_tier: "tier2_heuristic".into(),
    })
}

/// Look up a callee name in a cached target module.
/// Returns (confidence, has_star_chain) if found.
fn find_in_star_target(
    cache: &HashMap<PathBuf, ParseResult>,
    import_source: &str,
    callee_name: &str,
) -> Option<(f64, bool)> {
    let target_result = find_cached_module(cache, import_source)?;
    let has_star_chain = target_result
        .imports
        .iter()
        .any(|imp| imp.imported_names.contains(&"*".to_string()));

    for def in &target_result.definitions {
        if def.name == callee_name {
            if has_star_chain {
                return Some((0.40, true));
            }
            if def.is_public {
                // Detect __all__ usage: if any non-module, non-underscore def
                // is private, then __all__ is restricting visibility.
                let has_all = target_result.definitions.iter().any(|d| {
                    d.kind != keel_core::types::NodeKind::Module
                        && !d.is_public
                        && !d.name.starts_with('_')
                });
                if has_all {
                    return Some((0.65, false));
                }
                return Some((0.50, false));
            }
            // Name exists but is private
            return Some((0.50, false));
        }
    }
    None
}

/// Find a cached parse result matching an import source string.
/// Tries multiple matching strategies: exact path, filename stem, module path.
fn find_cached_module<'a>(
    cache: &'a HashMap<PathBuf, ParseResult>,
    import_source: &str,
) -> Option<&'a ParseResult> {
    // Strategy 1: exact path match
    let exact = PathBuf::from(import_source);
    if let Some(result) = cache.get(&exact) {
        return Some(result);
    }
    // Strategy 2: try with .py extension
    let with_ext = PathBuf::from(format!("{import_source}.py"));
    if let Some(result) = cache.get(&with_ext) {
        return Some(result);
    }
    // Strategy 3: match by filename stem or module path
    let module_parts: Vec<&str> = import_source.split('.').collect();
    let last_part = module_parts.last().unwrap_or(&import_source);
    for (path, result) in cache {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if stem == *last_part {
            return Some(result);
        }
        let path_str = path.to_string_lossy();
        let as_path = import_source.replace('.', "/");
        if path_str.ends_with(&format!("{as_path}.py"))
            || path_str.ends_with(&format!("{as_path}/__init__.py"))
        {
            return Some(result);
        }
    }
    None
}
