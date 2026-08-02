//! Navigation dimension — circular deps, coupling, unstable APIs, orphans, deep chains.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};

use crate::types::{AuditFinding, AuditSeverity};

/// Longest module cycle still worth reporting.
///
/// A 30-module strongly-connected component is not a defect a reader can act
/// on — it is a whole-architecture observation rendered as one unreadable
/// arrow chain. Cycles this long are dropped unless `--strict-cycles`.
const MAX_ACTIONABLE_CYCLE: usize = 8;

/// True for the edge kinds that make one MODULE depend on another.
///
/// A resolved call or an import is a load-order relationship between files; a
/// `uses` edge (a name handed around as a value) is not, and `contains` is
/// intra-file by construction. The set lives in
/// `keel_core::types::MODULE_DEP_KINDS`, read through here by the audit's
/// `circular_dep`/`high_coupling` checks and rendered into the metrics SQL's
/// `IN (...)` list — one definition, so a change moves both together.
///
/// Deliberately NOT `queries::is_dependency_edge`: that answers "does this
/// SYMBOL depend on that one" for fan-in counting, where a callback reference
/// absolutely counts.
pub(crate) fn is_module_dep_edge(kind: &EdgeKind) -> bool {
    keel_core::types::MODULE_DEP_KINDS.contains(kind)
}

/// Whether a module cycle is a real defect worth reporting.
///
/// `circular_dep` is **disabled for Rust**: intra-crate module cycles are legal,
/// idiomatic Rust (`mod a` and `mod b` may freely `use` each other — the crate
/// is one compilation unit), and cargo already forbids the illegal inter-crate
/// ones at build time, so keel can only ever report the legal kind. The check
/// remains meaningful for TypeScript, Python and Go, whose module systems all
/// make an import cycle a genuine load-order hazard.
///
/// A mixed-language cycle is still reported — that one crosses a boundary the
/// compiler does not police.
///
/// Shared with `crate::quality`, whose `cycle_count` metric must count exactly
/// the cycles `keel audit` would report — a trend line that disagrees with the
/// finding list it summarizes is worse than no trend line.
pub(crate) fn is_reportable_cycle(cycle: &[String]) -> bool {
    if cycle.len() > MAX_ACTIONABLE_CYCLE {
        return false;
    }
    !cycle
        .iter()
        .all(|p| keel_parsers::treesitter::detect_language(Path::new(p)) == Some("rust"))
}

/// Audit the navigation dimension: can a reader find their way around?
///
/// Flags modules that are unreachable from any other module (no incoming
/// edges) and modules whose fan-in is so high they act as chokepoints.
/// `files`, when given, restricts the audit to those module paths.
/// `strict_cycles` restores the pre-v0.5 `circular_dep` behavior: every cycle
/// reported, Rust and over-long ones included.
pub fn check_navigation(
    store: &dyn GraphStore,
    files: Option<&[String]>,
    strict_cycles: bool,
) -> Vec<AuditFinding> {
    let mut findings = Vec::new();

    let modules = match files {
        Some(paths) => paths
            .iter()
            .flat_map(|p| {
                store
                    .get_nodes_in_file(p)
                    .into_iter()
                    .find(|n| n.kind == NodeKind::Module)
                    .into_iter()
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        None => store.get_all_modules(),
    };

    // Build module-level dependency graph for cycle and coupling detection
    let mut module_deps: HashMap<String, HashSet<String>> = HashMap::new();
    let mut module_files: HashSet<String> = HashSet::new();

    for module in &modules {
        let path = &module.file_path;
        module_files.insert(path.clone());
        let nodes = store.get_nodes_in_file(path);

        let mut deps = HashSet::new();
        for node in &nodes {
            let outgoing = store.get_edges(node.id, EdgeDirection::Outgoing);
            for edge in &outgoing {
                if is_module_dep_edge(&edge.kind) {
                    if let Some(target) = store.get_node_by_id(edge.target_id) {
                        if target.file_path != *path {
                            deps.insert(target.file_path.clone());
                        }
                    }
                }
            }
        }
        module_deps.insert(path.clone(), deps);
    }

    // Circular dependency detection (Tarjan's SCC)
    let cycles = find_cycles(&module_deps);
    for cycle in &cycles {
        if !strict_cycles && !is_reportable_cycle(cycle) {
            continue;
        }
        let chain = cycle.join(" → ");
        let first = cycle.first().cloned().unwrap_or_default();
        let second = cycle.get(1).cloned().unwrap_or_default();
        findings.push(AuditFinding {
            severity: AuditSeverity::Fail,
            check: "circular_dep".into(),
            message: format!("Circular dependency: {}", chain),
            tip: Some(format!(
                "Circular dependency: {}. Extract the shared types/functions into a new module \
                 that both sides can import. Run `keel discover {}` and `keel discover {}` \
                 to identify the shared symbols.",
                chain, first, second,
            )),
            file: None,
            count: None,
        });
    }

    // High cross-module coupling: module imports >7 others
    for (path, deps) in &module_deps {
        if deps.len() > 7 {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warn,
                check: "high_coupling".into(),
                message: format!("{} imports {} other modules (>7)", path, deps.len()),
                tip: Some(format!(
                    "This module imports {} others (>7). Run `keel analyze {}` to see the \
                     dependency breakdown, then introduce a facade module or re-exports \
                     to reduce direct imports.",
                    deps.len(),
                    path,
                )),
                file: Some(path.clone()),
                count: None,
            });
        }
    }

    // Unstable APIs: public, >5 callers, no docstring
    // Orphan files: 0 incoming + 0 outgoing edges
    // Deep call chains: BFS >7 hops
    for module in &modules {
        let nodes = store.get_nodes_in_file(&module.file_path);

        let mut has_incoming = false;
        let mut has_outgoing = false;

        for node in &nodes {
            if node.kind == NodeKind::Module {
                continue;
            }

            let incoming = store.get_edges(node.id, EdgeDirection::Incoming);
            let outgoing = store.get_edges(node.id, EdgeDirection::Outgoing);

            let external_incoming = incoming.iter().any(|e| {
                store
                    .get_node_by_id(e.source_id)
                    .map(|n| n.file_path != module.file_path)
                    .unwrap_or(false)
            });
            let external_outgoing = outgoing.iter().any(|e| {
                store
                    .get_node_by_id(e.target_id)
                    .map(|n| n.file_path != module.file_path)
                    .unwrap_or(false)
            });

            if external_incoming {
                has_incoming = true;
            }
            if external_outgoing {
                has_outgoing = true;
            }

            // Unstable API: public + >5 callers + no docstring
            let call_callers = incoming
                .iter()
                .filter(|e| e.kind == EdgeKind::Calls)
                .count();
            if node.is_public && call_callers > 5 && !node.has_docstring {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Fail,
                    check: "unstable_api".into(),
                    message: format!(
                        "`{}` in {} — public, {} callers, no docstring",
                        node.name, module.file_path, call_callers,
                    ),
                    tip: Some(format!(
                        "Add a docstring to `{}` — it has {} callers and no documentation. \
                         Include: purpose, parameters, return type, and error conditions.",
                        node.name, call_callers,
                    )),
                    file: Some(module.file_path.clone()),
                    count: None,
                });
            }

            // Deep call chain detection (BFS from functions with 0 incoming calls)
            if node.kind == NodeKind::Function && call_callers == 0 {
                let max_depth = bfs_max_depth(store, node.id);
                if max_depth > 5 {
                    findings.push(AuditFinding {
                        severity: AuditSeverity::Warn,
                        check: "deep_call_chain".into(),
                        message: format!(
                            "`{}` in {} — call chain depth {} (>5 hops)",
                            node.name, module.file_path, max_depth,
                        ),
                        tip: Some(format!(
                            "Call chain from `{}` is {} hops deep (>5). Agents lose context \
                             beyond 5 levels. Run `keel discover --name {}` to trace the chain, \
                             then flatten with intermediate orchestrator functions.",
                            node.name, max_depth, node.name,
                        )),
                        file: Some(module.file_path.clone()),
                        count: None,
                    });
                }
            }
        }

        // Bottleneck detection: modules with high fan-in AND high fan-out
        let incoming_count = module_deps
            .iter()
            .filter(|(_, deps)| deps.contains(&module.file_path))
            .count();
        let outgoing_count = module_deps
            .get(&module.file_path)
            .map(|d| d.len())
            .unwrap_or(0);

        if incoming_count > 4 && outgoing_count > 4 {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warn,
                check: "bottleneck_module".into(),
                message: format!(
                    "{} — {} importers, {} dependencies (bottleneck)",
                    module.file_path, incoming_count, outgoing_count,
                ),
                tip: Some(format!(
                    "{} is imported by {} modules AND depends on {} others. Changes here \
                     have wide blast radius. Run `keel analyze {}` to identify which \
                     dependencies can be moved to separate modules.",
                    module.file_path, incoming_count, outgoing_count, module.file_path,
                )),
                file: Some(module.file_path.clone()),
                count: None,
            });
        }

        // Orphan file
        if !has_incoming && !has_outgoing {
            let non_module = nodes.iter().any(|n| n.kind != NodeKind::Module);
            if non_module {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Warn,
                    check: "orphan_file".into(),
                    message: format!(
                        "{} — no external dependencies in either direction",
                        module.file_path,
                    ),
                    tip: Some(format!(
                        "This file has no connections to the codebase. Run `keel discover {}` \
                         to verify — if it shows no symbols, it may be dead code. Either \
                         integrate it or delete it.",
                        module.file_path,
                    )),
                    file: Some(module.file_path.clone()),
                    count: None,
                });
            }
        }
    }

    findings
}

/// BFS from a node following outgoing Call edges, returns max depth reached.
fn bfs_max_depth(store: &dyn GraphStore, start_id: u64) -> u32 {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back((start_id, 0u32));
    visited.insert(start_id);
    let mut max_depth = 0u32;

    while let Some((node_id, depth)) = queue.pop_front() {
        if depth > max_depth {
            max_depth = depth;
        }
        // Cap BFS to avoid runaway on large graphs
        if depth >= 15 {
            continue;
        }
        let edges = store.get_edges(node_id, EdgeDirection::Outgoing);
        for edge in &edges {
            if edge.kind == EdgeKind::Calls && !visited.contains(&edge.target_id) {
                visited.insert(edge.target_id);
                queue.push_back((edge.target_id, depth + 1));
            }
        }
    }

    max_depth
}

/// Find cycles in a directed graph using iterative Tarjan's SCC algorithm.
/// Returns cycles as vectors of node names (file paths).
///
/// Shared with `crate::quality` so the `cycle_count` metric and the audit's
/// `circular_dep` findings are the same computation, not two that agree by
/// coincidence.
pub(crate) fn find_cycles(graph: &HashMap<String, HashSet<String>>) -> Vec<Vec<String>> {
    let mut index_counter = 0u32;
    let mut stack: Vec<String> = Vec::new();
    let mut on_stack: HashSet<String> = HashSet::new();
    let mut indices: HashMap<String, u32> = HashMap::new();
    let mut lowlinks: HashMap<String, u32> = HashMap::new();
    let mut cycles = Vec::new();

    // Iterative DFS with explicit stack to avoid stack overflow
    for start in graph.keys() {
        if indices.contains_key(start) {
            continue;
        }

        // DFS stack: (node, neighbor_iterator_index, is_returning)
        let mut dfs_stack: Vec<(String, Vec<String>, usize)> = Vec::new();

        indices.insert(start.clone(), index_counter);
        lowlinks.insert(start.clone(), index_counter);
        index_counter += 1;
        stack.push(start.clone());
        on_stack.insert(start.clone());

        let neighbors: Vec<String> = graph
            .get(start)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        dfs_stack.push((start.clone(), neighbors, 0));

        while let Some((node, neighbors, idx)) = dfs_stack.last_mut() {
            if *idx < neighbors.len() {
                let neighbor = neighbors[*idx].clone();
                *idx += 1;

                if !graph.contains_key(&neighbor) {
                    continue;
                }

                if !indices.contains_key(&neighbor) {
                    indices.insert(neighbor.clone(), index_counter);
                    lowlinks.insert(neighbor.clone(), index_counter);
                    index_counter += 1;
                    stack.push(neighbor.clone());
                    on_stack.insert(neighbor.clone());

                    let next_neighbors: Vec<String> = graph
                        .get(&neighbor)
                        .map(|s| s.iter().cloned().collect())
                        .unwrap_or_default();
                    dfs_stack.push((neighbor, next_neighbors, 0));
                } else if on_stack.contains(&neighbor) {
                    let node_ll = *lowlinks.get(node).unwrap_or(&u32::MAX);
                    let neighbor_idx = *indices.get(&neighbor).unwrap_or(&u32::MAX);
                    lowlinks.insert(node.clone(), node_ll.min(neighbor_idx));
                }
            } else {
                // Done with this node — pop and update parent
                let (finished_node, _, _) = dfs_stack.pop().unwrap();
                let finished_ll = *lowlinks.get(&finished_node).unwrap_or(&0);
                let finished_idx = *indices.get(&finished_node).unwrap_or(&0);

                if let Some((parent, _, _)) = dfs_stack.last() {
                    let parent_ll = *lowlinks.get(parent).unwrap_or(&u32::MAX);
                    lowlinks.insert(parent.clone(), parent_ll.min(finished_ll));
                }

                if finished_ll == finished_idx {
                    let mut scc = Vec::new();
                    while let Some(w) = stack.pop() {
                        on_stack.remove(&w);
                        scc.push(w.clone());
                        if w == finished_node {
                            break;
                        }
                    }
                    // Only report cycles (SCC with >1 member)
                    if scc.len() > 1 {
                        scc.reverse();
                        cycles.push(scc);
                    }
                }
            }
        }
    }

    cycles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cycle(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn rust_only_cycles_are_not_reported() {
        assert!(!is_reportable_cycle(&cycle(&[
            "crates/keel-core/src/a.rs",
            "crates/keel-core/src/b.rs",
        ])));
    }

    #[test]
    fn other_languages_still_report_cycles() {
        assert!(is_reportable_cycle(&cycle(&["src/a.ts", "src/b.ts"])));
        assert!(is_reportable_cycle(&cycle(&["app/a.py", "app/b.py"])));
        assert!(is_reportable_cycle(&cycle(&["pkg/a.go", "pkg/b.go"])));
    }

    #[test]
    fn mixed_language_cycles_are_reported() {
        // Not all-Rust: this one crosses a boundary no compiler polices.
        assert!(is_reportable_cycle(&cycle(&["src/a.rs", "src/b.py"])));
    }

    #[test]
    fn over_long_cycles_are_dropped() {
        let long: Vec<&str> = vec![
            "a.ts", "b.ts", "c.ts", "d.ts", "e.ts", "f.ts", "g.ts", "h.ts", "i.ts",
        ];
        assert_eq!(long.len(), MAX_ACTIONABLE_CYCLE + 1);
        assert!(!is_reportable_cycle(&cycle(&long)));
        assert!(is_reportable_cycle(&cycle(&long[..MAX_ACTIONABLE_CYCLE])));
    }
}
