use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind};
use keel_output::OutputFormatter;

use super::input_detect;

/// Run `keel discover <query>` — accepts hash, file path, or --name.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    query: String,
    depth: u32,
    _suggest_placement: bool,
    name_mode: bool,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel discover: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel discover: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(
        db_path.to_str().unwrap_or(""),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel discover: failed to open graph database: {}", e);
            return 2;
        }
    };

    // Name lookup mode: --name flag
    if name_mode {
        return discover_by_name(&store, &query, verbose);
    }

    // File path mode: auto-detected
    if input_detect::looks_like_file_path(&query) {
        return discover_file(&store, &query, &cwd, verbose);
    }

    // Hash mode: existing behavior
    let engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));
    match engine.discover(&query, depth) {
        Some(result) => {
            let output = formatter.format_discover(&result);
            if !output.is_empty() {
                println!("{}", output);
            }
            0
        }
        None => {
            if let Some(hint) = input_detect::suggest_command(&query) {
                eprintln!("error: hash not found: {}\nhint: {}", query, hint);
            } else {
                eprintln!("error: hash not found: {}", query);
            }
            2
        }
    }
}

/// List all symbols in a file with their hashes.
fn discover_file(
    store: &dyn GraphStore,
    query: &str,
    cwd: &std::path::Path,
    verbose: bool,
) -> i32 {
    // Normalize the file path to be relative (matching how nodes are stored)
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
    if nodes.is_empty() {
        eprintln!("keel discover: no nodes found in file: {}", rel_path);
        return 2;
    }

    if verbose {
        eprintln!("keel discover: {} symbols in {}", nodes.len(), rel_path);
    }

    println!("FILE {} symbols={}", rel_path, nodes.len());
    for node in &nodes {
        if node.kind == keel_core::types::NodeKind::Module {
            continue;
        }
        let callers = store
            .get_edges(node.id, EdgeDirection::Incoming)
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .count();
        let callees = store
            .get_edges(node.id, EdgeDirection::Outgoing)
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .count();
        println!(
            "  {} {} hash={} line={} callers={} callees={}",
            node.kind, node.name, node.hash, node.line_start, callers, callees,
        );
    }
    0
}

/// Look up a function by name and show its hash and location.
fn discover_by_name(store: &dyn GraphStore, name: &str, _verbose: bool) -> i32 {
    let nodes = store.find_nodes_by_name(name, "", "");
    if nodes.is_empty() {
        eprintln!("keel discover: no function named '{}' found", name);
        return 2;
    }

    for node in &nodes {
        let callers = store
            .get_edges(node.id, EdgeDirection::Incoming)
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .count();
        let callees = store
            .get_edges(node.id, EdgeDirection::Outgoing)
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .count();
        println!(
            "{} hash={} {}:{} callers={} callees={}",
            node.name, node.hash, node.file_path, node.line_start, callers, callees,
        );
    }
    0
}
