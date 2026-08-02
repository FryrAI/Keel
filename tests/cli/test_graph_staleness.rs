// `keel compile` must refuse to enforce against a graph built on a commit this
// checkout does not contain (T2.3): a poisoned CI cache, a rebase, a branch
// switch. Annotating phantom callers is strictly worse than failing.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use crate::common::{git, keel_bin};

fn keel(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run keel")
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Read one `keel_meta` value straight out of the repo's graph database.
fn meta(dir: &Path, key: &str) -> Option<String> {
    let conn = rusqlite::Connection::open_with_flags(
        dir.join(".keel").join("graph.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("failed to open graph.db");
    conn.query_row(
        "SELECT value FROM keel_meta WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

fn head(dir: &Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// An initialized, committed, mapped repo. `keel map` runs AFTER the first
/// commit so the graph carries a real `last_map_commit` marker.
fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "src/lib.ts",
        "export function execute(cmd: number): number {\n  return cmd;\n}\n",
    );
    git(root, &["init", "-q"]);
    assert!(keel(root, &["init"]).status.success(), "keel init failed");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "first"]);
    assert!(keel(root, &["map"]).status.success(), "keel map failed");
    dir
}

/// `keel map` drops both markers before it rebuilds, so a run that dies partway
/// reads as never-mapped instead of "mapped at HEAD over an empty graph" — a
/// state this guard would happily wave through. The other half of that
/// contract is pinned here: a map that *completes* leaves both stamped.
#[test]
fn a_completed_map_stamps_both_markers() {
    let dir = fixture();
    let root = dir.path();

    assert_eq!(
        meta(root, "last_map_commit").as_deref(),
        Some(head(root).as_str()),
        "a finished map must record the commit it described"
    );
    assert!(
        meta(root, "last_map_at").is_some(),
        "a finished map must record that it happened"
    );
}

/// The ordinary case: the graph's commit is behind HEAD, which is exactly what
/// every incremental workflow looks like. It must not trip the guard.
#[test]
fn a_graph_behind_head_still_compiles() {
    let dir = fixture();
    let root = dir.path();

    write(
        root,
        "src/other.ts",
        "export function other(): number {\n  return 1;\n}\n",
    );
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "second"]);

    let out = keel(root, &["compile", "--changed"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("not an ancestor"),
        "a descendant HEAD must not read as a stale graph: {stderr}"
    );
    assert_ne!(
        out.status.code(),
        Some(2),
        "compile must not error on a graph mapped at an ancestor: {stderr}"
    );
}

/// The poisoned-cache shape: the graph describes a commit that is no longer in
/// HEAD's history. Exit 2, not annotations.
#[test]
fn a_graph_from_a_rewritten_history_exits_two() {
    let dir = fixture();
    let root = dir.path();
    let first = head(root);

    // Map at a commit that is then rewritten away.
    write(root, "src/lib.ts", "export function execute(cmd: number, dry: boolean): number {\n  return dry ? 0 : cmd;\n}\n");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "second"]);
    let second = head(root);
    assert!(keel(root, &["map"]).status.success(), "keel map failed");

    git(root, &["reset", "-q", "--hard", &first]);
    write(
        root,
        "src/divergent.ts",
        "export function divergent(): number {\n  return 2;\n}\n",
    );
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "divergent"]);

    let out = keel(root, &["compile", "--changed"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a graph from an unreachable commit must fail loudly, got stderr: {stderr}"
    );
    assert!(
        stderr.contains(&second[..12]) && stderr.contains("keel map"),
        "the message must name the stale commit and the fix: {stderr}"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).is_empty(),
        "a stale graph must annotate nothing at all"
    );
}
