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

    let root = keel_core::paths::project_root(&cwd);

    // Resolve file arguments from the cwd; hashes keep their graph identity.
    let query = if !super::input_detect::looks_like_hash(&target) {
        keel_core::paths::make_relative(&root, &cwd.join(&target))
    } else {
        target.clone()
    };

    let engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));
    match engine.focus(&query, depth) {
        Some(mut result) => {
            result.target = super::file_display::graph_argument(&cwd, &root, &target, &query);
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
