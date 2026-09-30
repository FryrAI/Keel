use keel_output::OutputFormatter;

/// Run `keel analyze <file>` — architectural observations from graph data.
pub fn run(formatter: &dyn OutputFormatter, verbose: bool, file: String) -> i32 {
    let (cwd, store) = match super::open_store("analyze") {
        Ok(x) => x,
        Err(code) => return code,
    };

    // Normalize file path to relative (matching how nodes are stored).
    let root = keel_core::paths::project_root(&cwd);
    let rel_path = keel_core::paths::make_relative(&root, &cwd.join(&file));
    let display = super::file_display::graph_argument(&cwd, &root, &file, &rel_path);

    match keel_enforce::analyze::analyze_file(&store, &rel_path) {
        Some(mut result) => {
            result.file = display.clone();
            if verbose {
                eprintln!(
                    "keel analyze: {} — {} functions, {} classes, {} smells",
                    display,
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
            eprintln!("keel analyze: no data for file: {}", display);
            eprintln!("hint: Run `keel map` first to populate the graph.");
            2
        }
    }
}
