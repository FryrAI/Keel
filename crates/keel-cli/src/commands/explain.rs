use keel_output::OutputFormatter;

/// Run `keel explain <error_code> <hash>` — show resolution reasoning.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    error_code: String,
    hash: String,
    _tree: bool,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel explain: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel explain: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(
        db_path.to_str().unwrap_or(""),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel explain: failed to open graph database: {}", e);
            return 2;
        }
    };

    let engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));

    match engine.explain(&error_code, &hash) {
        Some(result) => {
            let output = formatter.format_explain(&result);
            if !output.is_empty() {
                println!("{}", output);
            }
            0
        }
        None => {
            if verbose {
                eprintln!("keel explain: hash {} not found", hash);
            }
            eprintln!("error: hash not found: {}", hash);
            2
        }
    }
}
