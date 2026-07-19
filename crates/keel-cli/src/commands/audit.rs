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

    let (cwd, store) = match super::open_store("audit") {
        Ok(x) => x,
        Err(code) => return code,
    };

    // Resolve changed files if --changed, via the shared git-diff helper: a
    // name-only working-tree diff against HEAD, restricted to parseable source
    // files, with the initial-commit fallback built in.
    let changed_files = if changed {
        Some(keel_enforce::gitdiff::changed_files(
            &cwd,
            &keel_enforce::gitdiff::DiffMode::Since(None),
            true,
        ))
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
