use std::path::Path;
use std::process::Command;

use super::*;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "user.email=test@keel.dev",
            "-c",
            "user.name=keel test",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A git repo with one commit, plus an open graph store beside it.
fn repo() -> (tempfile::TempDir, SqliteGraphStore) {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    std::fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    git(dir.path(), &["add", "a.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "first"]);
    let db = dir.path().join("graph.db");
    let store = SqliteGraphStore::open(db.to_str().unwrap()).unwrap();
    (dir, store)
}

fn head(dir: &Path) -> String {
    keel_enforce::gitdiff::head_commit(dir).expect("HEAD")
}

/// A graph mapped by a keel that never wrote the marker must keep working:
/// silence, not a hard failure, is the only safe answer for the graphs that
/// already exist on every machine running keel today.
#[test]
fn a_graph_without_the_marker_is_never_stale() {
    let (dir, store) = repo();
    assert!(stale_graph_message(dir.path(), &store).is_none());
}

/// The ordinary case: map, then commit on top. HEAD moved forward, the graph
/// still describes code that is in this history.
#[test]
fn a_descendant_head_is_not_stale() {
    let (dir, store) = repo();
    let mapped_at = head(dir.path());
    store.set_meta_value(LAST_MAP_COMMIT, &mapped_at).unwrap();
    assert!(stale_graph_message(dir.path(), &store).is_none());

    std::fs::write(dir.path().join("b.rs"), "fn b() {}\n").unwrap();
    git(dir.path(), &["add", "b.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "second"]);
    assert!(
        stale_graph_message(dir.path(), &store).is_none(),
        "committing on top of the mapped commit must not invalidate the graph"
    );
}

/// History rewritten out from under the graph — the rebase/amend/branch-switch
/// case, and the shape a poisoned CI cache takes.
#[test]
fn a_rewritten_history_is_stale_and_the_message_names_the_fix() {
    let (dir, store) = repo();
    let first = head(dir.path());
    std::fs::write(dir.path().join("b.rs"), "fn b() {}\n").unwrap();
    git(dir.path(), &["add", "b.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "second"]);
    let second = head(dir.path());
    store.set_meta_value(LAST_MAP_COMMIT, &second).unwrap();

    git(dir.path(), &["reset", "-q", "--hard", &first]);
    std::fs::write(dir.path().join("c.rs"), "fn c() {}\n").unwrap();
    git(dir.path(), &["add", "c.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "divergent"]);

    let msg = stale_graph_message(dir.path(), &store).expect("a diverged graph must be reported");
    assert!(msg.contains(&second[..12]), "must name the commit: {msg}");
    assert!(msg.contains("keel map"), "must name the fix: {msg}");
}

/// A marker naming an object this clone does not have (shallow fetch, dropped
/// branch) is unknowable. Unknowable is not stale.
#[test]
fn an_unknown_commit_is_not_reported_as_stale() {
    let (dir, store) = repo();
    store
        .set_meta_value(LAST_MAP_COMMIT, "0000000000000000000000000000000000000000")
        .unwrap();
    assert!(stale_graph_message(dir.path(), &store).is_none());
}

/// No git repository at all: the guard has no opinion.
#[test]
fn a_checkout_without_git_is_not_reported_as_stale() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graph.db");
    let store = SqliteGraphStore::open(db.to_str().unwrap()).unwrap();
    store
        .set_meta_value(LAST_MAP_COMMIT, "1234567890abcdef1234567890abcdef12345678")
        .unwrap();
    assert!(stale_graph_message(dir.path(), &store).is_none());
}
