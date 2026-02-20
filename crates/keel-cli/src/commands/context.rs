use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, GraphNode, NodeKind};
use keel_output::OutputFormatter;

/// A symbol in the target file with its external callers and callees.
struct NodeContext {
    node: GraphNode,
    /// (name, file_path, line) tuples for external callers
    callers: Vec<(String, String, u32)>,
    /// (name, file_path, line) tuples for external callees
    callees: Vec<(String, String, u32)>,
}

/// Run `keel context <file>` — minimal structural context for safe editing.
///
/// For each non-module symbol in the file, collects external (cross-file) callers
/// and callees. Output is either compact text or JSON.
pub fn run(
    _formatter: &dyn OutputFormatter,
    verbose: bool,
    file: String,
    json: bool,
    llm: bool,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel context: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel context: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel context: failed to open graph database: {}", e);
            return 2;
        }
    };

    // Normalize to relative path
    let path = std::path::Path::new(&file);
    let rel_path = if path.is_absolute() {
        path.strip_prefix(&cwd)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    } else {
        file.clone()
    };

    let nodes = store.get_nodes_in_file(&rel_path);
    if nodes.is_empty() {
        eprintln!("keel context: no data for file: {}", rel_path);
        eprintln!("hint: Run `keel map` first to populate the graph.");
        return 2;
    }

    let contexts = collect_contexts(&store, &rel_path, &nodes);
    let total_callers: usize = contexts.iter().map(|c| c.callers.len()).sum();
    let total_callees: usize = contexts.iter().map(|c| c.callees.len()).sum();

    if verbose {
        eprintln!(
            "keel context: {} — {} symbols, {} ext callers, {} ext callees",
            rel_path,
            contexts.len(),
            total_callers,
            total_callees,
        );
    }

    if json {
        print_json(&rel_path, &contexts);
    } else {
        print_text(&rel_path, &contexts, total_callers, total_callees, llm);
    }

    0
}

fn collect_contexts(
    store: &dyn GraphStore,
    file_path: &str,
    nodes: &[GraphNode],
) -> Vec<NodeContext> {
    nodes
        .iter()
        .filter(|n| n.kind != NodeKind::Module)
        .map(|node| {
            let incoming = store.get_edges(node.id, EdgeDirection::Incoming);
            let outgoing = store.get_edges(node.id, EdgeDirection::Outgoing);

            let callers: Vec<_> = incoming
                .iter()
                .filter_map(|e| {
                    let src = store.get_node_by_id(e.source_id)?;
                    if src.file_path == file_path {
                        return None; // skip internal
                    }
                    Some((src.name, src.file_path, src.line_start))
                })
                .collect();

            let callees: Vec<_> = outgoing
                .iter()
                .filter_map(|e| {
                    let tgt = store.get_node_by_id(e.target_id)?;
                    if tgt.file_path == file_path {
                        return None; // skip internal
                    }
                    Some((tgt.name, tgt.file_path, tgt.line_start))
                })
                .collect();

            NodeContext {
                node: node.clone(),
                callers,
                callees,
            }
        })
        .collect()
}

fn print_json(file_path: &str, contexts: &[NodeContext]) {
    let symbols: Vec<serde_json::Value> = contexts
        .iter()
        .map(|ctx| {
            let callers: Vec<serde_json::Value> = ctx
                .callers
                .iter()
                .map(|(name, fp, line)| {
                    serde_json::json!({ "name": name, "file": fp, "line": line })
                })
                .collect();
            let callees: Vec<serde_json::Value> = ctx
                .callees
                .iter()
                .map(|(name, fp, line)| {
                    serde_json::json!({ "name": name, "file": fp, "line": line })
                })
                .collect();
            serde_json::json!({
                "name": ctx.node.name,
                "hash": ctx.node.hash,
                "kind": ctx.node.kind.as_str(),
                "line_start": ctx.node.line_start,
                "line_end": ctx.node.line_end,
                "is_public": ctx.node.is_public,
                "signature": ctx.node.signature,
                "callers": callers,
                "callees": callees,
            })
        })
        .collect();

    println!(
        "{}",
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "command": "context",
            "file": file_path,
            "symbols": symbols,
        })
    );
}

fn print_text(
    file_path: &str,
    contexts: &[NodeContext],
    total_callers: usize,
    total_callees: usize,
    llm: bool,
) {
    println!(
        "CONTEXT {} ({} symbols, {} ext callers, {} ext callees)",
        file_path,
        contexts.len(),
        total_callers,
        total_callees,
    );
    for ctx in contexts {
        println!(
            "  {} hash={} L{}-L{} pub={}",
            ctx.node.name,
            ctx.node.hash,
            ctx.node.line_start,
            ctx.node.line_end,
            ctx.node.is_public,
        );
        if !ctx.callers.is_empty() {
            let refs: Vec<String> = ctx
                .callers
                .iter()
                .map(|(name, fp, line)| format!("{}[{}:{}]", name, fp, line))
                .collect();
            println!("    CALLERS: {}", refs.join(" "));
        }
        if !ctx.callees.is_empty() {
            let refs: Vec<String> = ctx
                .callees
                .iter()
                .map(|(name, fp, line)| format!("{}[{}:{}]", name, fp, line))
                .collect();
            println!("    CALLEES: {}", refs.join(" "));
        }
        if llm && !ctx.node.signature.is_empty() {
            println!("    SIG: {}", ctx.node.signature);
        }
    }
}
