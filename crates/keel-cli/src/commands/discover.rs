use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, GraphNode};
use keel_enforce::types::{FileSymbol, FileSymbols};
use keel_output::OutputFormatter;

use super::input_detect;

/// Run `keel discover <query>` — accepts hash, file path, or --name.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    query: String,
    depth: u32,
    name_mode: bool,
    context_lines: Option<u32>,
) -> i32 {
    let (cwd, store) = match super::open_store("discover") {
        Ok(x) => x,
        Err(code) => return code,
    };

    // Name lookup mode: --name flag
    if name_mode {
        return discover_by_name(formatter, &store, &query, verbose);
    }

    // File path mode: auto-detected
    if input_detect::looks_like_file_path(&query) {
        return discover_file(formatter, &store, &query, &cwd, verbose);
    }

    // Hash mode: existing behavior
    let engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));
    match engine.discover(&query, depth) {
        Some(mut result) => {
            // Add body context if --context was requested
            if let Some(max_lines) = context_lines {
                result.body_context = read_body_context(
                    &cwd,
                    &result.target.file,
                    result.target.line_start,
                    result.target.line_end,
                    max_lines,
                );
            }
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

/// Read source code lines for a function body.
fn read_body_context(
    cwd: &std::path::Path,
    file_path: &str,
    line_start: u32,
    line_end: u32,
    max_lines: u32,
) -> Option<keel_enforce::types::BodyContext> {
    let full_path = cwd.join(file_path);
    let content = std::fs::read_to_string(&full_path).ok()?;
    let all_lines: Vec<&str> = content.lines().collect();

    let start = (line_start as usize).saturating_sub(1);
    let end = (line_end as usize).min(all_lines.len());
    if start >= all_lines.len() || start >= end {
        return None;
    }

    let body_lines = &all_lines[start..end];
    let total = body_lines.len() as u32;
    let truncated = total > max_lines;
    let take = (max_lines as usize).min(body_lines.len());

    let lines: String = body_lines[..take]
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>4} | {}", start + i + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    Some(keel_enforce::types::BodyContext {
        lines,
        line_count: total,
        truncated,
    })
}

/// Build a `FileSymbol` for a node, computing its call adjacency counts.
fn to_file_symbol(store: &dyn GraphStore, node: &GraphNode) -> FileSymbol {
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
    FileSymbol {
        kind: node.kind.to_string(),
        name: node.name.clone(),
        hash: node.hash.clone(),
        file: node.file_path.clone(),
        line: node.line_start,
        callers: callers as u32,
        callees: callees as u32,
    }
}

/// List all symbols in a file with their hashes, honoring the output format.
fn discover_file(
    formatter: &dyn OutputFormatter,
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

    let symbols: Vec<FileSymbol> = nodes
        .iter()
        .filter(|n| n.kind != keel_core::types::NodeKind::Module)
        .map(|node| to_file_symbol(store, node))
        .collect();

    if verbose {
        eprintln!("keel discover: {} symbols in {}", symbols.len(), rel_path);
    }

    let result = FileSymbols {
        version: env!("CARGO_PKG_VERSION").into(),
        command: "discover".into(),
        path: Some(rel_path),
        symbols,
    };
    let out = formatter.format_file_symbols(&result);
    if !out.is_empty() {
        println!("{}", out.trim_end());
    }
    0
}

/// Look up a function by name and show its hash and location, honoring the output format.
fn discover_by_name(
    formatter: &dyn OutputFormatter,
    store: &dyn GraphStore,
    name: &str,
    _verbose: bool,
) -> i32 {
    let nodes = store.find_nodes_by_name(name, "", "");
    if nodes.is_empty() {
        eprintln!("keel discover: no function named '{}' found", name);
        return 2;
    }

    let symbols: Vec<FileSymbol> = nodes
        .iter()
        .map(|node| to_file_symbol(store, node))
        .collect();
    let result = FileSymbols {
        version: env!("CARGO_PKG_VERSION").into(),
        command: "discover".into(),
        path: None,
        symbols,
    };
    let out = formatter.format_file_symbols(&result);
    if !out.is_empty() {
        println!("{}", out.trim_end());
    }
    0
}
