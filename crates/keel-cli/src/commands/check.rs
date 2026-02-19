use keel_core::store::GraphStore;
use keel_output::OutputFormatter;

use super::input_detect;

/// Run `keel check <query>` — pre-edit risk assessment.
pub fn run(formatter: &dyn OutputFormatter, verbose: bool, query: String, name_mode: bool) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel check: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel check: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel check: failed to open graph database: {}", e);
            return 2;
        }
    };

    // Resolve query to a hash
    let hash = if name_mode {
        // Name lookup mode
        let nodes = store.find_nodes_by_name(&query, "", "");
        match nodes.len() {
            0 => {
                eprintln!("keel check: no function named '{}' found", query);
                return 2;
            }
            1 => nodes[0].hash.clone(),
            _ => {
                eprintln!(
                    "keel check: ambiguous name '{}' — {} matches:",
                    query,
                    nodes.len()
                );
                for n in &nodes {
                    eprintln!(
                        "  {} hash={} {}:{}",
                        n.name, n.hash, n.file_path, n.line_start
                    );
                }
                eprintln!("Use the hash directly: keel check <hash>");
                return 2;
            }
        }
    } else if input_detect::looks_like_file_path(&query) {
        eprintln!("keel check: file paths not supported. Use a hash or --name <function_name>");
        return 2;
    } else if input_detect::looks_like_hash(&query) {
        query
    } else {
        // Auto-detect: try as name first, then as hash
        let nodes = store.find_nodes_by_name(&query, "", "");
        match nodes.len() {
            0 => query, // Try as hash
            1 => nodes[0].hash.clone(),
            _ => {
                eprintln!(
                    "keel check: ambiguous name '{}' — {} matches:",
                    query,
                    nodes.len()
                );
                for n in &nodes {
                    eprintln!(
                        "  {} hash={} {}:{}",
                        n.name, n.hash, n.file_path, n.line_start
                    );
                }
                eprintln!("Use the hash directly: keel check <hash>");
                return 2;
            }
        }
    };

    let engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));
    match engine.check(&hash) {
        Some(result) => {
            if verbose {
                eprintln!(
                    "keel check: risk={} callers={}",
                    result.risk.level, result.risk.caller_count
                );
            }
            let output = formatter.format_check(&result);
            if !output.is_empty() {
                println!("{}", output);
            }
            0
        }
        None => {
            eprintln!("keel check: hash not found: {}", hash);
            2
        }
    }
}
