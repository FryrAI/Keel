use keel_output::OutputFormatter;

/// Run `keel analyze <file>` — architectural observations from graph data.
pub fn run(formatter: &dyn OutputFormatter, verbose: bool, file: String) -> i32 {
    let (cwd, store) = match super::open_store("analyze") {
        Ok(x) => x,
        Err(code) => return code,
    };

    // Normalize file path to relative (matching how nodes are stored).
    let rel_path = keel_core::paths::make_relative(&cwd, std::path::Path::new(&file));

    match keel_enforce::analyze::analyze_file(&store, &rel_path) {
        Some(result) => {
            if verbose {
                eprintln!(
                    "keel analyze: {} — {} functions, {} classes, {} smells",
                    rel_path,
                    result.structure.function_count,
                    result.structure.class_count,
                    result.smells.len(),
                );
            }
            let output = formatter.format_analyze(&result);
            if !output.is_empty() {
                println!("{}", output);
            }
            0
        }
        None => {
            eprintln!("keel analyze: no data for file: {}", rel_path);
            eprintln!("hint: Run `keel map` first to populate the graph.");
            2
        }
    }
}
