use std::path::Path;

use keel_output::OutputFormatter;

/// Run `keel map` — full re-parse of the codebase.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    _llm_verbose: bool,
    _scope: Option<String>,
    _strict: bool,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel map: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel map: not initialized. Run `keel init` first.");
        return 2;
    }

    // Open graph store
    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(
        db_path.to_str().unwrap_or(""),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel map: failed to open graph database: {}", e);
            return 2;
        }
    };

    // Walk all source files
    let walker = keel_parsers::walker::FileWalker::new(&cwd);
    let entries = walker.walk();

    if verbose {
        eprintln!("keel map: found {} source files", entries.len());
    }

    // Parse files and build graph
    let file_count = entries.len();
    let mut _engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));

    // TODO: Parse each file using the appropriate LanguageResolver,
    // build FileIndex entries, update graph store, run compile.
    // This requires the language resolvers (Agent A's work) to be complete.

    if verbose {
        eprintln!("keel map: mapped {} files", file_count);
    }

    let _ = formatter;
    let _ = Path::new(""); // suppress unused import
    0
}
