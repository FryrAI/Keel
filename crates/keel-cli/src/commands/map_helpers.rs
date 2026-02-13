use std::collections::{HashMap, HashSet};
use std::path::Path;

use keel_core::types::{EdgeChange, EdgeKind, NodeChange, NodeKind};

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
    let total_edges = valid_edges.iter().filter(|e| matches!(e, EdgeChange::Add(_))).count() as u32;
    let modules_count = nodes.iter().filter(|n| n.kind == NodeKind::Module).count() as u32;
    let functions_count = nodes.iter().filter(|n| n.kind == NodeKind::Function).count() as u32;
    let classes_count = nodes.iter().filter(|n| n.kind == NodeKind::Class).count() as u32;

    let non_module_nodes: Vec<_> = nodes.iter().filter(|n| n.kind != NodeKind::Module).collect();
    let type_hint_count = non_module_nodes.iter().filter(|n| n.type_hints_present).count();
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

    let external_endpoint_count = nodes.iter().map(|n| n.external_endpoints.len()).sum::<usize>() as u32;

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

        module_entries.push(ModuleEntry {
            path: file_path.clone(),
            function_count: fn_count,
            class_count: cls_count,
            edge_count,
            responsibility_keywords: None,
            external_endpoints: None,
        });
    }

    MapResult {
        version: "0.1.0".to_string(),
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
