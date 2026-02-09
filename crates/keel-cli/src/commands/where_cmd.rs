use keel_output::OutputFormatter;

/// Run `keel where <hash>` — resolve hash to file:line.
pub fn run(_formatter: &dyn OutputFormatter, verbose: bool, hash: String) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel where: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel where: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(
        db_path.to_str().unwrap_or(""),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel where: failed to open graph database: {}", e);
            return 2;
        }
    };

    let engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));

    match engine.where_hash(&hash) {
        Some((file, line)) => {
            println!("{}:{}", file, line);
            0
        }
        None => {
            if verbose {
                eprintln!("keel where: hash {} not found", hash);
            }
            eprintln!("error: hash not found: {}", hash);
            2
        }
    }

    // formatter is available for --json/--llm modes in future
    // Currently where just outputs file:line directly
}
