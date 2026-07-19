//! Map-graph assembly shared by `keel map`, `keel map --cached`, and the MCP
//! server's `keel/map` handler.
//!
//! These functions turn already-collected node/edge changes (from a fresh parse
//! or a `GraphStore` read) into the [`crate::types::MapResult`] every interface
//! serializes. Keeping the assembly here — rather than re-derived per call site
//! — is what guarantees the CLI and MCP server report identical summary counts,
//! hotspots, and module profiles for the same graph.

use std::collections::{HashMap, HashSet};

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{
    EdgeChange, EdgeDirection, EdgeKind, GraphNode, ModuleProfile, NodeChange, NodeKind,
};

use crate::types::{
    FunctionEntry, HotspotEntry, MapResult, MapSummary, ModuleEntry, ModuleFunctionRef,
};

/// Count incoming (caller) and outgoing (callee) `Calls` edges per node id.
///
/// Returns `(callers_by_id, callees_by_id)`. This one pass replaces the three
/// byte-identical counting loops that map assembly used to run over the same
/// edge set (summary refs, hotspots, function entries).
fn call_counts(valid_edges: &[EdgeChange]) -> (HashMap<u64, u32>, HashMap<u64, u32>) {
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
    (callers, callees)
}

/// Group non-module nodes by their owning module id, in a single pass.
///
/// Lets the per-module assembly look children up by id instead of rescanning
/// the whole node list for every module (was O(modules × nodes)).
fn group_nodes_by_module<'a>(nodes: &[&'a GraphNode]) -> HashMap<u64, Vec<&'a GraphNode>> {
    let mut by_module: HashMap<u64, Vec<&GraphNode>> = HashMap::new();
    for n in nodes {
        if n.kind != NodeKind::Module {
            by_module.entry(n.module_id).or_default().push(n);
        }
    }
    by_module
}

/// Build a MapResult from collected node and edge data (before they are consumed).
pub fn build_map_result(
    node_changes: &[NodeChange],
    valid_edges: &[EdgeChange],
    entries: &[keel_parsers::walker::WalkEntry],
) -> MapResult {
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

    // Build caller/callee count maps for function refs (one pass over edges).
    let (callers_map, callees_map) = call_counts(valid_edges);

    // One-pass grouping so per-module work is a lookup, not a full rescan.
    let nodes_by_module = group_nodes_by_module(&nodes);
    let mut edges_by_file: HashMap<&str, u32> = HashMap::new();
    for e in valid_edges {
        if let EdgeChange::Add(edge) = e {
            *edges_by_file.entry(edge.file_path.as_str()).or_default() += 1;
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

        let empty: Vec<&GraphNode> = Vec::new();
        let children = nodes_by_module.get(&module_id).unwrap_or(&empty);
        let fn_count = children
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .count() as u32;
        let cls_count = children
            .iter()
            .filter(|n| n.kind == NodeKind::Class)
            .count() as u32;
        let edge_count = edges_by_file.get(file_path.as_str()).copied().unwrap_or(0);

        // Collect function names + hashes for this module
        let fn_refs: Vec<ModuleFunctionRef> = children
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .map(|n| ModuleFunctionRef {
                name: n.name.clone(),
                hash: n.hash.clone(),
                callers: callers_map.get(&n.id).copied().unwrap_or(0),
                callees: callees_map.get(&n.id).copied().unwrap_or(0),
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

/// Populate hotspot entries by ranking non-module nodes by total connectivity.
///
/// Test files are excluded before ranking: test suites routinely call
/// everything under test, so their functions rack up huge caller counts that
/// crowd out genuine hotspots (e.g. a test-helper file outranking the actual
/// most-connected production code) without telling the agent anything useful
/// about the codebase's real structure.
pub fn populate_hotspots(
    result: &mut MapResult,
    node_changes: &[NodeChange],
    valid_edges: &[EdgeChange],
) {
    use crate::violations_util::is_test_file;

    let nodes: Vec<_> = node_changes
        .iter()
        .filter_map(|c| match c {
            NodeChange::Add(n) if n.kind != NodeKind::Module && !is_test_file(&n.file_path) => {
                Some(n)
            }
            _ => None,
        })
        .collect();

    // Count incoming (callers) and outgoing (callees) Calls edges per node.
    let (callers, callees) = call_counts(valid_edges);

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
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));

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
    result: &mut MapResult,
    node_changes: &[NodeChange],
    valid_edges: &[EdgeChange],
) {
    let functions: Vec<_> = node_changes
        .iter()
        .filter_map(|c| match c {
            NodeChange::Add(n) if n.kind == NodeKind::Function => Some(n),
            _ => None,
        })
        .collect();

    // Count callers/callees per function.
    let (callers, callees) = call_counts(valid_edges);

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

    // One-pass grouping: module_id -> its non-module children.
    let nodes_by_module = group_nodes_by_module(&nodes);
    let empty: Vec<&GraphNode> = Vec::new();

    modules
        .iter()
        .map(|m| {
            let module_id = m.id;
            let children: &[&GraphNode] = nodes_by_module.get(&module_id).unwrap_or(&empty);

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

            // Keywords from function/class names (single use — inlined).
            let name_keywords = children.iter().flat_map(|n| split_identifier(&n.name));

            // Combine and deduplicate
            let mut keywords: Vec<String> = path_keywords;
            keywords.extend(name_keywords);
            keywords.sort();
            keywords.dedup();
            keywords.truncate(20); // Cap at 20 keywords

            // Extract function name prefixes
            let prefixes = extract_function_prefixes(children);

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

/// Reconstruct a full [`MapResult`] by reading an entire graph out of a
/// [`GraphStore`], using only the frozen trait's read methods
/// (`get_all_modules` + `get_nodes_in_file` + per-node `get_edges`).
///
/// This is the single assembly path behind both `keel map --cached` and the
/// MCP server's `keel/map` handler, so a graph read either way produces the
/// same summary counts, hotspots, and module profiles as a fresh `keel map`.
/// Nodes and edges are deduplicated by id: `get_all_modules` and
/// `get_nodes_in_file` can surface the same node, and any node visited twice
/// would otherwise multiply the reconstructed counts.
pub fn build_map_from_store(store: &dyn GraphStore, depth: u32) -> MapResult {
    let (node_changes, edge_changes) = collect_graph_dyn(store);
    assemble_map_from_changes(node_changes, edge_changes, depth)
}

/// Fast-path reconstruction from a concrete [`SqliteGraphStore`].
///
/// Reads the whole graph in two bulk queries (`all_nodes` + `all_edges`)
/// instead of the trait-only path's N `get_nodes_in_file` + M `get_edges`
/// round-trips, then feeds the identical shared assembly. Used by the MCP
/// `keel/map` handler, which already holds the concrete store.
pub fn build_map_from_sqlite(store: &SqliteGraphStore, depth: u32) -> MapResult {
    let node_changes = store.all_nodes().into_iter().map(NodeChange::Add).collect();
    let edge_changes = store.all_edges().into_iter().map(EdgeChange::Add).collect();
    assemble_map_from_changes(node_changes, edge_changes, depth)
}

/// Collect the whole graph through the frozen trait's read methods, using only
/// `get_all_modules` + `get_nodes_in_file` + per-node `get_edges`.
///
/// Nodes and edges are deduplicated by id: `get_all_modules` and
/// `get_nodes_in_file` can surface the same node, and any node visited twice
/// would otherwise multiply the reconstructed counts.
fn collect_graph_dyn(store: &dyn GraphStore) -> (Vec<NodeChange>, Vec<EdgeChange>) {
    let modules = store.get_all_modules();

    let mut node_changes: Vec<NodeChange> = Vec::new();
    let mut seen_node_ids: HashSet<u64> = HashSet::new();
    let mut edge_set: HashSet<u64> = HashSet::new();
    let mut edge_changes: Vec<EdgeChange> = Vec::new();

    for module in &modules {
        if seen_node_ids.insert(module.id) {
            node_changes.push(NodeChange::Add(module.clone()));
        }

        let file_nodes = store.get_nodes_in_file(&module.file_path);
        for node in &file_nodes {
            if node.kind != NodeKind::Module && seen_node_ids.insert(node.id) {
                node_changes.push(NodeChange::Add(node.clone()));
            }
            // Collect edges for this node (deduplicated by edge id), fetched
            // unconditionally so edges sourced from/targeting a repeated node
            // are still found.
            for edge in store.get_edges(node.id, EdgeDirection::Both) {
                if edge_set.insert(edge.id) {
                    edge_changes.push(EdgeChange::Add(edge));
                }
            }
        }
    }

    (node_changes, edge_changes)
}

/// Shared assembly behind both `build_map_from_store` and
/// `build_map_from_sqlite`, so a graph read either way produces the same
/// summary counts, hotspots, and module profiles as a fresh `keel map`.
fn assemble_map_from_changes(
    node_changes: Vec<NodeChange>,
    edge_changes: Vec<EdgeChange>,
    depth: u32,
) -> MapResult {
    // Languages are not available from `WalkEntry` here; reconstruct them from
    // module file extensions using the canonical detection table so store-read
    // output matches a fresh `keel map` (which reports the walker's raw
    // language strings, e.g. "svelte").
    let entries: Vec<keel_parsers::walker::WalkEntry> = Vec::new();
    let mut map_result = build_map_result(&node_changes, &edge_changes, &entries);
    map_result.depth = depth;

    let mut languages: HashSet<String> = HashSet::new();
    for change in &node_changes {
        if let NodeChange::Add(node) = change {
            if node.kind == NodeKind::Module {
                let path = std::path::Path::new(&node.file_path);
                if let Some(lang) = keel_parsers::treesitter::detect_language(path) {
                    languages.insert(lang.to_string());
                }
            }
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

    map_result
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
fn extract_function_prefixes(children: &[&GraphNode]) -> Vec<String> {
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
