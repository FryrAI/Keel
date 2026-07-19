use keel_output::OutputFormatter;

/// Run `keel where <hash>` — resolve hash to file:line.
pub fn run(_formatter: &dyn OutputFormatter, verbose: bool, hash: String, json: bool) -> i32 {
    eprintln!("hint: `keel where` is deprecated. Use `keel discover --name <name>` or `keel discover <hash>` instead.");
    let (_cwd, store) = match super::open_store("where") {
        Ok(x) => x,
        Err(code) => return code,
    };

    let engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));

    match engine.where_hash(&hash) {
        Some((file, line)) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "version": env!("CARGO_PKG_VERSION"),
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
                        "version": env!("CARGO_PKG_VERSION"),
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
