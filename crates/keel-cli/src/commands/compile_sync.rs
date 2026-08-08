//! Incremental graph sync for `keel compile`.
//!
//! `keel map` rebuilds the whole graph; between maps the edge graph goes stale,
//! which is exactly when E001 broken-caller checks need it. This module keeps
//! the graph fresh for the files a compile touched:
//!
//! - insert nodes for definitions new since the last map,
//! - remove nodes for definitions that vanished from a file,
//! - re-resolve each compiled file's outgoing reference edges — `calls` plus
//!   the `uses` edges of functions named as values (replace, never wholesale
//!   prune).
//!
//! Re-resolution runs the SAME ladder as the map's second pass
//! ([`super::call_resolve::resolve_call_reference`]), backed here by
//! graph-backed lookups ([`GraphIndex`]) instead of the map's in-memory
//! indices. The ladder's *rungs* match the map's, but its *inputs* do not:
//! the map's tier-2 resolvers have parsed the whole repo, while compile-time
//! resolvers have parsed only the compiled files, so cross-file references
//! the map resolved through tier 2 routinely fail to re-resolve here. A
//! reference that fails to re-resolve therefore proves NOTHING about whether
//! its call site still exists. The sync deletes a stored edge only on
//! positive evidence: it re-resolved the same (source, target, kind) this
//! pass, or the target lives inside the compiled file itself — where the
//! ladder has complete information and replacement is at map parity. Every
//! other stored edge is kept; a genuinely deleted cross-file call site keeps
//! its edge until the next `keel map` rebuilds the graph (bounded staleness,
//! the accepted trade — an earlier version pruned a file's edges wholesale
//! and re-added what re-resolved, so ONE compile permanently erased a file's
//! tier-2/cross-crate edges, eroding E001/E004/W005/`keel discover` and
//! W009's stored-boundary baseline until the next full map).
//!
//! The keep rule deliberately covers `uses` edges too, superseding the old
//! prune's rationale for including them ("leaving `uses` behind would keep a
//! deleted value reference alive forever"): a deleted cross-file value
//! reference now keeps its edge until the next map, so W005 can under-report
//! a dead callback between maps — the mirror image of the false POSITIVES
//! that wholesale-pruning live `uses` edges produced, and the cheaper error:
//! it is bounded, and same-file `uses` edges (the common W005 case) are
//! still replaced wholesale by the intra-file rule.
//!
//! It runs *after* enforcement (on a separate store handle) so E001/E004 still
//! diff against the pre-edit graph; the refreshed edges are then in place for
//! the next compile. Only the compiled files' edges are re-resolved, keeping
//! the single-file path well within its latency budget.
//!
//! The module's second job is the mirror image: [`resolve_call_targets`] runs
//! the same ladder *before* enforcement, read-only, to populate
//! [`Reference::resolved_to`] for E005 arity checking (issue #54). Both jobs
//! share one per-reference entry point ([`resolve_reference`]) so they cannot
//! drift.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use keel_core::hash::compute_hash;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind};
use keel_parsers::boundary::BoundaryProvider;
use keel_parsers::resolver::{Definition, FileIndex, Reference, ReferenceKind};
use keel_parsers::treesitter::detect_language;

use super::call_resolve::{
    edge_for_reference, resolve_call_reference, tier_for_reference, CallSiteCtx, ResolvedCall,
};
use super::map_lang_resolve::ResolverSet;
use super::map_resolve::CallIndex;

/// The parts of the graph-backed [`CallIndex`] that do not depend on which file
/// is being resolved — built once per compile so a multi-file compile does not
/// re-query the module list / package index / boundary surface per file.
struct GraphIndexBase<'a> {
    store: &'a SqliteGraphStore,
    /// Every `relative_file_path -> module_id` in the graph (for import paths).
    module_files: HashMap<String, u64>,
    /// `package -> (symbol -> node id)`; empty unless the repo sets packages.
    package_node_index: HashMap<String, HashMap<String, u64>>,
    /// Boundary `function name -> (node id, confidence)` (e.g. BAML); empty
    /// unless `baml_src` exists. The confidence is the boundary provider's own
    /// tier, carried per entry so compile-sync re-resolution matches map quality
    /// (see [`super::map_resolve::CallIndex::boundary_index`]).
    boundary_index: HashMap<String, (u64, f64)>,
}

impl<'a> GraphIndexBase<'a> {
    fn build(store: &'a SqliteGraphStore, cwd: &Path) -> Self {
        let module_files = store
            .get_all_modules()
            .into_iter()
            .map(|m| (m.file_path, m.id))
            .collect();
        let package_node_index = store.package_node_index();
        // Only pay for the boundary lookup when the repo actually has a
        // baml_src. `BamlProvider::confidence()` is a stateless constant lookup
        // (no scan), so the tier stays sourced from the provider, not a literal,
        // and is stapled onto each entry to mirror the map's per-entry index.
        let boundary_index = if cwd.join("baml_src").exists() {
            let confidence = keel_parsers::boundary::BamlProvider.confidence();
            store
                .baml_function_nodes()
                .into_iter()
                .map(|(name, id)| (name, (id, confidence)))
                .collect()
        } else {
            HashMap::new()
        };
        Self {
            store,
            module_files,
            package_node_index,
            boundary_index,
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
    /// Memoized `candidates()` results for this file's resolution pass.
    ///
    /// The ladder asks for the same name up to three times per unresolved
    /// reference, and each miss was a fresh `find_nodes_by_name` round-trip to
    /// SQLite — squarely inside `keel compile`'s <200ms budget. The candidate
    /// set cannot change mid-pass (nothing writes to the store until the sync
    /// commits), so one query per distinct name is sufficient.
    candidate_memo: RefCell<HashMap<String, Vec<(String, u64)>>>,
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
            candidate_memo: RefCell::new(HashMap::new()),
        }
    }

    /// Query the store for `name` (uncached) — see [`Self::candidates`].
    fn lookup_candidates(&self, name: &str) -> Vec<(String, u64)> {
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
}

impl CallIndex for GraphIndex<'_> {
    fn candidates(&self, name: &str) -> Cow<'_, [(String, u64)]> {
        // Owned, not borrowed: a `RefCell` borrow cannot outlive this call. The
        // memo still removes what actually costs — the SQLite round-trip. What
        // remains is a clone of a list that is empty or a handful of entries for
        // essentially every name.
        if let Some(hit) = self.candidate_memo.borrow().get(name) {
            return Cow::Owned(hit.clone());
        }
        let fresh = self.lookup_candidates(name);
        self.candidate_memo
            .borrow_mut()
            .insert(name.to_string(), fresh.clone());
        Cow::Owned(fresh)
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
    fn boundary_index(&self) -> &HashMap<String, (u64, f64)> {
        &self.base.boundary_index
    }
}

/// Resolve each compiled file's call references to their target node's hash
/// *before* enforcement runs, populating [`Reference::resolved_to`] — the
/// field E005 arity checking keys on (issue #54: nothing ever set it, so E005
/// was unreachable).
///
/// Runs the same ladder as the post-enforcement edge sync below (via the
/// shared [`resolve_reference`]), read-only against the pre-edit graph, plus
/// two gates of its own that the edge sync does not apply. Only `Call`
/// references that carry a syntactic argument count are resolved (they are
/// the only ones E005 can judge), and only resolutions at error-tier
/// confidence stick: a heuristic match (unknown receiver, cross-package name,
/// boundary surface) must never escalate into an arity ERROR against what may
/// be the wrong function.
pub fn resolve_call_targets(
    store: &SqliteGraphStore,
    cwd: &Path,
    files: &mut [FileIndex],
    resolvers: &ResolverSet,
) {
    let countable_call = |r: &Reference| r.kind == ReferenceKind::Call && r.call_arity.is_some();
    if !files.iter().flat_map(|f| &f.references).any(countable_call) {
        return; // nothing E005 could judge — skip the index build entirely
    }
    let base = GraphIndexBase::build(store, cwd);
    // id -> hash memo across the whole pass: repeated calls to one target must
    // not re-run the store's full node load per call site.
    let mut hash_memo: HashMap<u64, Option<String>> = HashMap::new();
    for file in files.iter_mut() {
        let FileIndex {
            file_path,
            references,
            imports,
            definitions,
            ..
        } = file;
        if !references.iter().any(countable_call) {
            continue;
        }
        let language = detect_language(Path::new(file_path.as_str())).unwrap_or("");
        let abs_file = cwd.join(file_path.as_str());
        // Bare-name -> node id for this file's stored defs, and the names that
        // appear MORE THAN ONCE. A file legally holding two same-named defs (a
        // free `search_graph` next to a `search_graph` method) collapses to
        // one map entry, so a name-keyed binding is a coin flip — ambiguity
        // must refuse to resolve rather than guess (same rule as the
        // same-directory rung). Deliberately THIS pass only, not the shared
        // `resolve_reference`: an ERROR-tier arity claim against the wrong
        // sibling is worse than no claim, but the edge sync's best-effort
        // binding still lands inside the right file/module, and pruning those
        // edges would trade a rare mis-attributed edge for fresh W005 noise.
        let mut local: HashMap<String, u64> = HashMap::new();
        let mut ambiguous: HashSet<String> = HashSet::new();
        for node in store.get_nodes_in_file(file_path) {
            if node.kind == NodeKind::Module {
                continue;
            }
            if local.insert(node.name.clone(), node.id).is_some() {
                ambiguous.insert(node.name);
            }
        }
        // The freshly-parsed definitions know duplicate names authoritatively
        // — the stored side can lag (the edge sync inserts only one node per
        // name), so a name the CURRENT file text defines twice is ambiguous
        // even when the graph holds a single sibling.
        let mut seen: HashSet<&str> = HashSet::new();
        for def in definitions.iter() {
            if !seen.insert(def.name.as_str()) {
                ambiguous.insert(def.name.clone());
            }
        }
        let idx = GraphIndex::new(&base, &local, file_path);
        let ctx = CallSiteCtx {
            resolvers,
            language,
            file_path,
            abs_file: &abs_file,
            imports,
            definitions,
        };
        for reference in references.iter_mut() {
            if !countable_call(reference) {
                continue;
            }
            let Some(resolved) = resolve_reference(&local, &idx, &ctx, reference) else {
                continue;
            };
            // Refuse only a SAME-FILE bind on an ambiguous name — that is the
            // coin flip. The callee segment covers both `search_graph(..)`
            // and `self.search_graph(..)`; a cross-file resolution of the
            // same bare name (import, package) is unaffected by this file's
            // local collision and stays eligible.
            let callee = reference
                .name
                .rsplit(['.', ':'])
                .next()
                .unwrap_or(reference.name.as_str());
            if ambiguous.contains(callee) && local.get(callee) == Some(&resolved.target_id) {
                continue;
            }
            if resolved.confidence < keel_core::confidence::ERROR_TIER_THRESHOLD {
                continue;
            }
            let hash = hash_memo
                .entry(resolved.target_id)
                .or_insert_with(|| store.get_node_by_id(resolved.target_id).map(|n| n.hash));
            reference.resolved_to.clone_from(hash);
        }
    }
}

/// The ONE per-reference resolution entry both the pre-enforcement pass above
/// and the edge sync's [`resolve_outgoing_edges`] use: a same-file direct name
/// hit binds against the file's own definitions first (at the reference
/// kind's same-file confidence, exactly as the map's first pass), everything
/// else runs the shared ladder. Two mirrored copies of this sequence is the
/// documented failure mode this module exists to prevent.
fn resolve_reference(
    local: &HashMap<String, u64>,
    idx: &GraphIndex,
    ctx: &CallSiteCtx,
    reference: &Reference,
) -> Option<ResolvedCall> {
    if let Some(&target_id) = local.get(&reference.name) {
        let (_, same_file_confidence) = edge_for_reference(&reference.kind)?;
        return Some(ResolvedCall {
            target_id,
            confidence: same_file_confidence,
            tier: tier_for_reference(&reference.kind).to_string(),
        });
    }
    resolve_call_reference(idx, ctx, reference)
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
    // Stored edge ids each file's keep/replace decision marked for deletion.
    // Deferred to the write phase: deleting during the read phase would be a
    // committed DELETE, so a later node-write failure would leave the file's
    // outgoing edges destroyed with nothing re-added.
    let mut edge_removals: Vec<u64> = Vec::new();

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
                &mut edge_removals,
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
    if !edge_removals.is_empty() || !edge_changes.is_empty() {
        // Removals FIRST: `update_edges` applies changes in order and adds
        // are INSERT OR IGNORE, so an add for an unchanged call site (same
        // line) would be ignored against the still-present old row and a
        // later removal would then delete the only copy.
        let changes: Vec<EdgeChange> = edge_removals
            .into_iter()
            .map(EdgeChange::Remove)
            .chain(edge_changes)
            .collect();
        if let Err(e) = store.update_edges(changes) {
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
    edge_removals: &mut Vec<u64>,
) {
    // A parse that produced zero definitions is far more likely a transient
    // parse failure (an on-edit hook compiling mid-edit syntax) than a file
    // genuinely emptied out. Treat it as no-evidence and leave the graph
    // exactly as it was — no node removals, no edge replacement; the next
    // successful compile or `keel map` supplies the real state. E004/E001
    // already ran against the pre-sync graph, so detection loses nothing.
    // (With no definitions there is also no containing def to attribute any
    // reference edge to, so nothing below could produce output anyway.)
    if file.definitions.is_empty() {
        return;
    }

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

    // Re-resolve this file's outgoing reference edges, then decide per stored
    // edge whether the fresh result replaces it (see the module docs): delete
    // a stored edge only when this pass re-added the same
    // (source, target, kind), or when its target lives inside this file —
    // never because a cross-file reference failed to re-resolve. The
    // deletions land in the WRITE phase (after node writes succeed), so a
    // failed node write can never leave the file edge-less.
    let stored = store.reference_edges_from_file(rel_path);
    let fresh_start = edge_changes.len();
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
    let added: HashSet<(u64, u64, EdgeKind)> = edge_changes[fresh_start..]
        .iter()
        .filter_map(|c| match c {
            EdgeChange::Add(e) => Some((e.source_id, e.target_id, e.kind.clone())),
            _ => None,
        })
        .collect();
    edge_removals.extend(
        stored
            .iter()
            .filter(|(e, target_file)| {
                added.contains(&(e.source_id, e.target_id, e.kind.clone()))
                    || target_file == rel_path
            })
            .map(|(e, _)| e.id),
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
        let Some((kind, _)) = edge_for_reference(&reference.kind) else {
            continue;
        };
        if let Some(resolved) = resolve_reference(local, &idx, &ctx, reference) {
            push_reference_edge(
                file,
                local,
                reference.line,
                resolved.target_id,
                kind,
                resolved.confidence,
                &resolved.tier,
                next_id,
                edge_changes,
                node_tiers,
            );
        }
    }
}

/// Find the definition containing `line` and emit a `calls`/`uses` edge from it.
#[allow(clippy::too_many_arguments)]
fn push_reference_edge(
    file: &FileIndex,
    local: &HashMap<String, u64>,
    line: u32,
    target_id: u64,
    kind: EdgeKind,
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
    let is_call = kind == EdgeKind::Calls;
    edge_changes.push(EdgeChange::Add(GraphEdge {
        id: edge_id,
        source_id,
        target_id,
        kind,
        file_path: file.file_path.clone(),
        line,
        confidence,
    }));
    // A node's tier describes its call-resolution quality, so value references
    // do not set it (mirrors the map's second pass).
    if is_call {
        node_tiers
            .entry(source_id)
            .or_insert_with(|| tier.to_string());
    }
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
        is_associated: false,
        // Modules are not measured — no body, nothing to branch.
        complexity: 0,
        is_trivial_wrapper: false,
        in_test_context: false,
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
        is_associated: def.is_associated,
        complexity: def.complexity,
        is_trivial_wrapper: def.stored_trivial_wrapper(),
        in_test_context: def.in_test_context,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id,
        package: None,
    }
}

#[cfg(test)]
#[path = "compile_sync_tests.rs"]
mod tests;
