//! Navigation dimension — circular deps, coupling, unstable APIs, orphans, deep chains.

use std::collections::{HashMap, HashSet, VecDeque};

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};

use crate::types::{AuditFinding, AuditSeverity};

pub fn check_navigation(
    store: &dyn GraphStore,
    files: Option<&[String]>,
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
                if edge.kind == EdgeKind::Calls || edge.kind == EdgeKind::Imports {
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
        let chain = cycle.join(" → ");
        findings.push(AuditFinding {
            severity: AuditSeverity::Warn,
            check: "circular_dep".into(),
            message: format!("Circular dependency: {}", chain),
            tip: Some("Extract shared types to a common module to break the cycle".into()),
            file: None,
            count: None,
        });
    }

    // High cross-module coupling: module imports >10 others
    for (path, deps) in &module_deps {
        if deps.len() > 10 {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warn,
                check: "high_coupling".into(),
                message: format!("{} imports {} other modules (>10)", path, deps.len()),
                tip: Some("Reduce coupling by introducing facade modules or interfaces".into()),
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

            let external_incoming = incoming
                .iter()
                .any(|e| {
                    store
                        .get_node_by_id(e.source_id)
                        .map(|n| n.file_path != module.file_path)
                        .unwrap_or(false)
                });
            let external_outgoing = outgoing
                .iter()
                .any(|e| {
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
                    tip: Some(
                        "High-use public functions need docstrings to prevent misuse by agents"
                            .into(),
                    ),
                    file: Some(module.file_path.clone()),
                    count: None,
                });
            }

            // Deep call chain detection (BFS from functions with 0 incoming calls)
            if node.kind == NodeKind::Function && call_callers == 0 {
                let max_depth = bfs_max_depth(store, node.id);
                if max_depth > 7 {
                    findings.push(AuditFinding {
                        severity: AuditSeverity::Tip,
                        check: "deep_call_chain".into(),
                        message: format!(
                            "`{}` in {} — call chain depth {} (>7 hops)",
                            node.name, module.file_path, max_depth,
                        ),
                        tip: Some(
                            "Deep call chains make it hard for agents to trace execution flow"
                                .into(),
                        ),
                        file: Some(module.file_path.clone()),
                        count: None,
                    });
                }
            }
        }

        // Orphan file
        if !has_incoming && !has_outgoing {
            let non_module = nodes.iter().any(|n| n.kind != NodeKind::Module);
            if non_module {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Tip,
                    check: "orphan_file".into(),
                    message: format!(
                        "{} — no external dependencies in either direction",
                        module.file_path,
                    ),
                    tip: Some("Orphan files may be dead code or poorly integrated".into()),
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
            break;
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
fn find_cycles(graph: &HashMap<String, HashSet<String>>) -> Vec<Vec<String>> {
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
