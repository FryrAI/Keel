use keel_output::OutputFormatter;

/// Run `keel explain <error_code> <hash>` — show resolution reasoning.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    error_code: String,
    hash: String,
    _tree: bool,
    depth: u32,
) -> i32 {
    let (_cwd, store) = match super::open_store("explain") {
        Ok(x) => x,
        Err(code) => return code,
    };

    let engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));

    match engine.explain(&error_code, &hash) {
        Some(mut result) => {
            // Truncate resolution chain by depth: 0=summary only, 1=first hop, 2=two hops, 3=full
            if depth < 3 {
                result.resolution_chain.truncate(depth as usize);
            }
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
