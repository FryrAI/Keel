use std::collections::HashSet;
use std::path::Path;

use keel_core::types::{EdgeChange, NodeChange, NodeKind};

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
    }
}

/// Make a path relative to the project root.
pub fn make_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}
