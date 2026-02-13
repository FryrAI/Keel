use keel_output::OutputFormatter;

/// Run the `keel name` command.
///
/// Suggests names and file locations for new code based on graph analysis.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    description: String,
    module: Option<String>,
    kind: Option<String>,
) -> i32 {
    // Open graph store
    let db_path = ".keel/graph.db";
    let store = match keel_core::sqlite::SqliteGraphStore::open(db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel name: failed to open graph store: {}", e);
            eprintln!("  hint: run `keel init` first");
            return 2;
        }
    };

    let result = keel_enforce::naming::suggest_name(
        &store,
        &description,
        module.as_deref(),
        kind.as_deref(),
    );

    let output = formatter.format_name(&result);
    if !output.is_empty() {
        print!("{}", output);
    }

    if verbose {
        eprintln!(
            "keel name: {} suggestion(s) for \"{}\"",
            result.suggestions.len(),
            description,
        );
    }

    0
}
