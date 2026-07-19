//! `keel focus` (issue #20) — the minimal context set for safely modifying a
//! target: transitive callers, direct callees, and the files containing them.
//!
//! Backed entirely by the existing graph (`GraphStore`), reusing the same
//! BFS traversal `discover` uses (`collect_adjacency`). Shared by the CLI
//! command and the `keel/focus` MCP tool.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use keel_core::types::{GraphNode, NodeKind};

use crate::engine::EnforcementEngine;
use crate::queries::{call_fan_in, call_fan_out};
use crate::types::{FocusFile, FocusResult, FocusSymbol, Relation};

impl EnforcementEngine {
    /// Build the minimal context set for safely modifying `target`.
    ///
    /// `target` is a node hash or a file path. Collects transitive callers up
    /// to `depth` (the symbols at risk) and direct callees (dependencies), then
    /// groups them by file. Files are ranked by graph distance then caller
    /// count; `read_order` lists them dependencies-first.
    pub fn focus(&self, target: &str, depth: u32) -> Option<FocusResult> {
        let targets = self.resolve_focus_targets(target)?;
        let mut symbols: HashMap<String, FocusSymbol> = HashMap::new();

        for node in &targets {
            self.record_symbol(&mut symbols, node, 0, Relation::Target);

            // Transitive callers up to `depth`; direct callees only (distance 1).
            let mut upstream = Vec::new();
            let mut downstream = Vec::new();
            self.collect_adjacency(node.id, depth.max(1), &mut upstream, &mut downstream);

            for c in upstream {
                if let Some(n) = self.store.get_node(&c.hash) {
                    self.record_symbol(&mut symbols, &n, c.distance, Relation::Caller);
                }
            }
            for c in downstream.into_iter().filter(|c| c.distance == 1) {
                if let Some(n) = self.store.get_node(&c.hash) {
                    self.record_symbol(&mut symbols, &n, 1, Relation::Callee);
                }
            }
        }

        // Symbols at risk: the transitive callers, ranked by distance then fan-in.
        let mut callers: Vec<FocusSymbol> = symbols
            .values()
            .filter(|s| s.relation == Relation::Caller)
            .cloned()
            .collect();
        callers.sort_by(|a, b| {
            a.distance
                .cmp(&b.distance)
                .then(b.callers.cmp(&a.callers))
                .then(a.name.cmp(&b.name))
        });

        // Group symbols into files.
        let mut files_map: HashMap<String, FocusFile> = HashMap::new();
        for s in symbols.values() {
            let entry = files_map
                .entry(s.file.clone())
                .or_insert_with(|| FocusFile {
                    path: s.file.clone(),
                    role: s.relation,
                    distance: s.distance,
                    symbols: Vec::new(),
                });
            if s.distance < entry.distance {
                entry.distance = s.distance;
                entry.role = s.relation;
            }
            entry.symbols.push(s.clone());
        }
        let mut files: Vec<FocusFile> = files_map.into_values().collect();
        for f in &mut files {
            f.symbols.sort_by_key(|s| s.line);
        }
        // Ranked: nearest first, then higher fan-in, then path for stability.
        files.sort_by(|a, b| {
            a.distance
                .cmp(&b.distance)
                .then(file_max_callers(b).cmp(&file_max_callers(a)))
                .then(a.path.cmp(&b.path))
        });

        // Suggested read order: dependencies-first (callee → target → caller).
        let mut ordered = files.clone();
        ordered.sort_by(|a, b| {
            role_rank(a.role)
                .cmp(&role_rank(b.role))
                .then(a.distance.cmp(&b.distance))
                .then(a.path.cmp(&b.path))
        });
        let read_order: Vec<String> = ordered.into_iter().map(|f| f.path).collect();

        Some(FocusResult {
            version: env!("CARGO_PKG_VERSION").to_string(),
            command: "focus".to_string(),
            target: target.to_string(),
            depth,
            files,
            callers,
            read_order,
        })
    }

    /// Resolve a focus target string to one or more nodes: a hash resolves to a
    /// single node; otherwise the string is treated as a file path and every
    /// non-module symbol in it becomes a target.
    ///
    /// The file path resolves through [`Self::nodes_in_file_flex`] (the same
    /// path-flexible lookup `discover` uses), so an absolute or otherwise
    /// mismatched path from an editor resolves against the stored graph the
    /// same way it does in `keel discover`.
    fn resolve_focus_targets(&self, target: &str) -> Option<Vec<GraphNode>> {
        if let Some(node) = self.store.get_node(target) {
            return Some(vec![node]);
        }
        let nodes: Vec<GraphNode> = self
            .nodes_in_file_flex(target)
            .into_iter()
            .filter(|n| n.kind != NodeKind::Module)
            .collect();
        if nodes.is_empty() {
            None
        } else {
            Some(nodes)
        }
    }

    /// Insert or upgrade a symbol in the focus set, keeping the entry with the
    /// highest priority (target beats a nearer relation beats a farther one).
    fn record_symbol(
        &self,
        map: &mut HashMap<String, FocusSymbol>,
        node: &GraphNode,
        distance: u32,
        relation: Relation,
    ) {
        let incoming = relation_priority(relation, distance);
        match map.entry(node.hash.clone()) {
            Entry::Occupied(mut e) => {
                let current = relation_priority(e.get().relation, e.get().distance);
                if incoming < current {
                    *e.get_mut() = self.build_focus_symbol(node, distance, relation);
                }
            }
            Entry::Vacant(v) => {
                v.insert(self.build_focus_symbol(node, distance, relation));
            }
        }
    }

    /// Build a `FocusSymbol` for a node, computing its call fan-in/fan-out.
    fn build_focus_symbol(
        &self,
        node: &GraphNode,
        distance: u32,
        relation: Relation,
    ) -> FocusSymbol {
        FocusSymbol {
            name: node.name.clone(),
            hash: node.hash.clone(),
            file: node.file_path.clone(),
            line: node.line_start,
            callers: call_fan_in(&*self.store, node.id),
            callees: call_fan_out(&*self.store, node.id),
            distance,
            relation,
        }
    }
}

/// Read-order priority: dependencies (callees) first, then the target, then
/// the callers that are affected by the change.
fn role_rank(role: Relation) -> u8 {
    match role {
        Relation::Callee => 0,
        Relation::Target => 1,
        Relation::Caller => 2,
    }
}

/// Merge priority (lower wins): the target always, then nearer relations. A
/// callee (direct dependency) outranks an equally-distant caller.
fn relation_priority(relation: Relation, distance: u32) -> u32 {
    match relation {
        Relation::Target => 0,
        Relation::Callee => 1,
        Relation::Caller => 1 + distance,
    }
}

fn file_max_callers(f: &FocusFile) -> u32 {
    f.symbols.iter().map(|s| s.callers).max().unwrap_or(0)
}

#[cfg(test)]
#[path = "focus_tests.rs"]
mod tests;
