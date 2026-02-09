use keel_output::OutputFormatter;

/// Run `keel compile` — incremental validation of changed files.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    files: Vec<String>,
    batch_start: bool,
    batch_end: bool,
    strict: bool,
    suppress: Option<String>,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel compile: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel compile: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(
        db_path.to_str().unwrap_or(""),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel compile: failed to open graph database: {}", e);
            return 2;
        }
    };

    let mut engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));

    // Apply suppressions
    if let Some(code) = &suppress {
        engine.suppress(code);
    }

    // Handle batch mode
    if batch_start {
        engine.batch_start();
        if verbose {
            eprintln!("keel compile: batch mode started");
        }
        return 0;
    }

    if batch_end {
        let result = engine.batch_end();
        return output_result(formatter, &result, strict, verbose);
    }

    // Parse target files into FileIndex entries
    // TODO: Use language resolvers to parse files into FileIndex.
    // For now, compile against graph store with empty file indices.
    let file_indices: Vec<keel_parsers::resolver::FileIndex> = Vec::new();

    if verbose && !files.is_empty() {
        eprintln!("keel compile: checking {} file(s)", files.len());
    }

    let result = engine.compile(&file_indices);
    output_result(formatter, &result, strict, verbose)
}

fn output_result(
    formatter: &dyn OutputFormatter,
    result: &keel_enforce::types::CompileResult,
    strict: bool,
    verbose: bool,
) -> i32 {
    // Clean compile = empty stdout, exit 0
    let has_errors = !result.errors.is_empty();
    let has_warnings = !result.warnings.is_empty();

    if !has_errors && !has_warnings {
        if verbose {
            eprintln!("keel compile: clean — no violations");
        }
        return 0;
    }

    // Output violations
    let output = formatter.format_compile(result);
    if !output.is_empty() {
        println!("{}", output);
    }

    if has_errors || (strict && has_warnings) {
        1
    } else {
        0
    }
}
