//! Incremental graph sync for `keel compile`.
//!
//! `keel map` rebuilds the whole graph; between maps the edge graph goes stale,
//! which is exactly when E001 broken-caller checks need it. This module keeps
//! the graph fresh for the files a compile touched:
//!
//! - insert nodes for definitions new since the last map,
//! - remove nodes for definitions that vanished from a file,
//! - re-resolve each compiled file's outgoing call edges (prune + re-add),
//!   using the same per-language `resolve_call_edge` path as the map pipeline.
//!
//! It runs *after* enforcement (on a separate store handle) so E001/E004 still
//! diff against the pre-edit graph; the refreshed edges are then in place for
//! the next compile. Only the compiled files' edges are re-resolved, keeping
//! the single-file path well within its latency budget.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use keel_core::hash::{compute_hash, compute_hash_disambiguated};
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind};
use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::{Definition, FileIndex, LanguageResolver, ReferenceKind};
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::treesitter::{detect_language, is_typescript_family};
use keel_parsers::typescript::TsResolver;

use super::map_lang_resolve::resolve_with;
use super::map_resolve::resolve_edge_to_node;

/// The (possibly warm-cached) resolvers used to parse the compiled files.
///
/// Reusing the same instances the parse loop populated means
/// `resolve_call_edge` sees the cached parse data for each caller.
pub struct SyncResolvers<'a> {
    pub ts: Option<&'a TsResolver>,
    pub py: Option<&'a PyResolver>,
    pub go: Option<&'a GoResolver>,
    pub rs: Option<&'a RustLangResolver>,
}

impl<'a> SyncResolvers<'a> {
    fn for_language(&self, language: &str) -> Option<&'a dyn LanguageResolver> {
        match language {
            l if is_typescript_family(l) => self.ts.map(|r| r as &dyn LanguageResolver),
            "python" => self.py.map(|r| r as &dyn LanguageResolver),
            "go" => self.go.map(|r| r as &dyn LanguageResolver),
            "rust" => self.rs.map(|r| r as &dyn LanguageResolver),
            _ => None,
        }
    }
}

/// Refresh the graph for the compiled files. Best-effort: a failure is logged
/// (when `verbose`) and never blocks the compile result.
pub fn sync_compiled_files(
    store: &mut SqliteGraphStore,
    cwd: &Path,
    files: &[FileIndex],
    resolvers: &SyncResolvers,
    verbose: bool,
) {
    let mut next_id = store.max_id() + 1;
    let mut node_changes: Vec<NodeChange> = Vec::new();
    let mut edge_changes: Vec<EdgeChange> = Vec::new();
    let mut node_tiers: HashMap<u64, String> = HashMap::new();

    // Files in this compile batch: a vanished definition with callers only in
    // these files was fixed in the same pass, so its node may be pruned. One
    // with callers OUTSIDE the batch is a live broken contract — keep the node
    // so callee-side E004 re-fires until those callers are fixed (below).
    let batch_files: HashSet<&str> = files.iter().map(|f| f.file_path.as_str()).collect();

    for file in files {
        sync_one_file(
            store,
            cwd,
            file,
            resolvers,
            &batch_files,
            &mut next_id,
            &mut node_changes,
            &mut edge_changes,
            &mut node_tiers,
        );
    }

    // Modules/definitions first (module_id + Contains FK), then removals last —
    // `update_nodes` already sorts, but node inserts must land before edges.
    if !node_changes.is_empty() {
        if let Err(e) = store.update_nodes(node_changes) {
            if verbose {
                eprintln!("keel compile: graph sync (nodes) failed: {}", e);
            }
            return;
        }
    }
    if !edge_changes.is_empty() {
        if let Err(e) = store.update_edges(edge_changes) {
            if verbose {
                eprintln!("keel compile: graph sync (edges) failed: {}", e);
            }
        }
    }
    if let Err(e) = store.set_node_resolution_tiers(&node_tiers) {
        if verbose {
            eprintln!("keel compile: graph sync (tiers) failed: {}", e);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_one_file(
    store: &SqliteGraphStore,
    cwd: &Path,
    file: &FileIndex,
    resolvers: &SyncResolvers,
    batch_files: &HashSet<&str>,
    next_id: &mut u64,
    node_changes: &mut Vec<NodeChange>,
    edge_changes: &mut Vec<EdgeChange>,
    node_tiers: &mut HashMap<u64, String>,
) {
    let rel_path = &file.file_path;
    let existing = store.get_nodes_in_file(rel_path);

    // Resolve (or create) the module node for this file.
    let (module_id, mut created_module) = match existing.iter().find(|n| n.kind == NodeKind::Module)
    {
        Some(m) => (m.id, false),
        None => {
            let id = *next_id;
            *next_id += 1;
            (id, true)
        }
    };

    // name -> node id for the definitions currently in the graph for this file.
    let mut local: HashMap<String, u64> = existing
        .iter()
        .filter(|n| n.kind != NodeKind::Module)
        .map(|n| (n.name.clone(), n.id))
        .collect();
    let current_names: HashSet<&str> = file.definitions.iter().map(|d| d.name.as_str()).collect();

    // Insert nodes for definitions new since the last graph write.
    for def in &file.definitions {
        if local.contains_key(&def.name) {
            continue;
        }
        if created_module {
            node_changes.push(NodeChange::Add(module_node(module_id, rel_path, file)));
            created_module = false;
        }
        let id = *next_id;
        *next_id += 1;
        let hash = node_hash_for(store, def, rel_path);
        node_changes.push(NodeChange::Add(definition_node(
            id, hash, def, rel_path, module_id,
        )));
        // "contains" edge module -> definition, mirroring the map first pass.
        let edge_id = *next_id;
        *next_id += 1;
        edge_changes.push(EdgeChange::Add(GraphEdge {
            id: edge_id,
            source_id: module_id,
            target_id: id,
            kind: EdgeKind::Contains,
            file_path: rel_path.clone(),
            line: def.line_start,
            confidence: 1.0,
        }));
        local.insert(def.name.clone(), id);
    }

    // Remove nodes for definitions that vanished from the file. E004 already
    // ran against the pre-sync graph, so pruning here does not hide removals.
    // Exception: keep a vanished node that still has live callers OUTSIDE this
    // batch — removing it (and, via FK cascade, its caller edges) would erase
    // the broken contract, so the next compile of this file would see nothing
    // to remove and E004 would stop re-firing while real callers stay broken.
    for node in existing
        .iter()
        .filter(|n| n.kind != NodeKind::Module && !current_names.contains(n.name.as_str()))
    {
        if has_live_external_callers(store, node.id, batch_files) {
            continue;
        }
        node_changes.push(NodeChange::Remove(node.id));
        local.remove(&node.name);
    }

    // Re-resolve this file's outgoing call edges: prune the stale set, then add
    // freshly-resolved ones so E001 (a later compile) sees current callers.
    if let Err(e) = store.prune_call_edges_from_file(rel_path) {
        let _ = e; // best-effort; stale edges are re-added below regardless
    }
    resolve_outgoing_edges(
        store,
        cwd,
        file,
        resolvers,
        &local,
        next_id,
        edge_changes,
        node_tiers,
    );
}

/// Resolve the compiled file's call references into `calls` edges.
#[allow(clippy::too_many_arguments)]
fn resolve_outgoing_edges(
    store: &SqliteGraphStore,
    cwd: &Path,
    file: &FileIndex,
    resolvers: &SyncResolvers,
    local: &HashMap<String, u64>,
    next_id: &mut u64,
    edge_changes: &mut Vec<EdgeChange>,
    node_tiers: &mut HashMap<u64, String>,
) {
    let rel_path = &file.file_path;
    let language = detect_language(Path::new(rel_path)).unwrap_or("");
    let resolver = resolvers.for_language(language);
    let abs_file = cwd.join(rel_path);

    for reference in &file.references {
        if reference.kind != ReferenceKind::Call {
            continue;
        }
        // Same-file call: resolve directly against this file's definitions.
        if let Some(&tgt) = local.get(&reference.name) {
            push_call_edge(
                file,
                local,
                reference.line,
                tgt,
                0.95,
                "tier1",
                next_id,
                edge_changes,
                node_tiers,
            );
            continue;
        }

        // Tier 2: the language resolver, then a graph-name fallback.
        let mut resolved: Option<(u64, f64, String)> = resolver
            .and_then(|r| resolve_with(r, &abs_file, reference))
            .and_then(|edge| {
                target_node_in_graph(store, local, &edge.target_file, &edge.target_name)
                    .map(|id| (id, edge.confidence, edge.resolution_tier))
            });

        if resolved.is_none() {
            // Fallback heuristic: the bare callee name, matched in the graph.
            let bare = reference
                .name
                .rsplit(['.', ':'])
                .next()
                .unwrap_or(&reference.name);
            resolved = graph_lookup_by_name(store, bare).map(|id| (id, 0.80, "tier1".to_string()));
        }

        if let Some((tgt, confidence, tier)) = resolved {
            push_call_edge(
                file,
                local,
                reference.line,
                tgt,
                confidence,
                &tier,
                next_id,
                edge_changes,
                node_tiers,
            );
        }
    }
}

/// Find the caller definition containing `line` and emit a `calls` edge from it.
#[allow(clippy::too_many_arguments)]
fn push_call_edge(
    file: &FileIndex,
    local: &HashMap<String, u64>,
    line: u32,
    target_id: u64,
    confidence: f64,
    tier: &str,
    next_id: &mut u64,
    edge_changes: &mut Vec<EdgeChange>,
    node_tiers: &mut HashMap<u64, String>,
) {
    let Some(source_id) = containing_def(file, local, line) else {
        return;
    };
    if source_id == target_id {
        return;
    }
    let edge_id = *next_id;
    *next_id += 1;
    edge_changes.push(EdgeChange::Add(GraphEdge {
        id: edge_id,
        source_id,
        target_id,
        kind: EdgeKind::Calls,
        file_path: file.file_path.clone(),
        line,
        confidence,
    }));
    node_tiers
        .entry(source_id)
        .or_insert_with(|| tier.to_string());
}

/// True if `node_id` still has incoming `calls` edges from source nodes whose
/// file is NOT in the current compile batch — i.e. a real, still-broken caller
/// that this pass did not touch. Used to keep a removed function's node alive so
/// callee-side E004 keeps firing until those callers are fixed.
fn has_live_external_callers(
    store: &SqliteGraphStore,
    node_id: u64,
    batch_files: &HashSet<&str>,
) -> bool {
    use keel_core::types::EdgeDirection;
    store
        .get_edges(node_id, EdgeDirection::Incoming)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .filter_map(|e| store.get_node_by_id(e.source_id))
        .any(|caller| !batch_files.contains(caller.file_path.as_str()))
}

/// Find which definition in `file` contains `line`, returning its node id.
fn containing_def(file: &FileIndex, local: &HashMap<String, u64>, line: u32) -> Option<u64> {
    file.definitions
        .iter()
        .find(|d| line >= d.line_start && line <= d.line_end)
        .and_then(|d| local.get(&d.name).copied())
}

/// Map a resolver's `ResolvedEdge` target to a node id, checking this file's
/// own definitions first (same-file targets) then the persisted graph.
fn target_node_in_graph(
    store: &SqliteGraphStore,
    local: &HashMap<String, u64>,
    target_file: &str,
    target_name: &str,
) -> Option<u64> {
    if let Some(&id) = local.get(target_name) {
        return Some(id);
    }
    // Reuse the map's index-based matcher over the graph's candidates.
    let candidates = store.find_nodes_by_name(target_name, "", "");
    let index: HashMap<String, Vec<(String, u64)>> = {
        let mut m: HashMap<String, Vec<(String, u64)>> = HashMap::new();
        for n in &candidates {
            m.entry(n.name.clone())
                .or_default()
                .push((n.file_path.clone(), n.id));
        }
        m
    };
    resolve_edge_to_node(&index, target_file, target_name)
}

/// Fallback: resolve a bare callee name to a single graph node, if unambiguous.
fn graph_lookup_by_name(store: &SqliteGraphStore, name: &str) -> Option<u64> {
    let mut candidates: Vec<GraphNode> = store
        .find_nodes_by_name(name, "", "")
        .into_iter()
        .filter(|n| n.kind != NodeKind::Module)
        .collect();
    match candidates.len() {
        1 => Some(candidates.remove(0).id),
        _ => None,
    }
}

/// Content hash for a new node, disambiguated if the base hash collides with a
/// different node already in the graph (mirrors the map first pass).
fn node_hash_for(store: &SqliteGraphStore, def: &Definition, rel_path: &str) -> String {
    let doc = def.docstring.as_deref().unwrap_or("");
    let base = compute_hash(&def.signature, &def.body_text, doc);
    if let Some(existing) = store.get_node(&base) {
        if existing.file_path != rel_path || existing.name != def.name {
            return compute_hash_disambiguated(&def.signature, &def.body_text, doc, rel_path);
        }
    }
    base
}

/// Build a module `GraphNode` for a file first seen at compile time.
fn module_node(id: u64, rel_path: &str, file: &FileIndex) -> GraphNode {
    let line_end = file
        .definitions
        .iter()
        .map(|d| d.line_end)
        .max()
        .unwrap_or(1);
    GraphNode {
        id,
        hash: compute_hash(rel_path, "", ""),
        kind: NodeKind::Module,
        name: rel_path.to_string(),
        signature: String::new(),
        file_path: rel_path.to_string(),
        line_start: 1,
        line_end,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

/// Build a definition `GraphNode` from a parsed definition.
fn definition_node(
    id: u64,
    hash: String,
    def: &Definition,
    rel_path: &str,
    module_id: u64,
) -> GraphNode {
    GraphNode {
        id,
        hash,
        kind: def.kind.clone(),
        name: def.name.clone(),
        signature: def.signature.clone(),
        file_path: rel_path.to_string(),
        line_start: def.line_start,
        line_end: def.line_end,
        docstring: def.docstring.clone(),
        is_public: def.is_public,
        type_hints_present: def.type_hints_present,
        has_docstring: def.docstring.is_some(),
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id,
        package: None,
    }
}
