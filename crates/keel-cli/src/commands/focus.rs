//! `keel focus <hash|file>` — the minimal context set for safely modifying a
//! target: transitive callers, direct callees, and the files containing them.
//!
//! Graph-backed (like `discover`). Delegates to `EnforcementEngine::focus` so
//! the CLI and the `keel/focus` MCP tool share one implementation.

use keel_output::OutputFormatter;

/// Run `keel focus <target>`.
pub fn run(formatter: &dyn OutputFormatter, verbose: bool, target: String, depth: u32) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel focus: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = keel_core::paths::keel_dir(&cwd);
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

    // Normalize a file target to the relative path the graph stores.
    let query = normalize_target(&target, &cwd);

    let engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));
    match engine.focus(&query, depth) {
        Some(result) => {
            if verbose {
                eprintln!(
                    "keel focus: {} — {} files, {} callers at risk",
                    result.target,
                    result.files.len(),
                    result.callers.len(),
                );
            }
            let output = formatter.format_focus(&result);
            if !output.is_empty() {
                println!("{}", output.trim_end());
            }
            0
        }
        None => {
            eprintln!(
                "keel focus: no node or file found for '{}'. Pass a hash (see `keel discover <file>`) or a mapped file path.",
                target
            );
            2
        }
    }
}

/// If `target` is an absolute path inside the repo, make it relative so it
/// matches how the graph stores file paths; otherwise pass it through (hashes
/// and relative paths are already in the right shape).
fn normalize_target(target: &str, cwd: &std::path::Path) -> String {
    let path = std::path::Path::new(target);
    if path.is_absolute() {
        path.strip_prefix(cwd)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    } else {
        target.to_string()
    }
}
