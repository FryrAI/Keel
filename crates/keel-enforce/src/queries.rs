//! Read-only graph queries used by the HTTP/MCP servers: name-scoped discover,
//! substring search, and a graph-wide module map.
//!
//! These delegate to the same `GraphStore` the CLI uses, so the server never
//! grows its own copy of graph-traversal logic — HTTP handlers stay thin.

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, GraphNode};

use crate::engine::EnforcementEngine;
use crate::types::DiscoverResult;

/// True for the edge kinds that make one node *depend on* another: a resolved
/// call, and a name used as a value (callback, handler table, Svelte template
/// expression).
///
/// The adjacency surfaces — `discover`, `focus`, `search` — answer "what
/// depends on this?", and a `uses` edge answers it: a function reached only
/// through `.map(render)` or `<Panel onDone={refresh} />` is being used, and is
/// not free to delete. Counting only `Calls` reported those functions at zero
/// callers, which is the same false-dead-code signal W005 already refuses to
/// send.
///
/// The set itself lives in `keel_core::types::SYMBOL_DEP_KINDS`, which the
/// metrics SQL renders its `IN (...)` list from — one definition, two readers.
///
/// Severity checks deliberately do NOT go through here. E001/E004/E005 and the
/// fix planner filter [`EdgeKind::Calls`] themselves, because only a parsed
/// call site carries an argument list.
pub fn is_dependency_edge(kind: &EdgeKind) -> bool {
    keel_core::types::SYMBOL_DEP_KINDS.contains(kind)
}

/// Count the stored dependency edges INTO `node_id` — its fan-in (caller
/// count), including value/template uses (see [`is_dependency_edge`]).
///
/// The shared home for the `get_edges + filter + count` idiom that was
/// copy-pasted across focus, checkpoint, check, and the CLI. Callers hand it a
/// `&dyn GraphStore` so every interface counts the same way.
pub fn call_fan_in(store: &dyn GraphStore, node_id: u64) -> u32 {
    store
        .get_edges(node_id, EdgeDirection::Incoming)
        .iter()
        .filter(|e| is_dependency_edge(&e.kind))
        .count() as u32
}

/// Count the stored dependency edges OUT OF `node_id` — its fan-out (callee
/// count), including value/template uses (see [`is_dependency_edge`]).
pub fn call_fan_out(store: &dyn GraphStore, node_id: u64) -> u32 {
    store
        .get_edges(node_id, EdgeDirection::Outgoing)
        .iter()
        .filter(|e| is_dependency_edge(&e.kind))
        .count() as u32
}

/// Search graph nodes by name: exact `(name, kind)` match first, then a
/// case-insensitive substring scan. Results are capped at `limit`.
///
/// This is the single search implementation shared by every interface — the
/// CLI `keel search`, the MCP `keel/search` tool, and the HTTP `/search`
/// route all call it, so they return identical results in identical order.
///
/// The substring fallback stops the moment it has `limit` matches rather than
/// materializing every node in the graph and truncating afterwards — the
/// exact-match path is tried first and unchanged, so ranking is preserved.
pub fn search_graph(
    store: &dyn GraphStore,
    term: &str,
    kind: Option<&str>,
    limit: usize,
) -> Vec<GraphNode> {
    let kind_str = kind.unwrap_or("");
    let mut results = store.find_nodes_by_name(term, kind_str, "");

    if results.is_empty() && limit > 0 {
        let term_lower = term.to_lowercase();
        'scan: for module in store.get_all_modules() {
            for node in store.get_nodes_in_file(&module.file_path) {
                if node.name.to_lowercase().contains(&term_lower)
                    && (kind_str.is_empty() || node.kind.as_str() == kind_str)
                {
                    results.push(node);
                    if results.len() >= limit {
                        break 'scan;
                    }
                }
            }
        }
    }

    results.truncate(limit);
    results
}

impl EnforcementEngine {
    /// Search graph nodes by name, delegating to the shared [`search_graph`]
    /// so the engine-backed interfaces (HTTP) rank results identically to the
    /// store-backed ones (CLI, MCP).
    pub fn search_graph(&self, term: &str, kind: Option<&str>, limit: usize) -> Vec<GraphNode> {
        search_graph(&*self.store, term, kind, limit)
    }

    /// Reconstruct the full [`crate::types::MapResult`] from the engine's graph.
    ///
    /// Delegates to [`crate::map::build_map_from_store`] — the single map
    /// assembly path — so the HTTP `/map` route reports the same summary,
    /// hotspots, and module profiles as `keel map --cached` and MCP `keel/map`.
    pub fn build_map(&self, depth: u32) -> crate::types::MapResult {
        crate::map::build_map_from_store(&*self.store, depth)
    }

    /// Discover a symbol identified by name (and optional position), falling
    /// back from a direct hash lookup to a name lookup scoped to `file`.
    ///
    /// Resolution order (per the extension↔server contract):
    /// 1. Treat `ident` as a hash — the existing `discover` path.
    /// 2. Otherwise match nodes named `ident` in `file`; when several match,
    ///    pick the one whose start line is closest to `line`.
    ///
    /// Returns the same [`DiscoverResult`] as hash discover, so callers can
    /// reuse one shape or flatten it as they need.
    pub fn discover_named(
        &self,
        ident: &str,
        file: Option<&str>,
        line: Option<u32>,
        depth: u32,
    ) -> Option<DiscoverResult> {
        if let Some(result) = self.discover(ident, depth) {
            return Some(result);
        }

        let file = file?;
        let matches: Vec<GraphNode> = self
            .nodes_in_file_flex(file)
            .into_iter()
            .filter(|n| n.name == ident)
            .collect();

        let chosen = match line {
            Some(l) => matches
                .into_iter()
                .min_by_key(|n| (i64::from(n.line_start) - i64::from(l)).abs())?,
            None => matches.into_iter().next()?,
        };

        self.discover(&chosen.hash, depth)
    }

    /// Nodes in `file`, tolerating relative/absolute path differences between
    /// what a client sends and what the graph stored. Exact match wins; a
    /// suffix match is the fallback so a workspace-relative path from an editor
    /// still resolves against absolute stored paths (and vice versa).
    pub(crate) fn nodes_in_file_flex(&self, file: &str) -> Vec<GraphNode> {
        let exact = self.store.get_nodes_in_file(file);
        if !exact.is_empty() {
            return exact;
        }

        let want = file.replace('\\', "/");
        let mut out = Vec::new();
        for module in self.store.get_all_modules() {
            let stored = module.file_path.replace('\\', "/");
            let matches = stored == want
                || stored.ends_with(&format!("/{}", want))
                || want.ends_with(&format!("/{}", stored));
            if matches {
                out.extend(self.store.get_nodes_in_file(&module.file_path));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_core::sqlite::SqliteGraphStore;
    use keel_core::types::{GraphNode, NodeKind};

    fn node(id: u64, name: &str, kind: NodeKind) -> GraphNode {
        GraphNode {
            id,
            hash: format!("hash{id:08}"),
            kind,
            name: name.to_string(),
            signature: format!("fn {name}()"),
            file_path: "src/lib.rs".to_string(),
            line_start: id as u32,
            line_end: id as u32 + 1,
            docstring: None,
            is_public: true,
            type_hints_present: true,
            has_docstring: false,
            is_associated: false,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 0,
            package: None,
        }
    }

    fn fixture_store() -> SqliteGraphStore {
        let store = SqliteGraphStore::in_memory().unwrap();
        // Module so the substring scan has a file to walk.
        store
            .insert_node(&node(1, "module_lib", NodeKind::Module))
            .unwrap();
        store
            .insert_node(&node(2, "parse", NodeKind::Function))
            .unwrap();
        store
            .insert_node(&node(3, "parse_body", NodeKind::Function))
            .unwrap();
        store
            .insert_node(&node(4, "reparse_all", NodeKind::Function))
            .unwrap();
        store
    }

    #[test]
    fn exact_match_ranks_first() {
        let store = fixture_store();
        // "parse" matches exactly, so the exact-match path returns only it —
        // the substring scan (which would also catch parse_body/reparse_all)
        // is skipped because the exact match is non-empty.
        let results = search_graph(&store, "parse", None, 20);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "parse");
    }

    #[test]
    fn substring_fallback_when_no_exact_match() {
        let store = fixture_store();
        let results = search_graph(&store, "pars", None, 20);
        let names: Vec<&str> = results.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"parse"));
        assert!(names.contains(&"parse_body"));
        assert!(names.contains(&"reparse_all"));
    }

    #[test]
    fn limit_caps_results() {
        let store = fixture_store();
        let results = search_graph(&store, "pars", None, 2);
        assert_eq!(results.len(), 2);
    }
}
