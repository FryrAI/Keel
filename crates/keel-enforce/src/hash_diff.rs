//! Discovery, resolution, and graph-traversal methods for EnforcementEngine.
//!
//! Contains: discover(), where_hash(), explain(), collect_adjacency(),
//! and build_module_context().

use std::collections::{HashSet, VecDeque};

use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};

use crate::engine::{node_to_info, EnforcementEngine};
use crate::types::{
    CalleeInfo, CallerInfo, DiscoverResult, ExplainResult, ModuleContext, ResolutionStep,
};

impl EnforcementEngine {
    /// Look up a node's callers, callees, and module context.
    pub fn discover(&self, hash: &str, depth: u32) -> Option<DiscoverResult> {
        let node = self.store.get_node(hash)?;

        let mut upstream = Vec::new();
        let mut downstream = Vec::new();
        self.collect_adjacency(node.id, depth, &mut upstream, &mut downstream);

        let module_ctx = self.build_module_context(node.module_id);

        Some(DiscoverResult {
            version: "0.1.0".to_string(),
            command: "discover".to_string(),
            target: node_to_info(&node),
            upstream,
            downstream,
            module_context: module_ctx,
            body_context: None,
        })
    }

    /// Resolve hash to (file, line).
    pub fn where_hash(&self, hash: &str) -> Option<(String, u32)> {
        let node = self.store.get_node(hash)?;
        Some((node.file_path, node.line_start))
    }

    /// Show resolution reasoning for an error.
    pub fn explain(&self, error_code: &str, hash: &str) -> Option<ExplainResult> {
        let node = self.store.get_node(hash)?;
        let mut chain = Vec::new();

        // Build resolution chain based on edges
        let edges = self.store.get_edges(node.id, EdgeDirection::Both);
        for edge in &edges {
            let kind = match edge.kind {
                EdgeKind::Calls => "call",
                EdgeKind::Imports => "import",
                EdgeKind::Inherits => "type_ref",
                EdgeKind::Contains => "re_export",
            };
            chain.push(ResolutionStep {
                kind: kind.to_string(),
                file: edge.file_path.clone(),
                line: edge.line,
                text: format!("{} edge at {}:{}", kind, edge.file_path, edge.line),
            });
        }

        let confidence = if self
            .circuit_breaker
            .is_downgraded(error_code, hash, &node.file_path)
        {
            0.5
        } else {
            0.92
        };

        Some(ExplainResult {
            version: "0.1.0".to_string(),
            command: "explain".to_string(),
            error_code: error_code.to_string(),
            hash: hash.to_string(),
            confidence,
            resolution_tier: "tree-sitter".to_string(),
            resolution_chain: chain,
            summary: format!(
                "{} on `{}` in {}:{}",
                error_code, node.name, node.file_path, node.line_start
            ),
        })
    }

    /// BFS traversal to collect callers/callees up to `depth` levels.
    /// depth=1 returns direct callers/callees, depth=2 adds their callers/callees, etc.
    pub(crate) fn collect_adjacency(
        &self,
        node_id: u64,
        depth: u32,
        upstream: &mut Vec<CallerInfo>,
        downstream: &mut Vec<CalleeInfo>,
    ) {
        if depth == 0 {
            return;
        }
        let max_depth = depth.min(3); // Cap at 3 to prevent runaway traversals

        // BFS for callers (incoming edges)
        let mut caller_queue: VecDeque<(u64, u32)> = VecDeque::new();
        let mut caller_visited: HashSet<u64> = HashSet::new();
        caller_visited.insert(node_id);
        caller_queue.push_back((node_id, 0));

        while let Some((current_id, current_depth)) = caller_queue.pop_front() {
            if current_depth >= max_depth {
                continue;
            }
            let edges = self.store.get_edges(current_id, EdgeDirection::Incoming);
            for edge in &edges {
                if edge.kind != EdgeKind::Calls {
                    continue;
                }
                if caller_visited.contains(&edge.source_id) {
                    continue;
                }
                caller_visited.insert(edge.source_id);
                if let Some(caller) = self.store.get_node_by_id(edge.source_id) {
                    upstream.push(CallerInfo {
                        hash: caller.hash.clone(),
                        name: caller.name.clone(),
                        signature: caller.signature.clone(),
                        file: caller.file_path.clone(),
                        line: caller.line_start,
                        docstring: caller.docstring.clone(),
                        call_line: edge.line,
                        distance: current_depth + 1,
                    });
                    caller_queue.push_back((edge.source_id, current_depth + 1));
                }
            }
        }

        // BFS for callees (outgoing edges)
        let mut callee_queue: VecDeque<(u64, u32)> = VecDeque::new();
        let mut callee_visited: HashSet<u64> = HashSet::new();
        callee_visited.insert(node_id);
        callee_queue.push_back((node_id, 0));

        while let Some((current_id, current_depth)) = callee_queue.pop_front() {
            if current_depth >= max_depth {
                continue;
            }
            let edges = self.store.get_edges(current_id, EdgeDirection::Outgoing);
            for edge in &edges {
                if edge.kind != EdgeKind::Calls {
                    continue;
                }
                if callee_visited.contains(&edge.target_id) {
                    continue;
                }
                callee_visited.insert(edge.target_id);
                if let Some(callee) = self.store.get_node_by_id(edge.target_id) {
                    downstream.push(CalleeInfo {
                        hash: callee.hash.clone(),
                        name: callee.name.clone(),
                        signature: callee.signature.clone(),
                        file: callee.file_path.clone(),
                        line: callee.line_start,
                        docstring: callee.docstring.clone(),
                        call_line: edge.line,
                        distance: current_depth + 1,
                    });
                    callee_queue.push_back((edge.target_id, current_depth + 1));
                }
            }
        }
    }

    pub(crate) fn build_module_context(&self, module_id: u64) -> ModuleContext {
        let profile = self.store.get_module_profile(module_id);
        match profile {
            Some(p) => {
                let nodes = self.store.get_nodes_in_file(&p.path);
                let siblings: Vec<String> = nodes
                    .iter()
                    .filter(|n| n.kind == NodeKind::Function)
                    .map(|n| n.name.clone())
                    .collect();
                let endpoints: Vec<String> = nodes
                    .iter()
                    .flat_map(|n| n.external_endpoints.iter())
                    .map(|e| format!("{} {}", e.method, e.path))
                    .collect();
                ModuleContext {
                    module: p.path,
                    sibling_functions: siblings,
                    responsibility_keywords: p.responsibility_keywords,
                    function_count: p.function_count,
                    external_endpoints: endpoints,
                }
            }
            None => ModuleContext {
                module: String::new(),
                sibling_functions: vec![],
                responsibility_keywords: vec![],
                function_count: 0,
                external_endpoints: vec![],
            },
        }
    }
}

#[cfg(test)]
#[path = "hash_diff_tests.rs"]
mod tests;
