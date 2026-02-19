use keel_enforce::types::{FixApplyDetail, FixApplyResult, FixResult};
use keel_output::OutputFormatter;
use std::path::PathBuf;

/// Run the `keel fix` command.
///
/// Generates fix plans for violations. Without --apply, outputs plan only.
/// With --apply, writes fixes to disk and re-compiles to verify.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    hashes: Vec<String>,
    file: Option<String>,
    apply: bool,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel fix: failed to get current directory: {}", e);
            return 2;
        }
    };

    let db_path = cwd.join(".keel").join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(&db_path.to_string_lossy()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel fix: failed to open graph store: {}", e);
            eprintln!("  hint: run `keel init` first");
            return 2;
        }
    };

    // Gather files to check
    let file_paths: Vec<PathBuf> = if let Some(ref f) = file {
        vec![cwd.join(f)]
    } else {
        // Use all files known to the store via compile
        vec![]
    };

    let mut engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));

    // Parse files and compile to get violations
    let file_indices = super::parse_util::parse_files_to_indices(&file_paths, &cwd);
    let compile_result = engine.compile(&file_indices);

    // Filter violations by hash if specified
    let all_violations: Vec<_> = compile_result
        .errors
        .iter()
        .chain(compile_result.warnings.iter())
        .filter(|v| {
            if hashes.is_empty() {
                true
            } else {
                hashes.contains(&v.hash)
            }
        })
        .collect();

    // Generate fix plans using a fresh store (engine consumed the previous one)
    let fix_store = match keel_core::sqlite::SqliteGraphStore::open(&db_path.to_string_lossy()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel fix: failed to reopen graph store: {}", e);
            return 2;
        }
    };
    let plans = keel_enforce::fix_generator::generate_fix_plans(&all_violations, &fix_store);

    let files_affected: std::collections::HashSet<&str> = plans
        .iter()
        .flat_map(|p| p.actions.iter().map(|a| a.file.as_str()))
        .collect();

    let result = FixResult {
        version: "0.1.0".to_string(),
        command: "fix".to_string(),
        violations_addressed: plans.len() as u32,
        files_affected: files_affected.len() as u32,
        plans,
    };

    if !apply {
        let output = formatter.format_fix(&result);
        if !output.is_empty() {
            print!("{}", output);
        }
        if verbose {
            eprintln!(
                "keel fix: {} plans generated for {} files",
                result.violations_addressed, result.files_affected,
            );
        }
        return 0;
    }

    // --apply mode: write fixes to disk, then re-compile
    let apply_result = apply_fix_plans(&result, &cwd, verbose);
    let exit_code = if apply_result.actions_failed > 0 || !apply_result.recompile_clean {
        1
    } else {
        0
    };

    let output = keel_output::llm::fix::format_fix_apply(&apply_result);
    if !output.is_empty() {
        print!("{}", output);
    }
    exit_code
}

/// Apply fix plans by writing changes to files, then re-compile to verify.
fn apply_fix_plans(result: &FixResult, cwd: &std::path::Path, verbose: bool) -> FixApplyResult {
    let mut details = Vec::new();
    let mut files_modified = std::collections::HashSet::new();
    let mut applied = 0u32;
    let mut failed = 0u32;

    for plan in &result.plans {
        // Validate before applying
        let validation_errors = keel_enforce::fix_generator::validate_fix_plan(plan, cwd);

        for (i, action) in plan.actions.iter().enumerate() {
            if let Some((_, err)) = validation_errors.iter().find(|(idx, _)| *idx == i) {
                details.push(FixApplyDetail {
                    file: action.file.clone(),
                    line: action.line,
                    status: "failed".into(),
                    error: Some(err.clone()),
                });
                failed += 1;
                continue;
            }

            match apply_single_action(action, cwd) {
                Ok(()) => {
                    files_modified.insert(action.file.clone());
                    details.push(FixApplyDetail {
                        file: action.file.clone(),
                        line: action.line,
                        status: "applied".into(),
                        error: None,
                    });
                    applied += 1;
                }
                Err(e) => {
                    details.push(FixApplyDetail {
                        file: action.file.clone(),
                        line: action.line,
                        status: "failed".into(),
                        error: Some(e),
                    });
                    failed += 1;
                }
            }
        }
    }

    // Re-compile to verify fixes
    let (recompile_clean, recompile_errors) = recompile_verify(cwd, verbose);

    let files_vec: Vec<String> = files_modified.into_iter().collect();
    FixApplyResult {
        version: "0.1.0".into(),
        command: "fix --apply".into(),
        actions_applied: applied,
        actions_failed: failed,
        files_modified: files_vec,
        recompile_clean,
        recompile_errors,
        details,
    }
}

/// Apply a single fix action to a file.
fn apply_single_action(
    action: &keel_enforce::types::FixAction,
    cwd: &std::path::Path,
) -> Result<(), String> {
    let path = cwd.join(&action.file);
    let content = std::fs::read_to_string(&path).map_err(|e| format!("read error: {}", e))?;

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let idx = (action.line as usize).saturating_sub(1);

    if action.old_text.is_empty() {
        // Insert new_text before the target line
        if idx <= lines.len() {
            lines.insert(idx, action.new_text.clone());
        } else {
            lines.push(action.new_text.clone());
        }
    } else if idx < lines.len() && lines[idx].contains(&action.old_text) {
        // Exact replacement on the target line
        lines[idx] = lines[idx].replace(&action.old_text, &action.new_text);
    } else {
        // Search nearby lines (±2) for old_text
        let start = idx.saturating_sub(2);
        let end = (idx + 3).min(lines.len());
        let mut found = false;
        for line in &mut lines[start..end] {
            if line.contains(&action.old_text) {
                *line = line.replace(&action.old_text, &action.new_text);
                found = true;
                break;
            }
        }
        if !found {
            // Fallback: insert as guidance comment
            let comment = format!("// FIX: {}", action.new_text);
            if idx <= lines.len() {
                lines.insert(idx, comment);
            } else {
                lines.push(comment);
            }
        }
    }

    let new_content = lines.join("\n");
    // Preserve trailing newline if original had one
    let final_content = if content.ends_with('\n') && !new_content.ends_with('\n') {
        format!("{}\n", new_content)
    } else {
        new_content
    };

    std::fs::write(&path, final_content).map_err(|e| format!("write error: {}", e))
}

/// Re-compile after applying fixes and return (is_clean, error_count).
fn recompile_verify(cwd: &std::path::Path, verbose: bool) -> (bool, u32) {
    let db_path = cwd.join(".keel").join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(&db_path.to_string_lossy()) {
        Ok(s) => s,
        Err(_) => return (false, 0),
    };
    let mut engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[]);

    let error_count = result.errors.len() as u32;
    let is_clean = result.errors.is_empty() && result.warnings.is_empty();

    if verbose {
        eprintln!(
            "keel fix --apply: recompile {} (errors={}, warnings={})",
            if is_clean { "CLEAN" } else { "DIRTY" },
            result.errors.len(),
            result.warnings.len(),
        );
    }

    (is_clean, error_count)
}
