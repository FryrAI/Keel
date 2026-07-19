//! `keel focus <hash|file>` — the minimal context set for safely modifying a
//! target: transitive callers, direct callees, and the files containing them.
//!
//! Graph-backed (like `discover`). Delegates to `EnforcementEngine::focus` so
//! the CLI and the `keel/focus` MCP tool share one implementation.

use keel_output::OutputFormatter;

/// Run `keel focus <target>`.
pub fn run(formatter: &dyn OutputFormatter, verbose: bool, target: String, depth: u32) -> i32 {
    let (cwd, store) = match super::open_store("focus") {
        Ok(x) => x,
        Err(code) => return code,
    };

    // Normalize a file target to the relative path the graph stores. A hash or
    // already-relative path passes through unchanged (strip_prefix is a no-op).
    let query = keel_core::paths::make_relative(&cwd, std::path::Path::new(&target));

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
