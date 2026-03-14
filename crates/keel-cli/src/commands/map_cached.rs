//! Cached map: read from existing graph.db without re-parsing.
//! Used by `keel map --cached` for fast session-start hooks.

use std::collections::HashSet;

use keel_core::store::GraphStore;
use keel_core::types::NodeKind;
use keel_output::OutputFormatter;

use super::map_helpers::{build_map_result, populate_functions, populate_hotspots};
use crate::telemetry_recorder::EventMetrics;

/// Read map from existing graph.db without re-parsing. Returns error if DB is empty.
pub fn run_cached(
    store: &dyn GraphStore,
    formatter: &dyn OutputFormatter,
    verbose: bool,
    depth: u32,
) -> (i32, EventMetrics) {
    use keel_core::types::{EdgeChange, EdgeDirection, NodeChange};

    let modules = store.get_all_modules();
    if modules.is_empty() {
        if verbose {
            eprintln!("keel map --cached: graph.db is empty, falling back to full map");
        }
        eprintln!("keel map --cached: no cached graph found. Run `keel map` first.");
        return (2, EventMetrics::default());
    }

    // Collect all nodes and edges from the DB
    let mut node_changes: Vec<NodeChange> = Vec::new();
    let mut edge_set: HashSet<u64> = HashSet::new();
    let mut edge_changes: Vec<EdgeChange> = Vec::new();

    for module in &modules {
        node_changes.push(NodeChange::Add(module.clone()));

        // Get all nodes in this module's file
        let file_nodes = store.get_nodes_in_file(&module.file_path);
        for node in &file_nodes {
            if node.kind != NodeKind::Module {
                node_changes.push(NodeChange::Add(node.clone()));
            }
            // Collect edges for this node (deduplicated by edge ID)
            let edges = store.get_edges(node.id, EdgeDirection::Both);
            for edge in edges {
                if edge_set.insert(edge.id) {
                    edge_changes.push(EdgeChange::Add(edge));
                }
            }
        }
    }

    if verbose {
        eprintln!(
            "keel map --cached: read {} nodes, {} edges from graph.db",
            node_changes.len(),
            edge_changes.len()
        );
    }

    // Build MapResult using same helpers as full map
    let entries: Vec<keel_parsers::walker::WalkEntry> = Vec::new();
    let mut map_result = build_map_result(&node_changes, &edge_changes, &entries);
    map_result.depth = depth;

    // Reconstruct language list from file extensions in module paths
    let mut languages: HashSet<String> = HashSet::new();
    for module in &modules {
        if let Some(ext) = std::path::Path::new(&module.file_path).extension() {
            let lang = match ext.to_str().unwrap_or("") {
                "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" => "typescript",
                "py" | "pyi" => "python",
                "go" => "go",
                "rs" => "rust",
                _ => continue,
            };
            languages.insert(lang.to_string());
        }
    }
    let mut langs: Vec<String> = languages.into_iter().collect();
    langs.sort();
    map_result.summary.languages = langs;

    if depth >= 1 {
        populate_hotspots(&mut map_result, &node_changes, &edge_changes);
    }
    if depth >= 2 {
        populate_functions(&mut map_result, &node_changes, &edge_changes);
    }

    let metrics = EventMetrics {
        node_count: map_result.summary.total_nodes,
        edge_count: map_result.summary.total_edges,
        ..Default::default()
    };

    let output = formatter.format_map(&map_result);
    if !output.is_empty() {
        println!("{}", output);
    }
    (0, metrics)
}
