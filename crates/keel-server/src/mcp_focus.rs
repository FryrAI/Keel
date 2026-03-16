//! MCP focus handler — compute minimal relevant file set for safe modification.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde_json::Value;

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, GraphNode, NodeKind};

use crate::mcp::{lock_store, missing_param, not_found, JsonRpcError, SharedStore};

/// Handle the `keel/focus` MCP tool call.
pub(crate) fn handle_focus(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let query = params
        .as_ref()
        .and_then(|p| p.get("query"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| missing_param("query"))?
        .to_string();

    let depth = params
        .as_ref()
        .and_then(|p| p.get("depth"))
        .and_then(|v| v.as_u64())
        .unwrap_or(2) as u32;

    let budget = params
        .as_ref()
        .and_then(|p| p.get("budget"))
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let name_mode = params
        .as_ref()
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let store = lock_store(store)?;
    let targets = resolve_targets(&*store, &query, name_mode);

    if targets.is_empty() {
        return Err(not_found(&query));
    }

    let symbols = collect_focus_symbols(&*store, &targets, depth);
    let mut files = group_and_rank(symbols, &targets);

    if let Some(max_tokens) = budget {
        truncate_by_budget(&mut files, max_tokens);
    }

    let target_name = targets.first().map(|n| n.name.as_str()).unwrap_or(&query);
    let target_hash = targets.first().map(|n| n.hash.as_str()).unwrap_or("");
    let read_order = compute_read_order(&files);

    let file_values: Vec<Value> = files
        .iter()
        .map(|file| {
            let syms: Vec<Value> = file
                .symbols
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "name": s.node.name,
                        "hash": s.node.hash,
                        "kind": s.node.kind.as_str(),
                        "signature": s.node.signature,
                        "line_start": s.node.line_start,
                        "line_end": s.node.line_end,
                        "is_public": s.node.is_public,
                        "relationship": s.relationship.as_str(),
                        "distance": s.distance,
                        "connection_count": s.connection_count,
                    })
                })
                .collect();
            serde_json::json!({
                "path": file.path,
                "relationship": file.relationship.as_str(),
                "relevance": file.relevance,
                "symbols": syms,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "command": "focus",
        "target": target_name,
        "target_hash": target_hash,
        "file_count": files.len(),
        "files": file_values,
        "read_order": read_order,
    }))
}

// --- Internal types ---

struct FocusSymbol {
    node: GraphNode,
    relationship: Rel,
    distance: u32,
    connection_count: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Rel {
    Target,
    Caller,
    Callee,
    TypeDep,
}

impl Rel {
    fn as_str(self) -> &'static str {
        match self {
            Rel::Target => "target",
            Rel::Caller => "caller",
            Rel::Callee => "callee",
            Rel::TypeDep => "type_dep",
        }
    }
}

struct FocusFile {
    path: String,
    symbols: Vec<FocusSymbol>,
    relationship: Rel,
    relevance: u32,
}

// --- Query resolution ---

fn looks_like_file_path(s: &str) -> bool {
    s.contains('/')
        || s.contains('\\')
        || s.ends_with(".py")
        || s.ends_with(".ts")
        || s.ends_with(".tsx")
        || s.ends_with(".js")
        || s.ends_with(".jsx")
        || s.ends_with(".go")
        || s.ends_with(".rs")
}

fn resolve_targets(store: &dyn GraphStore, query: &str, name_mode: bool) -> Vec<GraphNode> {
    if name_mode {
        return store
            .find_nodes_by_name(query, "", "")
            .into_iter()
            .take(1)
            .collect();
    }

    if looks_like_file_path(query) {
        return store
            .get_nodes_in_file(query)
            .into_iter()
            .filter(|n| n.kind != NodeKind::Module)
            .collect();
    }

    if let Some(node) = store.get_node(query) {
        return vec![node];
    }

    store
        .find_nodes_by_name(query, "", "")
        .into_iter()
        .take(1)
        .collect()
}

// --- BFS traversal ---

fn collect_focus_symbols(
    store: &dyn GraphStore,
    targets: &[GraphNode],
    depth: u32,
) -> Vec<FocusSymbol> {
    let max_depth = depth.min(3);
    let mut result = Vec::new();
    let mut visited: HashSet<u64> = HashSet::new();
    let mut connection_counts: HashMap<u64, u32> = HashMap::new();

    for target in targets {
        visited.insert(target.id);
        result.push(FocusSymbol {
            node: target.clone(),
            relationship: Rel::Target,
            distance: 0,
            connection_count: 0,
        });
    }

    // Callers BFS
    let mut queue: VecDeque<(u64, u32)> = targets.iter().map(|t| (t.id, 0u32)).collect();
    while let Some((current_id, current_depth)) = queue.pop_front() {
        if current_depth >= max_depth {
            continue;
        }
        for edge in &store.get_edges(current_id, EdgeDirection::Incoming) {
            if edge.kind != EdgeKind::Calls {
                continue;
            }
            *connection_counts.entry(edge.source_id).or_insert(0) += 1;
            if visited.contains(&edge.source_id) {
                continue;
            }
            visited.insert(edge.source_id);
            if let Some(caller) = store.get_node_by_id(edge.source_id) {
                result.push(FocusSymbol {
                    node: caller,
                    relationship: Rel::Caller,
                    distance: current_depth + 1,
                    connection_count: 0,
                });
                queue.push_back((edge.source_id, current_depth + 1));
            }
        }
    }

    // Callees BFS
    let mut queue: VecDeque<(u64, u32)> = targets.iter().map(|t| (t.id, 0u32)).collect();
    while let Some((current_id, current_depth)) = queue.pop_front() {
        if current_depth >= max_depth {
            continue;
        }
        for edge in &store.get_edges(current_id, EdgeDirection::Outgoing) {
            if edge.kind != EdgeKind::Calls {
                continue;
            }
            *connection_counts.entry(edge.target_id).or_insert(0) += 1;
            if visited.contains(&edge.target_id) {
                continue;
            }
            visited.insert(edge.target_id);
            if let Some(callee) = store.get_node_by_id(edge.target_id) {
                result.push(FocusSymbol {
                    node: callee,
                    relationship: Rel::Callee,
                    distance: current_depth + 1,
                    connection_count: 0,
                });
                queue.push_back((edge.target_id, current_depth + 1));
            }
        }
    }

    // Type dependencies (Inherits edges, depth 1 only)
    for target in targets {
        for edge in &store.get_edges(target.id, EdgeDirection::Both) {
            if edge.kind != EdgeKind::Inherits {
                continue;
            }
            let dep_id = if edge.source_id == target.id {
                edge.target_id
            } else {
                edge.source_id
            };
            *connection_counts.entry(dep_id).or_insert(0) += 1;
            if visited.contains(&dep_id) {
                continue;
            }
            visited.insert(dep_id);
            if let Some(dep) = store.get_node_by_id(dep_id) {
                result.push(FocusSymbol {
                    node: dep,
                    relationship: Rel::TypeDep,
                    distance: 1,
                    connection_count: 0,
                });
            }
        }
    }

    // Backfill connection counts
    for sym in &mut result {
        if let Some(&count) = connection_counts.get(&sym.node.id) {
            sym.connection_count = count;
        }
    }

    result
}

// --- Grouping and ranking ---

fn group_and_rank(symbols: Vec<FocusSymbol>, targets: &[GraphNode]) -> Vec<FocusFile> {
    let mut file_map: BTreeMap<String, Vec<FocusSymbol>> = BTreeMap::new();
    for sym in symbols {
        file_map
            .entry(sym.node.file_path.clone())
            .or_default()
            .push(sym);
    }

    let target_files: HashSet<&str> = targets.iter().map(|t| t.file_path.as_str()).collect();

    let mut files: Vec<FocusFile> = file_map
        .into_iter()
        .map(|(path, syms)| {
            let relationship = if target_files.contains(path.as_str()) {
                Rel::Target
            } else if syms.iter().any(|s| s.relationship == Rel::Caller) {
                Rel::Caller
            } else if syms.iter().any(|s| s.relationship == Rel::Callee) {
                Rel::Callee
            } else {
                Rel::TypeDep
            };

            let relevance: u32 = syms
                .iter()
                .map(|s| 4u32.saturating_sub(s.distance) * s.connection_count.max(1))
                .sum();

            FocusFile {
                path,
                symbols: syms,
                relationship,
                relevance,
            }
        })
        .collect();

    files.sort_by(|a, b| {
        let a_target = a.relationship == Rel::Target;
        let b_target = b.relationship == Rel::Target;
        b_target.cmp(&a_target).then(b.relevance.cmp(&a.relevance))
    });

    files
}

fn truncate_by_budget(files: &mut Vec<FocusFile>, max_tokens: usize) {
    let mut used = 0usize;
    let mut keep = 0usize;
    for file in files.iter() {
        let mut chars = file.path.len() + 20;
        for sym in &file.symbols {
            chars += sym.node.name.len() + sym.node.signature.len() + sym.node.hash.len() + 30;
        }
        let file_tokens = chars / 4;
        if used + file_tokens > max_tokens && keep > 0 {
            break;
        }
        used += file_tokens;
        keep += 1;
    }
    files.truncate(keep);
}

fn compute_read_order(files: &[FocusFile]) -> Vec<usize> {
    let mut targets = Vec::new();
    let mut callees = Vec::new();
    let mut callers = Vec::new();
    let mut type_deps = Vec::new();

    for (i, f) in files.iter().enumerate() {
        match f.relationship {
            Rel::Target => targets.push(i + 1),
            Rel::Callee => callees.push(i + 1),
            Rel::TypeDep => type_deps.push(i + 1),
            Rel::Caller => callers.push(i + 1),
        }
    }

    let mut order = Vec::new();
    order.extend(targets);
    order.extend(type_deps);
    order.extend(callees);
    order.extend(callers);
    order
}
