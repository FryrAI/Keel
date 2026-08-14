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
    semantic: bool,
) -> i32 {
    // Open graph store from the worktree-aware `.keel` location.
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let db_path = keel_core::paths::keel_dir(&cwd).join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel name: failed to open graph store: {}", e);
            eprintln!("  hint: run `keel init` first");
            return 2;
        }
    };

    let result = keel_enforce::naming::suggest_name_with_options(
        &store,
        &description,
        module.as_deref(),
        kind.as_deref(),
        keel_enforce::naming::NameOptions {
            semantic_candidates: semantic,
        },
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
