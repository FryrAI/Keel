//! Boundary materialisation for `keel map`.
//!
//! Turns the [`BoundarySymbol`]s discovered by a
//! [`BoundaryProvider`](keel_parsers::boundary::BoundaryProvider) into graph
//! nodes so that calls into boundary functions (e.g. a `.baml` LLM function
//! `b.ExtractResume(...)`) resolve to a recognizable external surface instead
//! of reading as silent unresolved edges.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use keel_core::hash::{compute_hash, compute_hash_disambiguated};
use keel_core::sqlite::SqliteGraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind};
use keel_parsers::boundary::BoundarySymbol;

/// Materialise boundary `symbols` as graph nodes and return an index of
/// `function name -> (node id, confidence)` used to resolve calls into them.
///
/// Each declaring file becomes a [`NodeKind::Module`] node containing one
/// [`NodeKind::Function`] node per function symbol and one [`NodeKind::Class`]
/// node per class symbol — mirroring how real source files are represented so
/// the nodes show up naturally in the map. Only function symbols are call
/// targets, so only they enter the returned index (first-file-wins on a name
/// collision).
///
/// `confidence` is the producing provider's
/// [`confidence`](keel_parsers::boundary::BoundaryProvider::confidence) and is
/// stored per entry, so a call resolving into this entry records the confidence
/// of the provider that supplied it (not a shared scalar that the last provider
/// in the loop would otherwise overwrite for every boundary edge).
pub fn inject_boundary_symbols(
    symbols: &[BoundarySymbol],
    confidence: f64,
    node_changes: &mut Vec<NodeChange>,
    edge_changes: &mut Vec<EdgeChange>,
    next_id: &mut u64,
    assigned_hashes: &mut HashSet<String>,
    valid_node_ids: &mut HashSet<u64>,
) -> HashMap<String, (u64, f64)> {
    let mut fn_index: HashMap<String, (u64, f64)> = HashMap::new();
    if symbols.is_empty() {
        return fn_index;
    }

    // Group symbols by declaring file so each file gets exactly one module node,
    // keeping functions before classes within each file (encounter order). This
    // ordering fixes the node-id / hash-disambiguation sequence, so it must stay
    // deterministic across runs.
    let mut by_file: HashMap<&str, (Vec<&BoundarySymbol>, Vec<&BoundarySymbol>)> = HashMap::new();
    for sym in symbols {
        let entry = by_file.entry(sym.file_path.as_str()).or_default();
        if sym.kind == NodeKind::Function {
            entry.0.push(sym);
        } else {
            entry.1.push(sym);
        }
    }

    // Deterministic ordering → stable node ids across runs.
    let mut files: Vec<&str> = by_file.keys().copied().collect();
    files.sort_unstable();

    for file in files {
        let (funcs, classes) = &by_file[file];

        let module_id = alloc_node(
            next_id,
            valid_node_ids,
            assigned_hashes,
            node_changes,
            NodeKind::Module,
            file,
            file,
            String::new(),
            1,
            0,
        );

        // Functions first, then classes — the order fixes the node-id sequence.
        for sym in funcs.iter().chain(classes) {
            let node_id = alloc_node(
                next_id,
                valid_node_ids,
                assigned_hashes,
                node_changes,
                sym.kind.clone(),
                &sym.name,
                file,
                sym.signature.clone(),
                sym.line,
                module_id,
            );
            push_contains(edge_changes, next_id, module_id, node_id, file, sym.line);
            // Only functions are call targets, so only they enter the index.
            if sym.kind == NodeKind::Function {
                fn_index
                    .entry(sym.name.clone())
                    .or_insert((node_id, confidence));
            }
        }
    }

    fn_index
}

/// Resolve a call reference name to a boundary function node, returning its
/// `(node id, confidence)`.
///
/// Matches the trailing segment of a (possibly qualified) call — the
/// `ExtractResume` in `b.ExtractResume` or `client::ExtractResume` — against
/// the boundary function index. Returns `None` when nothing matches. The
/// confidence is the one stored with the matched entry, i.e. the confidence of
/// the provider that produced it. Case-sensitive matching plus PascalCase
/// boundary names keep collisions with ordinary snake_case methods vanishingly
/// unlikely.
pub fn resolve_boundary_call(
    callee_name: &str,
    index: &HashMap<String, (u64, f64)>,
) -> Option<(u64, f64)> {
    if index.is_empty() {
        return None;
    }
    let segment = callee_name
        .rsplit_once('.')
        .or_else(|| callee_name.rsplit_once("::"))
        .map(|(_, s)| s)
        .unwrap_or(callee_name);
    index.get(segment).copied()
}

/// The string-literal key set for a repo whose boundary surface has just been
/// scanned: every *function* symbol's name.
///
/// Installed on the language resolvers before parsing so a string literal
/// naming one of these becomes a [`keel_parsers::resolver::ReferenceKind::Literal`]
/// reference and every other string in the repo is dropped inside the parser.
/// Only function symbols are call targets, matching
/// [`inject_boundary_symbols`]'s index.
pub fn literal_keys<'a>(
    symbols: impl IntoIterator<Item = &'a BoundarySymbol>,
) -> Arc<HashSet<String>> {
    Arc::new(
        symbols
            .into_iter()
            .filter(|s| s.kind == NodeKind::Function)
            .map(|s| s.name.clone())
            .collect(),
    )
}

/// The same key set for a pipeline that reads the boundary surface from the
/// *persisted graph* instead of re-scanning it — `keel compile`, which must
/// reproduce the map's literal edges or its prune-and-re-resolve would delete
/// them (see the contract trap on
/// [`keel_parsers::boundary::BoundaryProvider`]).
///
/// Gated on `baml_src/` exactly as the compile-side boundary index is, so a
/// repo with no BAML surface pays no query.
pub fn persisted_literal_keys(cwd: &Path, store: &SqliteGraphStore) -> Arc<HashSet<String>> {
    if !cwd.join("baml_src").exists() {
        return Arc::new(HashSet::new());
    }
    Arc::new(store.baml_function_nodes().into_keys().collect())
}

/// Allocate a graph node, register its id/hash, and push the `Add` change.
/// Returns the new node's id.
#[allow(clippy::too_many_arguments)]
fn alloc_node(
    next_id: &mut u64,
    valid_node_ids: &mut HashSet<u64>,
    assigned_hashes: &mut HashSet<String>,
    node_changes: &mut Vec<NodeChange>,
    kind: NodeKind,
    name: &str,
    file_path: &str,
    signature: String,
    line: u32,
    module_id: u64,
) -> u64 {
    let id = *next_id;
    *next_id += 1;
    valid_node_ids.insert(id);

    let mut hash = compute_hash(&signature, "", "");
    if assigned_hashes.contains(&hash) {
        hash = compute_hash_disambiguated(&signature, "", "", file_path);
    }
    assigned_hashes.insert(hash.clone());

    node_changes.push(NodeChange::Add(GraphNode {
        id,
        hash,
        kind,
        name: name.to_string(),
        signature,
        file_path: file_path.to_string(),
        line_start: line,
        line_end: line,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        is_associated: false,
        // Boundary declarations (`.baml` functions) have no walked body.
        complexity: 0,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id,
        package: None,
    }));
    id
}

/// Push a `Contains` edge from a module node to one of its members.
fn push_contains(
    edge_changes: &mut Vec<EdgeChange>,
    next_id: &mut u64,
    module_id: u64,
    node_id: u64,
    file_path: &str,
    line: u32,
) {
    let edge_id = *next_id;
    *next_id += 1;
    edge_changes.push(EdgeChange::Add(GraphEdge {
        id: edge_id,
        source_id: module_id,
        target_id: node_id,
        kind: EdgeKind::Contains,
        file_path: file_path.to_string(),
        line,
        confidence: 1.0,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn func(name: &str, file: &str, line: u32) -> BoundarySymbol {
        BoundarySymbol {
            name: name.to_string(),
            file_path: file.to_string(),
            line,
            signature: format!("function {name}()"),
            kind: NodeKind::Function,
        }
    }

    #[test]
    fn test_inject_creates_module_and_function_nodes() {
        let symbols = vec![
            func("ExtractResume", "baml_src/main.baml", 3),
            BoundarySymbol {
                name: "Resume".into(),
                file_path: "baml_src/main.baml".into(),
                line: 1,
                signature: "class Resume".into(),
                kind: NodeKind::Class,
            },
        ];

        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut next_id = 1u64;
        let mut hashes = HashSet::new();
        let mut valid = HashSet::new();

        let index = inject_boundary_symbols(
            &symbols,
            keel_core::confidence::BAML_BOUNDARY,
            &mut nodes,
            &mut edges,
            &mut next_id,
            &mut hashes,
            &mut valid,
        );

        // module + function + class = 3 nodes; 2 contains edges.
        assert_eq!(nodes.len(), 3);
        assert_eq!(edges.len(), 2);
        // The function symbol enters the index carrying the provider confidence.
        assert_eq!(
            index.get("ExtractResume").map(|&(_, c)| c),
            Some(keel_core::confidence::BAML_BOUNDARY)
        );
        // classes are not call targets
        assert!(!index.contains_key("Resume"));
    }

    #[test]
    fn test_resolve_qualified_and_bare_calls() {
        let mut index = HashMap::new();
        index.insert(
            "ExtractResume".to_string(),
            (42u64, keel_core::confidence::BAML_BOUNDARY),
        );

        let expect = Some((42u64, keel_core::confidence::BAML_BOUNDARY));
        assert_eq!(resolve_boundary_call("b.ExtractResume", &index), expect);
        assert_eq!(
            resolve_boundary_call("client::ExtractResume", &index),
            expect
        );
        assert_eq!(resolve_boundary_call("ExtractResume", &index), expect);
        assert_eq!(resolve_boundary_call("b.somethingElse", &index), None);
        assert_eq!(
            resolve_boundary_call("ExtractResume", &HashMap::new()),
            None
        );
    }

    /// The literal key set carries exactly the call targets — the same
    /// function-only rule `inject_boundary_symbols` applies to its index, so a
    /// `"Resume"` string cannot conjure an edge to a BAML *class*.
    #[test]
    fn test_literal_keys_are_function_names_only() {
        let symbols = vec![
            func("ExtractResume", "baml_src/main.baml", 3),
            BoundarySymbol {
                name: "Resume".into(),
                file_path: "baml_src/main.baml".into(),
                line: 1,
                signature: "class Resume".into(),
                kind: NodeKind::Class,
            },
        ];
        let keys = literal_keys(&symbols);
        assert!(keys.contains("ExtractResume"));
        assert!(!keys.contains("Resume"));
        assert_eq!(keys.len(), 1);
        // No boundary surface -> empty set -> literals disabled entirely.
        assert!(literal_keys(&[]).is_empty());
    }

    #[test]
    fn test_inject_empty_boundary_is_noop() {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut next_id = 1u64;
        let mut hashes = HashSet::new();
        let mut valid = HashSet::new();
        let index = inject_boundary_symbols(
            &[],
            keel_core::confidence::BAML_BOUNDARY,
            &mut nodes,
            &mut edges,
            &mut next_id,
            &mut hashes,
            &mut valid,
        );
        assert!(index.is_empty());
        assert!(nodes.is_empty());
        assert!(edges.is_empty());
    }
}
