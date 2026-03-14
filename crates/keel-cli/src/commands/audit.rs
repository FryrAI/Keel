use keel_output::OutputFormatter;

/// Run `keel audit` — AI-readiness scorecard for the codebase.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    changed: bool,
    strict: bool,
    min_score: Option<u32>,
    dimension: Option<String>,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel audit: failed to get current directory: {}", e);
            return 2;
        }
    };

    const VALID_DIMENSIONS: &[&str] = &[
        "structure",
        "discoverability",
        "navigation",
        "config",
        "verification",
    ];
    if let Some(ref dim) = dimension {
        if !VALID_DIMENSIONS.iter().any(|v| v.eq_ignore_ascii_case(dim)) {
            eprintln!(
                "keel audit: unknown dimension '{}'. Valid: {}",
                dim,
                VALID_DIMENSIONS.join(", ")
            );
            return 2;
        }
    }

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel audit: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel audit: failed to open graph database: {}", e);
            return 2;
        }
    };

    // Resolve changed files if --changed
    let changed_files = if changed {
        match resolve_changed_files(&cwd) {
            Some(files) => Some(files),
            None => {
                eprintln!("keel audit: failed to detect changed files via git");
                return 2;
            }
        }
    } else {
        None
    };

    let options = keel_enforce::types::AuditOptions {
        changed_only: changed,
        strict,
        min_score,
        dimension,
    };

    let result = keel_enforce::audit::audit_repo(&store, &cwd, &options, changed_files.as_deref());

    if verbose {
        eprintln!(
            "keel audit: score {}/{}, {} dimensions",
            result.total_score,
            result.max_score,
            result.dimensions.len(),
        );
    }

    let should_fail = keel_enforce::audit::should_fail(&result, &options);
    let output = formatter.format_audit(&result);
    if !output.is_empty() {
        println!("{}", output);
    }

    if should_fail {
        1
    } else {
        0
    }
}

fn resolve_changed_files(cwd: &std::path::Path) -> Option<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = text
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();
    Some(files)
}
