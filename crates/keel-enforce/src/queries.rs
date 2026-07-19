//! Read-only graph queries used by the HTTP/MCP servers: name-scoped discover,
//! substring search, and a graph-wide module map.
//!
//! These delegate to the same `GraphStore` the CLI uses, so the server never
//! grows its own copy of graph-traversal logic — HTTP handlers stay thin.

use keel_core::types::GraphNode;

use crate::engine::EnforcementEngine;
use crate::types::DiscoverResult;

/// One module entry in a graph-wide map: file path, module name, and how many
/// nodes it contains.
#[derive(Debug, Clone)]
pub struct ModuleMapEntry {
    pub name: String,
    pub file: String,
    pub node_count: usize,
}

impl EnforcementEngine {
    /// Search graph nodes by name: exact `(name, kind)` match first, then a
    /// case-insensitive substring scan. Mirrors the CLI `keel search` fallback
    /// so all interfaces rank results identically. Results are capped at `limit`.
    pub fn search_graph(&self, term: &str, kind: Option<&str>, limit: usize) -> Vec<GraphNode> {
        let kind_str = kind.unwrap_or("");
        let mut results = self.store.find_nodes_by_name(term, kind_str, "");

        if results.is_empty() {
            let term_lower = term.to_lowercase();
            for module in self.store.get_all_modules() {
                for node in self.store.get_nodes_in_file(&module.file_path) {
                    if node.name.to_lowercase().contains(&term_lower)
                        && (kind_str.is_empty() || node.kind.as_str() == kind_str)
                    {
                        results.push(node);
                    }
                }
            }
        }

        results.truncate(limit);
        results
    }

    /// List every module with its node count, for a graph-wide map summary.
    pub fn module_map(&self) -> Vec<ModuleMapEntry> {
        self.store
            .get_all_modules()
            .into_iter()
            .map(|m| {
                let node_count = self.store.get_nodes_in_file(&m.file_path).len();
                ModuleMapEntry {
                    name: m.name,
                    file: m.file_path,
                    node_count,
                }
            })
            .collect()
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
    fn nodes_in_file_flex(&self, file: &str) -> Vec<GraphNode> {
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
