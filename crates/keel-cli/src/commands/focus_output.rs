//! Output formatting for `keel focus` — text and JSON rendering.

use super::focus::{FocusFile, Relationship};

/// Compute read order: target first, then type deps, then callees, then callers.
pub(super) fn compute_read_order(files: &[FocusFile]) -> Vec<usize> {
    let mut targets = Vec::new();
    let mut callees = Vec::new();
    let mut callers = Vec::new();
    let mut type_deps = Vec::new();

    for (i, f) in files.iter().enumerate() {
        match f.relationship {
            Relationship::Target => targets.push(i + 1),
            Relationship::Callee => callees.push(i + 1),
            Relationship::TypeDep => type_deps.push(i + 1),
            Relationship::Caller => callers.push(i + 1),
        }
    }

    let mut order = Vec::new();
    order.extend(targets);
    order.extend(type_deps);
    order.extend(callees);
    order.extend(callers);
    order
}

/// Print human-readable text output.
pub(super) fn print_text(
    target_name: &str,
    target_hash: &str,
    files: &[FocusFile],
    total_symbols: usize,
    llm: bool,
) {
    println!(
        "FOCUS {} [{}] ({} files, {} symbols)",
        target_name,
        target_hash,
        files.len(),
        total_symbols,
    );
    println!();

    for (i, file) in files.iter().enumerate() {
        let rel_label = match file.relationship {
            Relationship::Target => "(target)".to_string(),
            other => {
                let count = file
                    .symbols
                    .iter()
                    .filter(|s| s.relationship == other)
                    .count();
                format!("({} {})", count, other.label())
            }
        };
        println!("  {}. {} {}", i + 1, file.path, rel_label);

        for sym in &file.symbols {
            let sig = if llm || !sym.node.signature.is_empty() {
                format!("  {}  [{}]", sym.node.signature, sym.node.hash)
            } else {
                format!("  [{}]", sym.node.hash)
            };

            let arrow = match sym.relationship {
                Relationship::Target => "",
                Relationship::Caller => "  <- calls target",
                Relationship::Callee => "  -> called by target",
                Relationship::TypeDep => "  ~ type dependency",
            };
            println!("     {} {}{}{}", sym.node.kind, sym.node.name, sig, arrow);
        }
        println!();
    }

    let read_order = compute_read_order(files);
    if read_order.len() > 1 {
        let order_str: Vec<String> = read_order.iter().map(|i| i.to_string()).collect();
        println!("  Read order: {}", order_str.join(" -> "));
    }
}

/// Print JSON output.
pub(super) fn print_json(target_name: &str, target_hash: &str, files: &[FocusFile]) {
    let file_values: Vec<serde_json::Value> = files
        .iter()
        .map(|file| {
            let syms: Vec<serde_json::Value> = file
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

    let read_order = compute_read_order(files);

    println!(
        "{}",
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "command": "focus",
            "target": target_name,
            "target_hash": target_hash,
            "file_count": files.len(),
            "files": file_values,
            "read_order": read_order,
        })
    );
}
