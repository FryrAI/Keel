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
//! - an optional filter to only files keel can parse;
//! - the repository's `.keelignore`/`.gitignore` scope, applied here so every
//!   git-diff-driven command sees exactly the files `keel map` put in the graph
//!   (issue #70). Git lists a tracked-but-ignored file in a diff; the walker
//!   never does.

use std::path::{Path, PathBuf};
use std::process::Command;

use keel_parsers::treesitter::detect_language;
use keel_parsers::walker::KeelIgnore;

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

/// The repository root, which is what git prints its diff paths relative to —
/// so it, not the caller's `dir`, is where the ignore rules live. Falls back to
/// `dir` when git cannot say (no repository, no git).
fn repo_root(dir: &Path) -> PathBuf {
    run_git_checked(dir, &["rev-parse", "--show-toplevel"])
        .ok()
        .flatten()
        .map(|out| PathBuf::from(out.trim()))
        .filter(|root| !root.as_os_str().is_empty())
        .unwrap_or_else(|| dir.to_path_buf())
}

/// Keep non-empty NUL-delimited paths; drop paths the ignore rules exclude, and
/// when `only_supported`, paths keel cannot parse (per the canonical
/// `detect_language` extension table).
fn collect_paths(text: &str, only_supported: bool, ignore: &KeelIgnore) -> Vec<String> {
    text.split('\0')
        .filter(|l| !l.is_empty())
        .filter(|l| !only_supported || detect_language(Path::new(l)).is_some())
        .filter(|l| !ignore.is_ignored(Path::new(l)))
        .map(|s| s.to_string())
        .collect()
}

/// List repo-relative paths of files changed for `mode`, evaluated in `dir`.
///
/// Paths excluded by the repository root's `.keelignore`/`.gitignore` are
/// dropped, so a git-diff-driven command never checks a file `keel map` refused
/// to graph — `dir` may be any directory inside the repo.
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
        DiffMode::Staged => run_git_checked(dir, &["diff", "--name-only", "-z", "--cached"])?,
        DiffMode::Range(base) => {
            let range = format!("{}..HEAD", base);
            match run_git_checked(dir, &["diff", "--name-only", "-z", &range])? {
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
            match run_git_checked(dir, &["diff", "--name-only", "-z", arg])? {
                Some(t) => Some(t),
                // Initial commit / unresolvable base: fall back to the index.
                None => run_git_checked(dir, &["diff", "--name-only", "-z", "--cached"])?,
            }
        }
    };
    Ok(raw
        .map(|t| collect_paths(&t, only_supported, &KeelIgnore::new(&repo_root(dir))))
        .unwrap_or_default())
}

/// Whether one commit is contained in another revision's history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ancestry {
    /// git answered yes: the commit is an ancestor of the revision, or is it.
    Ancestor,
    /// git answered no: the revision's history does not contain the commit.
    NotAncestor,
    /// git could not answer — not installed, not a repository, or the commit
    /// is not an object this checkout knows (a shallow clone, a dropped
    /// branch). Callers must treat this as "no opinion", never as "not an
    /// ancestor": guessing turns a missing tool into a hard failure.
    Unknown,
}

/// Map `git merge-base --is-ancestor`'s exit status onto an [`Ancestry`].
///
/// git documents exactly two answers — 0 for yes and 1 for no — and uses any
/// other status (128 for an unknown object, 129 for a usage error) to say it
/// could not answer at all. A process killed by a signal has no code and lands
/// in the same bucket.
pub fn classify_ancestry(code: Option<i32>) -> Ancestry {
    match code {
        Some(0) => Ancestry::Ancestor,
        Some(1) => Ancestry::NotAncestor,
        _ => Ancestry::Unknown,
    }
}

/// Ask git whether `commit` is an ancestor of `rev` (usually `"HEAD"`).
pub fn is_ancestor(dir: &Path, commit: &str, rev: &str) -> Ancestry {
    match Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["merge-base", "--is-ancestor", commit, rev])
        .output()
    {
        Ok(out) => classify_ancestry(out.status.code()),
        Err(_) => Ancestry::Unknown,
    }
}

/// The commit `HEAD` currently points at, or `None` when there is none —
/// no git, no repository, or a repository whose first commit does not exist
/// yet (`git init` with nothing committed).
pub fn head_commit(dir: &Path) -> Option<String> {
    let sha = run_git_checked(dir, &["rev-parse", "HEAD"])
        .ok()??
        .trim()
        .to_string();
    (!sha.is_empty()).then_some(sha)
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

/// Parse `--name-status -z -M` records into changed paths.
///
/// Status and paths are NUL-separated; rename/copy records carry two paths,
/// every other status carries one. Unknown status letters are treated as
/// modifications, which is the safe direction — the file still gets diffed.
fn parse_name_status(text: &str) -> Vec<ChangedPath> {
    let mut parts = text.split_terminator('\0');
    let mut paths = Vec::new();
    while let Some(code) = parts.next() {
        let Some(first) = parts.next() else { break };
        let (path, status) = match code.chars().next() {
            Some('A') => (first, ChangeStatus::Added),
            Some('D') => (first, ChangeStatus::Deleted),
            Some('R' | 'C') => {
                let Some(new_path) = parts.next() else { break };
                (
                    new_path,
                    ChangeStatus::Renamed {
                        from: first.to_string(),
                    },
                )
            }
            _ => (first, ChangeStatus::Modified),
        };
        paths.push(ChangedPath {
            path: path.to_string(),
            status,
        });
    }
    paths
}

/// List every path changed between `base` and the working tree, with rename
/// detection (`git diff --name-status -M`).
///
/// Unlike [`changed_files`] this keeps the *status*, which `keel review` needs
/// to tell a move apart from an add plus a remove, and to know which side of
/// the diff a path exists on. No language filter is applied: the caller decides
/// what to parse and what to list as unanalyzed. Ignored paths are dropped as
/// they are everywhere else — a review is measured against the graph, and the
/// graph has no ignored files to compare against.
pub fn changed_paths(dir: &Path, base: &str) -> Result<Vec<ChangedPath>, String> {
    let raw = run_git_checked(dir, &["diff", "--name-status", "-z", "-M", base])?
        .ok_or_else(|| format!("cannot resolve base ref '{}'", base))?;
    let ignore = KeelIgnore::new(&repo_root(dir));
    Ok(parse_name_status(&raw)
        .into_iter()
        .filter_map(|entry| apply_ignore(entry, &ignore))
        .collect())
}

/// Re-frame one changed path against the ignore rules, or drop it.
///
/// A rename that crosses the ignore boundary is not a move to keel: the ignored
/// side is not in the graph, so only one endpoint is visible. Left as a rename,
/// a file moved *out of* an ignored tree would have its base blob parsed and its
/// symbols scored as merely relocated (cancelling their violations as
/// pre-existing), and a file moved *into* one would be dropped whole, hiding the
/// contracts it removed.
fn apply_ignore(entry: ChangedPath, ignore: &KeelIgnore) -> Option<ChangedPath> {
    let head_ignored = ignore.is_ignored(Path::new(&entry.path));
    match entry.status {
        ChangeStatus::Renamed { from } => match (ignore.is_ignored(Path::new(&from)), head_ignored)
        {
            (true, true) => None,
            // Arrived from outside the graph: new code at the new path.
            (true, false) => Some(ChangedPath {
                path: entry.path,
                status: ChangeStatus::Added,
            }),
            // Left the graph: its contracts are gone from the old path.
            (false, true) => Some(ChangedPath {
                path: from,
                status: ChangeStatus::Deleted,
            }),
            (false, false) => Some(ChangedPath {
                path: entry.path,
                status: ChangeStatus::Renamed { from },
            }),
        },
        status => (!head_ignored).then_some(ChangedPath {
            path: entry.path,
            status,
        }),
    }
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

/// Resolve a revision to one immutable commit, rejecting unborn or unknown refs.
pub fn resolve_commit(dir: &Path, rev: &str) -> Result<String, String> {
    let spec = format!("{}^{{commit}}", rev);
    let out = run_git_checked(dir, &["rev-parse", "--verify", "--end-of-options", &spec])?
        .ok_or_else(|| format!("cannot resolve base ref {rev:?}"))?;
    Ok(out.trim().to_string())
}

/// Read a UTF-8 blob, distinguishing a missing path from Git or decoding failures.
/// Symlink entries return `None`: their link text is not base-side source.
/// `rev` should be a resolved commit id, so all reads use the same snapshot.
pub fn blob_at_checked(dir: &Path, rev: &str, path: &str) -> Result<Option<String>, String> {
    let listing = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["ls-tree", "-z", rev, "--", path])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !listing.status.success() {
        return Err("cannot read base tree".into());
    }
    if listing.stdout.is_empty() {
        return Ok(None);
    }
    let header = listing
        .stdout
        .split(|b| *b == b'\t')
        .next()
        .unwrap_or_default();
    let header = std::str::from_utf8(header).map_err(|e| e.to_string())?;
    let mut fields = header.split_whitespace();
    if fields.next() == Some("120000") {
        return Ok(None);
    }
    if fields.next() != Some("blob") {
        return Err("base path is not a blob".into());
    }
    let oid = fields.next().ok_or("missing base blob id")?;
    let blob = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["cat-file", "blob", oid])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !blob.status.success() {
        return Err("cannot read base blob".into());
    }
    String::from_utf8(blob.stdout)
        .map(Some)
        .map_err(|e| format!("base blob is not UTF-8: {e}"))
}

#[cfg(test)]
#[path = "gitdiff_tests.rs"]
mod tests;
