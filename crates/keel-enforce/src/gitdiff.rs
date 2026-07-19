//! One shared "which files changed?" implementation.
//!
//! `keel checkpoint`, `keel audit`, and `keel compile` each grew their own
//! `git diff --name-only` wrapper, and they disagreed: checkpoint lacked the
//! initial-commit fallback, audit lacked the supported-language filter and any
//! fallback. This module is the union of the correct behaviors so the three
//! interfaces detect the same set:
//!
//! - a name-only working-tree diff against a base commit (default `HEAD`), or
//!   the staged index;
//! - an initial-commit fallback — a `Since` diff whose base cannot be resolved
//!   (e.g. a repo with no commits, so `HEAD` does not exist) retries against the
//!   staged index, so the very first commit's files are still reported;
//! - an optional filter to only files keel can parse.

use std::path::Path;
use std::process::Command;

use keel_parsers::treesitter::detect_language;

/// Which git diff to compute.
#[derive(Debug, Clone)]
pub enum DiffMode {
    /// Working tree vs a base commit (default base: `HEAD`).
    Since(Option<String>),
    /// Committed range `<base>..HEAD` (what `keel compile --since` means).
    Range(String),
    /// Staged (index) changes.
    Staged,
}

/// Run `git -C dir <args>`, distinguishing "git could not run at all" (`Err`)
/// from "git ran and exited non-zero" (`Ok(None)`), so callers that must not
/// silently treat a missing git as "no changes" can surface it.
fn run_git_checked(dir: &Path, args: &[&str]) -> Result<Option<String>, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git: {}", e))?;
    if !out.status.success() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&out.stdout).to_string()))
}

/// Keep non-empty lines; when `only_supported`, drop paths keel cannot parse
/// (per the canonical `detect_language` extension table).
fn collect_lines(text: &str, only_supported: bool) -> Vec<String> {
    text.lines()
        .filter(|l| !l.is_empty())
        .filter(|l| !only_supported || detect_language(Path::new(l)).is_some())
        .map(|s| s.to_string())
        .collect()
}

/// List repo-relative paths of files changed for `mode`, evaluated in `dir`.
///
/// A `Since` diff whose base is unresolvable (git exits non-zero — e.g. a repo
/// with no `HEAD` yet) falls back to the staged diff. `Staged` never falls back
/// (it is already the staged diff). A total git failure yields an empty list.
pub fn changed_files(dir: &Path, mode: &DiffMode, only_supported: bool) -> Vec<String> {
    changed_files_checked(dir, mode, only_supported).unwrap_or_default()
}

/// Like [`changed_files`], but a git that cannot run at all is an `Err` instead
/// of an empty list — for callers (`keel compile --changed/--since`) where
/// "git is broken" must not silently read as "nothing changed, exit 0".
pub fn changed_files_checked(
    dir: &Path,
    mode: &DiffMode,
    only_supported: bool,
) -> Result<Vec<String>, String> {
    let raw = match mode {
        DiffMode::Staged => run_git_checked(dir, &["diff", "--name-only", "--cached"])?,
        DiffMode::Range(base) => {
            let range = format!("{}..HEAD", base);
            match run_git_checked(dir, &["diff", "--name-only", &range])? {
                Some(t) => Some(t),
                // Unresolvable base / no HEAD: fall back to the index.
                None => run_git_checked(dir, &["diff", "--name-only", "--cached"])?,
            }
        }
        DiffMode::Since(base) => {
            let arg = base.as_deref().unwrap_or("HEAD");
            match run_git_checked(dir, &["diff", "--name-only", arg])? {
                Some(t) => Some(t),
                // Initial commit / unresolvable base: fall back to the index.
                None => run_git_checked(dir, &["diff", "--name-only", "--cached"])?,
            }
        }
    };
    Ok(raw
        .map(|t| collect_lines(&t, only_supported))
        .unwrap_or_default())
}

#[cfg(test)]
#[path = "gitdiff_tests.rs"]
mod tests;
