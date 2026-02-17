use keel_output::OutputFormatter;

/// Run `keel where <hash>` — resolve hash to file:line.
pub fn run(_formatter: &dyn OutputFormatter, verbose: bool, hash: String, json: bool) -> i32 {
    eprintln!("hint: `keel where` is deprecated. Use `keel discover --name <name>` or `keel discover <hash>` instead.");
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
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "version": "0.1.0",
                        "command": "where",
                        "hash": hash,
                        "file": file,
                        "line": line
                    })
                );
            } else {
                println!("{}:{}", file, line);
            }
            0
        }
        None => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "version": "0.1.0",
                        "command": "where",
                        "hash": hash,
                        "error": "hash not found"
                    })
                );
            } else {
                if verbose {
                    eprintln!("keel where: hash {} not found", hash);
                }
                eprintln!("error: hash not found: {}", hash);
            }
            2
        }
    }
}
