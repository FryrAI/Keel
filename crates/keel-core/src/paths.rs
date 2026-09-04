//! Resolution of the `.keel` directory, shared across git worktrees.
//!
//! `.keel/graph.db` is gitignored, so keying it to `cwd.join(".keel")` makes
//! every git worktree of a repo build and maintain its own full graph from
//! scratch. [`keel_dir`] instead resolves `.keel` to the **main checkout root**,
//! so all worktrees of a repo share one `.keel/graph.db`. Concurrent access is
//! made safe by the SQLite `busy_timeout` configured on every connection.

use std::fs;
use std::path::{Path, PathBuf};

/// Resolve the `.keel` directory for the repository containing `start`.
///
/// Walks up from `start` to the first ancestor holding a `.git` entry. For a
/// normal checkout `.git` is a directory and the keel dir is `<root>/.keel`. For
/// a linked worktree `.git` is a file whose `gitdir:` line points into the main
/// repo's `.git/worktrees/<name>`; reading that dir's `commondir` file locates
/// the common `.git`, whose parent is the main checkout — so every worktree
/// shares `<main_root>/.keel`.
///
/// If no `.git` is found, or any part of the worktree chain is missing or
/// unparseable, falls back to `<start>/.keel` (the pre-worktree behavior).
pub fn keel_dir(start: &Path) -> PathBuf {
    resolve_repo_root(start)
        .map(|root| root.join(".keel"))
        .unwrap_or_else(|| start.join(".keel"))
}

/// Render `path` as a project-root-relative string the way the graph stores it.
///
/// Strips `root` when `path` sits under it; otherwise (a path outside `root`, or
/// an already-relative path — `strip_prefix` fails on both) the input is
/// returned unchanged via `to_string_lossy`. This is the one shared spelling of
/// the "make it relative to the repo root" idiom that the CLI commands, the map
/// builder, and the server watcher all need.
pub fn make_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

/// The working-tree root containing `start`: the nearest ancestor holding a
/// `.git` entry (a directory for a normal checkout, a file for a linked
/// worktree), or `None` outside any repository.
///
/// This is `git rev-parse --show-toplevel`, not [`keel_dir`]'s main-checkout
/// root: a linked worktree's files live under the worktree, so "is this path
/// inside the repository" must be answered against the worktree itself.
pub fn worktree_root(start: &Path) -> Option<PathBuf> {
    find_dot_git(start).map(|(root, _)| root)
}

/// Find the main checkout root of the repo containing `start`, or `None` if
/// `start` is not inside a git repo or a worktree link cannot be resolved.
fn resolve_repo_root(start: &Path) -> Option<PathBuf> {
    let (root, git_path) = find_dot_git(start)?;
    if git_path.is_dir() {
        // Normal checkout: the repo root holds `.git/`.
        Some(root)
    } else {
        // Linked worktree: `.git` is a file pointing at the shared git dir.
        main_root_from_worktree(&git_path)
    }
}

/// Walk up from `start`, returning the first `(dir, dir/.git)` where `.git`
/// is an actual repository entry (see [`is_git_entry`]), not merely a path
/// that happens to exist under that name.
fn find_dot_git(start: &Path) -> Option<(PathBuf, PathBuf)> {
    for dir in start.ancestors() {
        let git = dir.join(".git");
        if is_git_entry(&git) {
            return Some((dir.to_path_buf(), git));
        }
    }
    None
}

/// Whether `path` (a candidate `.git` entry) is a real repository marker.
///
/// A directory named `.git` only counts if it holds a `HEAD` file, which
/// every real git directory (normal checkout or a linked worktree's own
/// `.git/worktrees/<name>` target) has — an *empty* directory named `.git`
/// does not. This matters because such a directory can appear by accident: a
/// sandbox marker under `/tmp`, or a stray `mkdir .git`. Without this check
/// it would be indistinguishable from a real repo root and would hijack
/// resolution for every path beneath it (see issue: empty `/tmp/.git`
/// breaking `keel_dir`/`worktree_root` for every tempdir-based test on the
/// box). A file named `.git` only counts if it is a linked worktree pointer —
/// its content has a line starting with `gitdir:` (mirrors the parsing in
/// [`main_root_from_worktree`]). Anything else (missing path, empty dir,
/// unrelated file) is not a repository entry, and the caller keeps walking up
/// to the next ancestor.
fn is_git_entry(path: &Path) -> bool {
    if path.is_dir() {
        // `symlink_metadata`, not `is_file`: a legacy `HEAD -> refs/heads/x`
        // symlink to an unborn branch is a valid git dir, and following it
        // would reject the inner repository in favour of an outer one.
        path.join("HEAD").symlink_metadata().is_ok()
    } else if path.is_file() {
        fs::read_to_string(path)
            .map(|content| content.lines().any(|l| l.starts_with("gitdir:")))
            .unwrap_or(false)
    } else {
        false
    }
}

/// Given a worktree's `.git` file, resolve the main checkout root via its
/// `gitdir:` pointer and the shared git dir's `commondir` file.
fn main_root_from_worktree(git_file: &Path) -> Option<PathBuf> {
    let content = fs::read_to_string(git_file).ok()?;
    let gitdir_value = content.lines().find_map(|l| l.strip_prefix("gitdir:"))?;

    // `.git/worktrees/<name>` for this worktree (absolute by default; if
    // relative it is relative to the dir holding the `.git` file).
    let raw = PathBuf::from(gitdir_value.trim());
    let gitdir = if raw.is_absolute() {
        raw
    } else {
        git_file.parent()?.join(raw)
    };

    // `commondir` points at the shared `.git` dir (usually `../..`).
    let common_rel = fs::read_to_string(gitdir.join("commondir")).ok()?;
    let common_rel = common_rel.trim();
    let common_git = if Path::new(common_rel).is_absolute() {
        PathBuf::from(common_rel)
    } else {
        gitdir.join(common_rel)
    };

    // Collapse `..` segments; the shared `.git` dir's parent is the main root.
    let common_git = common_git.canonicalize().unwrap_or(common_git);
    common_git.parent().map(Path::to_path_buf)
}

/// Resolve `candidate` against `root` and confine it to the project tree.
///
/// Returns the normalized absolute path when it stays under `root`, or `None`
/// when `candidate` is absolute-outside-root, escapes via `..`, or is a
/// symlink whose real target lies outside `root`. `root` is assumed already
/// canonicalized (an existing directory). Non-existent targets pass on the
/// lexical check alone (they cannot be read anyway).
///
/// This is the confinement primitive for every server-side surface that
/// accepts a path from a client (HTTP compile, MCP skeleton); local CLI
/// commands deliberately do not confine — the user's own shell is not a
/// privilege boundary.
pub fn confine(root: &Path, candidate: &str) -> Option<PathBuf> {
    let raw = Path::new(candidate);
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        root.join(raw)
    };
    let normalized = normalize_lexically(&joined);
    if !normalized.starts_with(root) {
        return None;
    }
    if normalized.exists() {
        let real = fs::canonicalize(&normalized).ok()?;
        if !real.starts_with(root) {
            return None;
        }
    }
    Some(normalized)
}

/// Resolve `.` and `..` components without consulting the filesystem, so
/// non-existent targets still validate deterministically.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
