use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind};
use keel_output::OutputFormatter;

/// Run `keel search <term>` — search the graph by function/class name.
pub fn run(
    _formatter: &dyn OutputFormatter,
    verbose: bool,
    json: bool,
    llm: bool,
    term: String,
    kind: Option<String>,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel search: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel search: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel search: failed to open graph database: {}", e);
            return 2;
        }
    };

    let kind_str = kind.as_deref().unwrap_or("");

    // Try exact name match first.
    let mut results = store.find_nodes_by_name(&term, kind_str, "");

    // If no exact matches, fall back to substring search across all modules.
    if results.is_empty() {
        if verbose {
            eprintln!("keel search: no exact match, trying substring search");
        }
        let modules = store.get_all_modules();
        let term_lower = term.to_lowercase();
        for module in &modules {
            let nodes = store.get_nodes_in_file(&module.file_path);
            for node in nodes {
                if node.name.to_lowercase().contains(&term_lower)
                    && (kind_str.is_empty() || node.kind.as_str() == kind_str)
                {
                    results.push(node);
                }
            }
        }
    }

    // Build result entries with caller/callee counts.
    let entries: Vec<serde_json::Value> = results
        .iter()
        .map(|node| {
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
            serde_json::json!({
                "name": node.name,
                "hash": node.hash,
                "file": node.file_path,
                "line": node.line_start,
                "kind": node.kind.as_str(),
                "callers": callers,
                "callees": callees,
            })
        })
        .collect();

    if json {
        println!(
            "{}",
            serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"), "command": "search",
                "term": term, "results": entries,
            })
        );
    } else {
        if llm {
            println!("SEARCH term={} results={}", term, entries.len());
        } else {
            println!("Search results for '{}' ({} found):", term, entries.len());
        }
        for e in &entries {
            println!(
                "  {} hash={} {}:{} callers={} callees={}",
                e["name"].as_str().unwrap_or(""),
                e["hash"].as_str().unwrap_or(""),
                e["file"].as_str().unwrap_or(""),
                e["line"],
                e["callers"],
                e["callees"]
            );
        }
    }

    0
}
