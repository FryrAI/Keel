//! Screening of `keel compile`'s target list before anything is parsed.

use std::path::{Path, PathBuf};

/// Validate the resolved target list, returning the files worth compiling.
///
/// Two rules, both about targets keel cannot honestly check:
///
/// * An explicitly-named target that does not exist is a hard error (`Err(2)`),
///   not a silent exit 0: the agent asked us to validate a specific file and we
///   could not. Git-deleted paths under `--changed`/`--since` are not explicit,
///   so they still skip silently.
/// * A target outside the repository is dropped with one stderr line. It cannot
///   appear in the graph, so enforcing against it reports phantom findings; the
///   editor hooks fire on every write an agent makes, including ones far from
///   the project. `Err(0)` when that leaves nothing to do, so a run that was
///   entirely out-of-tree exits clean instead of compiling the empty set (which
///   would overwrite the `--delta` snapshot with an empty one).
pub fn screen_targets(
    cwd: &Path,
    targets: Vec<String>,
    explicit: bool,
) -> Result<Vec<String>, i32> {
    if explicit {
        for path in &targets {
            if !Path::new(path).exists() {
                eprintln!("keel compile: file not found: {}", path);
                return Err(2);
            }
        }
    }
    // The working tree, not `cwd`: an agent started in a subdirectory still
    // edits files across the whole repository, and every one of them is in-tree.
    let root = keel_core::paths::worktree_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let root = root.canonicalize().unwrap_or(root);
    let total = targets.len();
    let kept: Vec<String> = targets
        .into_iter()
        .filter(|path| {
            if in_repo(&root, Path::new(path)) {
                return true;
            }
            eprintln!("keel compile: skipped (outside repository): {}", path);
            false
        })
        .collect();
    if kept.is_empty() && total > 0 {
        return Err(0);
    }
    Ok(kept)
}

/// Whether `path` resolves inside `root` (already canonicalized).
///
/// A target that cannot be canonicalized — it does not exist, and the explicit
/// check above let it through — is treated as in-repo: the later stages report
/// it honestly, and guessing "outside" would swallow it.
fn in_repo(root: &Path, path: &Path) -> bool {
    match canonical_parent(path) {
        Some(dir) => dir.starts_with(root),
        None => true,
    }
}

/// Canonicalize `path`'s parent directory — `path` itself may be a file that
/// was just deleted, and the directory is what decides in-tree membership.
fn canonical_parent(path: &Path) -> Option<PathBuf> {
    path.parent()?.canonicalize().ok()
}
