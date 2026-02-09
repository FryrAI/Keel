use keel_output::OutputFormatter;

/// Run `keel discover <hash>` — look up callers, callees, and context.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    hash: String,
    depth: u32,
    _suggest_placement: bool,
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

    let engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));

    match engine.discover(&hash, depth) {
        Some(result) => {
            let output = formatter.format_discover(&result);
            if !output.is_empty() {
                println!("{}", output);
            }
            0
        }
        None => {
            if verbose {
                eprintln!("keel discover: hash {} not found", hash);
            }
            eprintln!("error: hash not found: {}", hash);
            2
        }
    }
}
