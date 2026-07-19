//! Incremental graph sync for `keel compile`.
//!
//! `keel map` rebuilds the whole graph; between maps the edge graph goes stale,
//! which is exactly when E001 broken-caller checks need it. This module keeps
//! the graph fresh for the files a compile touched:
//!
//! - insert nodes for definitions new since the last map,
//! - remove nodes for definitions that vanished from a file,
//! - re-resolve each compiled file's outgoing call edges (prune + re-add).
//!
//! Re-resolution runs the SAME ladder as the map's second pass
//! ([`super::call_resolve::resolve_call_reference`]), backed here by
//! graph-backed lookups ([`GraphIndex`]) instead of the map's in-memory
//! indices. That is the whole point: an earlier version re-implemented a weaker
//! ladder and, because it prunes a file's call edges before re-resolving, every
//! `keel compile` deleted the method-call, same-directory, package, and BAML
//! edges the map had built. Running the map's own ladder makes the prune
//! lossless.
//!
//! It runs *after* enforcement (on a separate store handle) so E001/E004 still
//! diff against the pre-edit graph; the refreshed edges are then in place for
//! the next compile. Only the compiled files' edges are re-resolved, keeping
//! the single-file path well within its latency budget.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use keel_core::hash::compute_hash;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind};
use keel_parsers::resolver::{Definition, FileIndex, ReferenceKind};
use keel_parsers::treesitter::detect_language;

use super::call_resolve::{resolve_call_reference, CallSiteCtx};
use super::map_lang_resolve::ResolverSet;
use super::map_resolve::CallIndex;

/// The parts of the graph-backed [`CallIndex`] that do not depend on which file
/// is being resolved — built once per compile so a multi-file compile does not
/// re-query the module list / package index / BAML surface per file.
struct GraphIndexBase<'a> {
    store: &'a SqliteGraphStore,
    /// Every `relative_file_path -> module_id` in the graph (for import paths).
    module_files: HashMap<String, u64>,
    /// `package -> (symbol -> node id)`; empty unless the repo sets packages.
    package_node_index: HashMap<String, HashMap<String, u64>>,
    /// BAML boundary `function name -> node id`; empty unless `baml_src` exists.
    baml_fn_index: HashMap<String, u64>,
}

impl<'a> GraphIndexBase<'a> {
    fn build(store: &'a SqliteGraphStore, cwd: &Path) -> Self {
        let module_files = store
            .get_all_modules()
            .into_iter()
            .map(|m| (m.file_path, m.id))
            .collect();
        let package_node_index = store.package_node_index();
        // Only pay for the BAML lookup when the repo actually has a baml_src.
        let baml_fn_index = if cwd.join("baml_src").exists() {
            store.baml_function_nodes()
        } else {
            HashMap::new()
        };
        Self {
            store,
            module_files,
            package_node_index,
            baml_fn_index,
        }
    }
}

/// Per-file [`CallIndex`] over the persisted graph: the shared base plus the
/// file's freshly-parsed local definitions.
struct GraphIndex<'a> {
    base: &'a GraphIndexBase<'a>,
    /// This file's `name -> node id` (the ids assigned/looked up this compile).
    local: &'a HashMap<String, u64>,
    /// The file these locals belong to, repo-relative.
    file_path: &'a str,
    /// `(file_path, name) -> id` for same-file method resolution.
    name_to_id: HashMap<(String, String), u64>,
}

impl<'a> GraphIndex<'a> {
    fn new(
        base: &'a GraphIndexBase<'a>,
        local: &'a HashMap<String, u64>,
        file_path: &'a str,
    ) -> Self {
        let name_to_id = local
            .iter()
            .map(|(name, id)| ((file_path.to_string(), name.clone()), *id))
            .collect();
        Self {
            base,
            local,
            file_path,
            name_to_id,
        }
    }
}

impl CallIndex for GraphIndex<'_> {
    fn candidates(&self, name: &str) -> Vec<(String, u64)> {
        let mut out: Vec<(String, u64)> = self
            .base
            .store
            .find_nodes_by_name(name, "", "")
            .into_iter()
            .filter(|n| n.kind != NodeKind::Module)
            .map(|n| (n.file_path, n.id))
            .collect();
        // The current file's freshly-known def wins over any stale graph row for
        // the same (file, name): this compile's node insert has not committed
        // yet, so `local` holds the id the edge should point at.
        if let Some(&id) = self.local.get(name) {
            out.retain(|(f, _)| f != self.file_path);
            out.push((self.file_path.to_string(), id));
        }
        out
    }
    fn module_files(&self) -> &HashMap<String, u64> {
        &self.base.module_files
    }
    fn name_to_id(&self) -> &HashMap<(String, String), u64> {
        &self.name_to_id
    }
    fn package_index(&self) -> &HashMap<String, HashMap<String, u64>> {
        &self.base.package_node_index
    }
    fn baml_index(&self) -> &HashMap<String, u64> {
        &self.base.baml_fn_index
    }
}

/// Refresh the graph for the compiled files. Best-effort: a failure is logged
/// (when `verbose`) and never blocks the compile result.
pub fn sync_compiled_files(
    store: &mut SqliteGraphStore,
    cwd: &Path,
    files: &[FileIndex],
    resolvers: &ResolverSet,
    verbose: bool,
) {
    let mut next_id = store.max_id() + 1;
    let mut node_changes: Vec<NodeChange> = Vec::new();
    let mut edge_changes: Vec<EdgeChange> = Vec::new();
    let mut node_tiers: HashMap<u64, String> = HashMap::new();
    // Hashes assigned to NEW nodes earlier in this same batch (mirrors the map
    // pass's assigned-hash set): two identical new defs in one multi-file
    // compile must not share a hash, or the second silently overwrites the
    // first's row and its edges dangle.
    let mut assigned_hashes: HashSet<String> = HashSet::new();
    // Edge prunes are deferred to the write phase: pruning during the read
    // phase is a committed DELETE, so a later node-write failure would leave
    // the file's outgoing edges destroyed with nothing re-added.
    let mut files_to_prune: Vec<String> = Vec::new();

    // Files in this compile batch: a vanished definition with callers only in
    // these files was fixed in the same pass, so its node may be pruned. One
    // with callers OUTSIDE the batch is a live broken contract — keep the node
    // so callee-side E004 re-fires until those callers are fixed (below).
    let batch_files: HashSet<&str> = files.iter().map(|f| f.file_path.as_str()).collect();

    // Read phase: build the graph-backed index once, then resolve every
    // compiled file against it, collecting node/edge changes. Scoped so the
    // immutable borrow of `store` ends before the mutable write phase below.
    {
        let base = GraphIndexBase::build(&*store, cwd);
        for file in files {
            sync_one_file(
                &base,
                cwd,
                file,
                resolvers,
                &batch_files,
                &mut next_id,
                &mut node_changes,
                &mut edge_changes,
                &mut node_tiers,
                &mut assigned_hashes,
                &mut files_to_prune,
            );
        }
    }

    // Write phase (same handle, now mutable). Modules/definitions land first
    // (module_id + Contains FK) — `update_nodes` sorts, but node inserts must
    // precede edges.
    if !node_changes.is_empty() {
        if let Err(e) = store.update_nodes(node_changes) {
            if verbose {
                eprintln!("keel compile: graph sync (nodes) failed: {}", e);
            }
            return;
        }
    }
    for rel_path in &files_to_prune {
        if let Err(e) = store.prune_call_edges_from_file(rel_path) {
            let _ = e; // best-effort; fresh edges for this file are added next
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
    base: &GraphIndexBase,
    cwd: &Path,
    file: &FileIndex,
    resolvers: &ResolverSet,
    batch_files: &HashSet<&str>,
    next_id: &mut u64,
    node_changes: &mut Vec<NodeChange>,
    edge_changes: &mut Vec<EdgeChange>,
    node_tiers: &mut HashMap<u64, String>,
    assigned_hashes: &mut HashSet<String>,
    files_to_prune: &mut Vec<String>,
) {
    let store = base.store;
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
        let mut hash = node_hash_for(store, def, rel_path);
        if !assigned_hashes.insert(hash.clone()) {
            // Same-batch collision: an identical new def in another compiled
            // file already claimed this hash; disambiguate by file path.
            hash = def.hash_disambiguated(rel_path);
            assigned_hashes.insert(hash.clone());
        }
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

    // Re-resolve this file's outgoing call edges: the stale set is pruned in
    // the WRITE phase (after node writes succeed), then the fresh ones below
    // are added — so a failed node write can never leave the file edge-less.
    files_to_prune.push(rel_path.clone());
    resolve_outgoing_edges(
        base,
        cwd,
        file,
        resolvers,
        &local,
        next_id,
        edge_changes,
        node_tiers,
    );
}

/// Resolve the compiled file's call references into `calls` edges, running the
/// shared map/compile ladder so the re-added edges match map quality.
#[allow(clippy::too_many_arguments)]
fn resolve_outgoing_edges(
    base: &GraphIndexBase,
    cwd: &Path,
    file: &FileIndex,
    resolvers: &ResolverSet,
    local: &HashMap<String, u64>,
    next_id: &mut u64,
    edge_changes: &mut Vec<EdgeChange>,
    node_tiers: &mut HashMap<u64, String>,
) {
    let rel_path = &file.file_path;
    let language = detect_language(Path::new(rel_path)).unwrap_or("");
    let abs_file = cwd.join(rel_path);

    let idx = GraphIndex::new(base, local, rel_path);
    let ctx = CallSiteCtx {
        resolvers,
        language,
        file_path: rel_path,
        abs_file: &abs_file,
        imports: &file.imports,
        definitions: &file.definitions,
    };

    for reference in &file.references {
        if reference.kind != ReferenceKind::Call {
            continue;
        }
        // Same-file direct call: link against this file's own defs, exactly as
        // the map's first pass does, at SAME_FILE_CALL confidence. The shared
        // ladder only handles cross-file references (method calls still route
        // through it via its same-file-method step).
        if let Some(&tgt) = local.get(&reference.name) {
            push_call_edge(
                file,
                local,
                reference.line,
                tgt,
                keel_core::confidence::SAME_FILE_CALL,
                "tier1",
                next_id,
                edge_changes,
                node_tiers,
            );
            continue;
        }
        if let Some(resolved) = resolve_call_reference(&idx, &ctx, reference) {
            push_call_edge(
                file,
                local,
                reference.line,
                resolved.target_id,
                resolved.confidence,
                &resolved.tier,
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

/// Content hash for a new node, disambiguated if the base hash collides with a
/// different node already in the graph (mirrors the map first pass, which uses
/// the same normalized `Definition::hash` — raw-body hashing here would give
/// compile-created nodes a different identity than map would assign).
fn node_hash_for(store: &SqliteGraphStore, def: &Definition, rel_path: &str) -> String {
    let base = def.hash();
    if let Some(existing) = store.get_node(&base) {
        if existing.file_path != rel_path || existing.name != def.name {
            return def.hash_disambiguated(rel_path);
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

#[cfg(test)]
mod tests {
    use super::*;
    use keel_core::types::EdgeDirection;

    fn def(name: &str, file: &str) -> Definition {
        Definition {
            name: name.to_string(),
            kind: NodeKind::Function,
            signature: format!("fn {}()", name),
            file_path: file.to_string(),
            line_start: 1,
            line_end: 3,
            docstring: None,
            is_public: true,
            type_hints_present: true,
            body_text: "{ return 42; }".to_string(),
            in_test_context: false,
        }
    }

    fn index(file: &str, defs: Vec<Definition>) -> FileIndex {
        FileIndex {
            file_path: file.to_string(),
            content_hash: 1,
            definitions: defs,
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        }
    }

    /// Two files adding IDENTICAL new functions in one multi-file compile must
    /// not share a hash: without the in-batch assigned-hash guard, the second
    /// node silently overwrote the first's row and its edges dangled.
    #[test]
    fn same_batch_identical_defs_get_distinct_hashes() {
        let mut store = SqliteGraphStore::in_memory().unwrap();
        let cwd = std::env::current_dir().unwrap();
        let resolvers = ResolverSet {
            ts: None,
            py: None,
            go: None,
            rs: None,
        };

        let files = vec![
            index("src/a.rs", vec![def("twin", "src/a.rs")]),
            index("src/b.rs", vec![def("twin", "src/b.rs")]),
        ];
        sync_compiled_files(&mut store, &cwd, &files, &resolvers, false);

        let a_nodes: Vec<_> = store
            .get_nodes_in_file("src/a.rs")
            .into_iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        let b_nodes: Vec<_> = store
            .get_nodes_in_file("src/b.rs")
            .into_iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        assert_eq!(a_nodes.len(), 1, "first file's node must survive");
        assert_eq!(b_nodes.len(), 1, "second file's node must be inserted");
        assert_ne!(
            a_nodes[0].hash, b_nodes[0].hash,
            "identical same-batch defs must get disambiguated hashes"
        );
        // Both files' Contains edges resolve to live nodes (nothing dangles).
        for n in a_nodes.iter().chain(b_nodes.iter()) {
            assert!(
                !store.get_edges(n.id, EdgeDirection::Incoming).is_empty(),
                "each node keeps its module Contains edge"
            );
        }
    }
}
