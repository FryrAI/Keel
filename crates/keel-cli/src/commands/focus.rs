//! `keel focus <query>` — compute minimal relevant file set for safe modification.
//!
//! Given a hash, file path, or function name, returns the ordered set of
//! files and symbols needed in context for safe modification.
//!
//! Output formatting lives in `focus_output.rs`.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, GraphNode, NodeKind};
use keel_output::OutputFormatter;

use super::focus_output;
use super::input_detect;

/// A symbol discovered during BFS traversal, with its relationship to the target.
pub(super) struct FocusSymbol {
    pub(super) node: GraphNode,
    pub(super) relationship: Relationship,
    pub(super) distance: u32,
    /// Number of edges connecting this symbol to the target subgraph.
    pub(super) connection_count: u32,
}

/// How a symbol relates to the focus target.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Relationship {
    Target,
    Caller,
    Callee,
    TypeDep,
}

impl Relationship {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Relationship::Target => "target",
            Relationship::Caller => "caller",
            Relationship::Callee => "callee",
            Relationship::TypeDep => "type_dep",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Relationship::Target => "target",
            Relationship::Caller => "callers",
            Relationship::Callee => "callees",
            Relationship::TypeDep => "type deps",
        }
    }
}

/// A file in the focus result, containing its relevant symbols.
pub(super) struct FocusFile {
    pub(super) path: String,
    pub(super) symbols: Vec<FocusSymbol>,
    /// Primary relationship of this file to the target.
    pub(super) relationship: Relationship,
    /// Relevance score: lower distance + more connections = higher score.
    pub(super) relevance: u32,
}

/// Run `keel focus <query>` — compute minimal relevant file set for editing.
#[allow(clippy::too_many_arguments)]
pub fn run(
    _formatter: &dyn OutputFormatter,
    verbose: bool,
    query: String,
    depth: u32,
    budget: Option<usize>,
    name_mode: bool,
    json: bool,
    llm: bool,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel focus: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel focus: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel focus: failed to open graph database: {}", e);
            return 2;
        }
    };

    let targets = resolve_targets(&store, &query, name_mode, &cwd);
    if targets.is_empty() {
        eprintln!("keel focus: no matching nodes found for '{}'", query);
        if let Some(hint) = input_detect::suggest_command(&query) {
            eprintln!("hint: {}", hint);
        }
        return 2;
    }

    if verbose {
        eprintln!(
            "keel focus: resolved '{}' to {} target node(s)",
            query,
            targets.len()
        );
    }

    let symbols = collect_focus_symbols(&store, &targets, depth);
    let mut files = group_and_rank(symbols, &targets);

    if let Some(max_tokens) = budget {
        truncate_by_budget(&mut files, max_tokens);
    }

    let total_symbols: usize = files.iter().map(|f| f.symbols.len()).sum();
    let target_name = targets.first().map(|n| n.name.as_str()).unwrap_or(&query);
    let target_hash = targets.first().map(|n| n.hash.as_str()).unwrap_or("");

    if json {
        focus_output::print_json(target_name, target_hash, &files);
    } else {
        focus_output::print_text(target_name, target_hash, &files, total_symbols, llm);
    }

    0
}

/// Resolve a query string to one or more target GraphNodes.
fn resolve_targets(
    store: &dyn GraphStore,
    query: &str,
    name_mode: bool,
    cwd: &std::path::Path,
) -> Vec<GraphNode> {
    if name_mode {
        let nodes = store.find_nodes_by_name(query, "", "");
        if nodes.len() > 1 {
            eprintln!(
                "keel focus: ambiguous name '{}' — {} matches, using first",
                query,
                nodes.len()
            );
        }
        return nodes.into_iter().take(1).collect();
    }

    if input_detect::looks_like_file_path(query) {
        let path = std::path::Path::new(query);
        let rel_path = if path.is_absolute() {
            path.strip_prefix(cwd)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string()
        } else {
            query.to_string()
        };
        let nodes = store.get_nodes_in_file(&rel_path);
        return nodes
            .into_iter()
            .filter(|n| n.kind != NodeKind::Module)
            .collect();
    }

    // Try as hash first
    if let Some(node) = store.get_node(query) {
        return vec![node];
    }

    // Fall back to name search
    let nodes = store.find_nodes_by_name(query, "", "");
    if nodes.len() > 1 {
        eprintln!(
            "keel focus: ambiguous name '{}' — {} matches, using first",
            query,
            nodes.len()
        );
    }
    nodes.into_iter().take(1).collect()
}

/// BFS traversal from target nodes, collecting callers, callees, and type deps.
fn collect_focus_symbols(
    store: &dyn GraphStore,
    targets: &[GraphNode],
    depth: u32,
) -> Vec<FocusSymbol> {
    let max_depth = depth.min(3);
    let mut result = Vec::new();
    let mut visited: HashSet<u64> = HashSet::new();
    let mut connection_counts: HashMap<u64, u32> = HashMap::new();

    // Add targets first
    for target in targets {
        visited.insert(target.id);
        result.push(FocusSymbol {
            node: target.clone(),
            relationship: Relationship::Target,
            distance: 0,
            connection_count: 0,
        });
    }

    // BFS for callers (incoming Calls edges)
    let mut queue: VecDeque<(u64, u32)> = VecDeque::new();
    for target in targets {
        queue.push_back((target.id, 0));
    }
    while let Some((current_id, current_depth)) = queue.pop_front() {
        if current_depth >= max_depth {
            continue;
        }
        let edges = store.get_edges(current_id, EdgeDirection::Incoming);
        for edge in &edges {
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
                    relationship: Relationship::Caller,
                    distance: current_depth + 1,
                    connection_count: 0,
                });
                queue.push_back((edge.source_id, current_depth + 1));
            }
        }
    }

    // BFS for callees (outgoing Calls edges)
    let mut queue: VecDeque<(u64, u32)> = VecDeque::new();
    for target in targets {
        queue.push_back((target.id, 0));
    }
    while let Some((current_id, current_depth)) = queue.pop_front() {
        if current_depth >= max_depth {
            continue;
        }
        let edges = store.get_edges(current_id, EdgeDirection::Outgoing);
        for edge in &edges {
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
                    relationship: Relationship::Callee,
                    distance: current_depth + 1,
                    connection_count: 0,
                });
                queue.push_back((edge.target_id, current_depth + 1));
            }
        }
    }

    // Collect type dependencies (Inherits edges, depth 1 only from targets)
    for target in targets {
        let edges = store.get_edges(target.id, EdgeDirection::Both);
        for edge in &edges {
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
                    relationship: Relationship::TypeDep,
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

/// Group symbols by file and compute relevance ranking.
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
                Relationship::Target
            } else {
                best_relationship(&syms)
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
        let a_target = a.relationship == Relationship::Target;
        let b_target = b.relationship == Relationship::Target;
        b_target.cmp(&a_target).then(b.relevance.cmp(&a.relevance))
    });

    files
}

/// Determine the best (most important) relationship among symbols in a file.
fn best_relationship(syms: &[FocusSymbol]) -> Relationship {
    if syms.iter().any(|s| s.relationship == Relationship::Target) {
        Relationship::Target
    } else if syms.iter().any(|s| s.relationship == Relationship::Caller) {
        Relationship::Caller
    } else if syms.iter().any(|s| s.relationship == Relationship::Callee) {
        Relationship::Callee
    } else {
        Relationship::TypeDep
    }
}

/// Truncate files list to stay within a token budget (len/4 heuristic).
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
