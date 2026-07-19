use keel_output::OutputFormatter;

/// Upper bound on results returned by `keel search`, so a broad substring
/// term can't dump the entire graph.
const SEARCH_LIMIT: usize = 100;

/// Run `keel search <term>` — search the graph by function/class name.
pub fn run(
    _formatter: &dyn OutputFormatter,
    verbose: bool,
    json: bool,
    llm: bool,
    term: String,
    kind: Option<String>,
) -> i32 {
    let (_cwd, store) = match super::open_store("search") {
        Ok(x) => x,
        Err(code) => return code,
    };

    // Route through the shared search implementation so the CLI, the MCP
    // `keel/search` tool, and the HTTP `/search` route all rank results
    // identically (exact-match-first, then substring, capped at `limit`).
    let results = keel_enforce::queries::search_graph(&store, &term, kind.as_deref(), SEARCH_LIMIT);
    if verbose && results.is_empty() {
        eprintln!("keel search: no matches for '{}'", term);
    }

    // Build result entries with caller/callee counts.
    let entries: Vec<serde_json::Value> = results
        .iter()
        .map(|node| {
            let callers = keel_enforce::queries::call_fan_in(&store, node.id);
            let callees = keel_enforce::queries::call_fan_out(&store, node.id);
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
