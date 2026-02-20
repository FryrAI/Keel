use std::collections::{HashMap, HashSet};
use std::path::Path;

use keel_core::types::{EdgeChange, EdgeKind, ModuleProfile, NodeChange, NodeKind};
use keel_enforce::types::ModuleFunctionRef;

/// Build a MapResult from collected node and edge data (before they are consumed).
pub fn build_map_result(
    node_changes: &[NodeChange],
    valid_edges: &[EdgeChange],
    entries: &[keel_parsers::walker::WalkEntry],
) -> keel_enforce::types::MapResult {
    use keel_enforce::types::{MapResult, MapSummary, ModuleEntry};

    let nodes: Vec<_> = node_changes
        .iter()
        .filter_map(|c| match c {
            NodeChange::Add(n) => Some(n),
            _ => None,
        })
        .collect();

    let total_nodes = nodes.len() as u32;
    let total_edges = valid_edges
        .iter()
        .filter(|e| matches!(e, EdgeChange::Add(_)))
        .count() as u32;
    let modules_count = nodes.iter().filter(|n| n.kind == NodeKind::Module).count() as u32;
    let functions_count = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Function)
        .count() as u32;
    let classes_count = nodes.iter().filter(|n| n.kind == NodeKind::Class).count() as u32;

    let non_module_nodes: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind != NodeKind::Module)
        .collect();
    let type_hint_count = non_module_nodes
        .iter()
        .filter(|n| n.type_hints_present)
        .count();
    let docstring_count = non_module_nodes.iter().filter(|n| n.has_docstring).count();
    let type_hint_coverage = if non_module_nodes.is_empty() {
        0.0
    } else {
        type_hint_count as f64 / non_module_nodes.len() as f64
    };
    let docstring_coverage = if non_module_nodes.is_empty() {
        0.0
    } else {
        docstring_count as f64 / non_module_nodes.len() as f64
    };

    let mut languages: HashSet<String> = HashSet::new();
    for entry in entries {
        languages.insert(entry.language.clone());
    }
    let mut langs: Vec<String> = languages.into_iter().collect();
    langs.sort();

    let external_endpoint_count = nodes
        .iter()
        .map(|n| n.external_endpoints.len())
        .sum::<usize>() as u32;

    // Build caller/callee count maps for function refs
    let mut callers_map: HashMap<u64, u32> = HashMap::new();
    let mut callees_map: HashMap<u64, u32> = HashMap::new();
    for e in valid_edges {
        if let EdgeChange::Add(edge) = e {
            if edge.kind == EdgeKind::Calls {
                *callers_map.entry(edge.target_id).or_default() += 1;
                *callees_map.entry(edge.source_id).or_default() += 1;
            }
        }
    }

    // Per-module entries: count functions, classes, edges per module
    let mut module_entries = Vec::new();
    for node in &nodes {
        if node.kind != NodeKind::Module {
            continue;
        }
        let module_id = node.id;
        let file_path = &node.file_path;

        let fn_count = nodes
            .iter()
            .filter(|n| n.module_id == module_id && n.kind == NodeKind::Function)
            .count() as u32;
        let cls_count = nodes
            .iter()
            .filter(|n| n.module_id == module_id && n.kind == NodeKind::Class)
            .count() as u32;
        let edge_count = valid_edges
            .iter()
            .filter(|e| match e {
                EdgeChange::Add(edge) => &edge.file_path == file_path,
                _ => false,
            })
            .count() as u32;

        // Collect function names + hashes for this module
        let fn_refs: Vec<ModuleFunctionRef> = nodes
            .iter()
            .filter(|n| n.module_id == module_id && n.kind == NodeKind::Function)
            .map(|n| {
                let c = callers_map.get(&n.id).copied().unwrap_or(0);
                let ce = callees_map.get(&n.id).copied().unwrap_or(0);
                ModuleFunctionRef {
                    name: n.name.clone(),
                    hash: n.hash.clone(),
                    callers: c,
                    callees: ce,
                }
            })
            .collect();

        module_entries.push(ModuleEntry {
            path: file_path.clone(),
            function_count: fn_count,
            class_count: cls_count,
            edge_count,
            responsibility_keywords: None,
            external_endpoints: None,
            function_names: fn_refs,
        });
    }

    MapResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "map".to_string(),
        summary: MapSummary {
            total_nodes,
            total_edges,
            modules: modules_count,
            functions: functions_count,
            classes: classes_count,
            external_endpoints: external_endpoint_count,
            languages: langs,
            type_hint_coverage,
            docstring_coverage,
        },
        modules: module_entries,
        hotspots: vec![], // Populated later from store if depth >= 1
        depth: 1,
        functions: vec![], // Populated later if depth >= 2
    }
}

/// Make a path relative to the project root.
pub fn make_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

/// Populate hotspot entries by ranking non-module nodes by total connectivity.
pub fn populate_hotspots(
    result: &mut keel_enforce::types::MapResult,
    node_changes: &[NodeChange],
    valid_edges: &[EdgeChange],
) {
    use keel_enforce::types::HotspotEntry;

    let nodes: Vec<_> = node_changes
        .iter()
        .filter_map(|c| match c {
            NodeChange::Add(n) if n.kind != NodeKind::Module => Some(n),
            _ => None,
        })
        .collect();

    // Count incoming (callers) and outgoing (callees) Calls edges per node
    let mut callers: HashMap<u64, u32> = HashMap::new();
    let mut callees: HashMap<u64, u32> = HashMap::new();
    for e in valid_edges {
        if let EdgeChange::Add(edge) = e {
            if edge.kind == EdgeKind::Calls {
                *callers.entry(edge.target_id).or_default() += 1;
                *callees.entry(edge.source_id).or_default() += 1;
            }
        }
    }

    // Score and rank by total connectivity
    let mut scored: Vec<_> = nodes
        .iter()
        .map(|n| {
            let c = callers.get(&n.id).copied().unwrap_or(0);
            let ce = callees.get(&n.id).copied().unwrap_or(0);
            (c + ce, n, c, ce)
        })
        .filter(|(total, _, _, _)| *total > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));

    result.hotspots = scored
        .into_iter()
        .take(10)
        .map(|(_, n, c, ce)| HotspotEntry {
            path: n.file_path.clone(),
            name: n.name.clone(),
            hash: n.hash.clone(),
            callers: c,
            callees: ce,
            keywords: vec![], // Keywords come from module profile, not available here
        })
        .collect();
}

/// Populate function-level entries for depth >= 2 output.
pub fn populate_functions(
    result: &mut keel_enforce::types::MapResult,
    node_changes: &[NodeChange],
    valid_edges: &[EdgeChange],
) {
    use keel_enforce::types::FunctionEntry;

    let functions: Vec<_> = node_changes
        .iter()
        .filter_map(|c| match c {
            NodeChange::Add(n) if n.kind == NodeKind::Function => Some(n),
            _ => None,
        })
        .collect();

    // Count callers/callees per function
    let mut callers: HashMap<u64, u32> = HashMap::new();
    let mut callees: HashMap<u64, u32> = HashMap::new();
    for e in valid_edges {
        if let EdgeChange::Add(edge) = e {
            if edge.kind == EdgeKind::Calls {
                *callers.entry(edge.target_id).or_default() += 1;
                *callees.entry(edge.source_id).or_default() += 1;
            }
        }
    }

    result.functions = functions
        .iter()
        .map(|n| FunctionEntry {
            hash: n.hash.clone(),
            name: n.name.clone(),
            signature: n.signature.clone(),
            file: n.file_path.clone(),
            line: n.line_start,
            callers: callers.get(&n.id).copied().unwrap_or(0),
            callees: callees.get(&n.id).copied().unwrap_or(0),
            is_public: n.is_public,
        })
        .collect();
}

/// Build module profiles from node changes for populating the module_profiles table.
/// Generates responsibility_keywords from file paths and function/class names.
pub fn build_module_profiles(node_changes: &[NodeChange]) -> Vec<ModuleProfile> {
    let nodes: Vec<_> = node_changes
        .iter()
        .filter_map(|c| match c {
            NodeChange::Add(n) => Some(n),
            _ => None,
        })
        .collect();

    let modules: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Module)
        .collect();

    modules
        .iter()
        .map(|m| {
            let module_id = m.id;
            let children: Vec<_> = nodes
                .iter()
                .filter(|n| n.module_id == module_id && n.kind != NodeKind::Module)
                .collect();

            let fn_count = children
                .iter()
                .filter(|n| n.kind == NodeKind::Function)
                .count() as u32;
            let cls_count = children
                .iter()
                .filter(|n| n.kind == NodeKind::Class)
                .count() as u32;
            let line_count = m.line_end.saturating_sub(m.line_start) + 1;

            // Extract keywords from file path segments
            let path_keywords = extract_path_keywords(&m.file_path);

            // Extract keywords from function/class names
            let name_keywords = extract_name_keywords(&children);

            // Combine and deduplicate
            let mut keywords: Vec<String> = path_keywords;
            keywords.extend(name_keywords);
            keywords.sort();
            keywords.dedup();
            keywords.truncate(20); // Cap at 20 keywords

            // Extract function name prefixes
            let prefixes = extract_function_prefixes(&children);

            ModuleProfile {
                module_id,
                path: m.file_path.clone(),
                function_count: fn_count,
                class_count: cls_count,
                line_count,
                function_name_prefixes: prefixes,
                primary_types: vec![],
                import_sources: vec![],
                export_targets: vec![],
                external_endpoint_count: m.external_endpoints.len() as u32,
                responsibility_keywords: keywords,
            }
        })
        .collect()
}

/// Extract keywords from a file path (e.g., "src/auth/middleware.rs" -> ["auth", "middleware"]).
fn extract_path_keywords(path: &str) -> Vec<String> {
    let stop_words = [
        "src", "lib", "app", "pkg", "cmd", "internal", "mod", "index", "main",
    ];
    path.replace('\\', "/")
        .split('/')
        .flat_map(|seg| {
            // Strip extension from last segment
            let seg = seg.rsplit_once('.').map(|(name, _)| name).unwrap_or(seg);
            split_identifier(seg)
        })
        .filter(|w| w.len() > 1 && !stop_words.contains(&w.as_str()))
        .collect()
}

/// Extract keywords from function/class names in a module.
fn extract_name_keywords(children: &[&&keel_core::types::GraphNode]) -> Vec<String> {
    children
        .iter()
        .flat_map(|n| split_identifier(&n.name))
        .collect()
}

/// Split an identifier into words by underscore or camelCase boundaries.
fn split_identifier(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    // First split on underscores
    for part in name.split('_') {
        // Then split on camelCase boundaries
        let mut current = String::new();
        for ch in part.chars() {
            if ch.is_uppercase() && !current.is_empty() {
                words.push(current.to_lowercase());
                current = String::new();
            }
            current.push(ch);
        }
        if !current.is_empty() {
            words.push(current.to_lowercase());
        }
    }
    words.into_iter().filter(|w| w.len() > 1).collect()
}

/// Extract common function name prefixes (first segment before underscore).
fn extract_function_prefixes(children: &[&&keel_core::types::GraphNode]) -> Vec<String> {
    let mut prefix_counts: HashMap<String, u32> = HashMap::new();
    for n in children {
        if n.kind != NodeKind::Function {
            continue;
        }
        if let Some(prefix) = n.name.split('_').next() {
            if prefix.len() > 1 {
                *prefix_counts.entry(prefix.to_lowercase()).or_default() += 1;
            }
        }
    }
    // Keep prefixes that appear in at least 2 functions
    let mut prefixes: Vec<String> = prefix_counts
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(prefix, _)| prefix)
        .collect();
    prefixes.sort();
    prefixes
}
