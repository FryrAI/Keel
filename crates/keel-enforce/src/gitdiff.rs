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
//!   staged index, so the very first commit's files are still reported. A
//!   `Range` base is always explicit, so it does NOT fall back: an unresolvable
//!   one is reported as an error rather than silently diffing something else;
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
/// (it is already the staged diff), and neither does `Range` — its base is
/// explicit, so an unresolvable one is an error. A total git failure (or an
/// unresolvable `Range` base) yields an empty list here; use
/// [`changed_files_checked`] to surface it.
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
                // An explicit base that git cannot resolve is a user error, not
                // an initial-commit repo: `--since typo` must NOT quietly become
                // the staged diff (in CI that compiles zero files and exits 0,
                // reading as "clean"). Only `Since` owns the index fallback.
                None => return Err(format!("cannot resolve --since base '{}'", base)),
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

/// How a single path changed between two revisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeStatus {
    /// The file did not exist on the base side.
    Added,
    /// The file exists on both sides with different content.
    Modified,
    /// The file existed on the base side only.
    Deleted,
    /// The file moved; `from` is its base-side path. Content may also differ.
    Renamed { from: String },
}

/// One entry of `git diff --name-status -M <base>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedPath {
    /// Head-side (or, for a deletion, base-side) repo-relative path.
    pub path: String,
    /// How the path changed.
    pub status: ChangeStatus,
}

impl ChangedPath {
    /// The base-side path to read this file's previous content from, or `None`
    /// for a file that did not exist on the base side.
    pub fn base_path(&self) -> Option<&str> {
        match &self.status {
            ChangeStatus::Added => None,
            ChangeStatus::Renamed { from } => Some(from.as_str()),
            _ => Some(self.path.as_str()),
        }
    }
}

/// Parse one `--name-status -M` record into a [`ChangedPath`].
///
/// Rename/copy records carry two tab-separated paths (`R096\told\tnew`);
/// every other status carries one. Unknown status letters are treated as
/// modifications, which is the safe direction — the file still gets diffed.
fn parse_name_status_line(line: &str) -> Option<ChangedPath> {
    let mut parts = line.split('\t');
    let code = parts.next()?;
    let first = parts.next()?;
    match code.chars().next()? {
        'A' => Some(ChangedPath {
            path: first.to_string(),
            status: ChangeStatus::Added,
        }),
        'D' => Some(ChangedPath {
            path: first.to_string(),
            status: ChangeStatus::Deleted,
        }),
        'R' | 'C' => {
            let new_path = parts.next()?;
            Some(ChangedPath {
                path: new_path.to_string(),
                status: ChangeStatus::Renamed {
                    from: first.to_string(),
                },
            })
        }
        _ => Some(ChangedPath {
            path: first.to_string(),
            status: ChangeStatus::Modified,
        }),
    }
}

/// List every path changed between `base` and the working tree, with rename
/// detection (`git diff --name-status -M`).
///
/// Unlike [`changed_files`] this keeps the *status*, which `keel review` needs
/// to tell a move apart from an add plus a remove, and to know which side of
/// the diff a path exists on. No language filter is applied: the caller decides
/// what to parse and what to list as unanalyzed.
pub fn changed_paths(dir: &Path, base: &str) -> Result<Vec<ChangedPath>, String> {
    let raw = run_git_checked(dir, &["diff", "--name-status", "-M", base])?
        .ok_or_else(|| format!("cannot resolve base ref '{}'", base))?;
    Ok(raw.lines().filter_map(parse_name_status_line).collect())
}

/// Read the contents of `path` as of revision `rev` (`git show <rev>:<path>`).
///
/// Returns `None` when the blob does not exist there or is not valid UTF-8 —
/// both mean "there is no base-side source to parse", which callers treat as
/// an addition rather than an error.
pub fn blob_at(dir: &Path, rev: &str, path: &str) -> Option<String> {
    let spec = format!("{}:{}", rev, path);
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["show", &spec])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

#[cfg(test)]
#[path = "gitdiff_tests.rs"]
mod tests;
