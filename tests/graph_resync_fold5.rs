//! A dependent file is resolved from two texts (HEAD's blob and the working
//! tree); the working tree wins per (source, target, kind), and call sites
//! within one text are never merged.

#[path = "common/mod.rs"]
mod common;

use common::{git, keel_bin};
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

const IDLE: &str = "/// Widget.\npub fn widget() -> u32 { 29 }\n";
const GO_MOD: &str = "module pkg\n\ngo 1.21\n";
const GO_ALPHA: &str =
    "package pkg\n\n// Alpha.\nfunc Alpha(value int) int {\n    return value + 1\n}\n";
const GO_BETA: &str =
    "package pkg\n\n// Beta.\nfunc Beta(value int) int {\n    return value * 3\n}\n";
const ONE_CALL: &str = "package pkg\n\n// Widget.\nfunc Widget() int {\n    return Alpha(1)\n}\n";
const TWO_CALLS: &str = "package pkg\n\n// Widget.\nfunc Widget() int {\n    first := Alpha(1)\n    return first + Alpha(2)\n}\n";
const SHIFTED: &str = "package pkg\n\n// A working-tree comment.\n// Another line.\n\n// Widget.\nfunc Widget() int {\n    return Alpha(1)\n}\n";

// These helpers mirror tests/graph_resync_fold4.rs (each target carries its own copy).
fn keel(root: &Path, args: &[&str]) -> Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

fn write(root: &Path, rel: &str, content: &str) {
    let file = root.join(rel);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, content).unwrap();
}

fn head(root: &Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn count(root: &Path, sql: &str) -> i64 {
    Connection::open(root.join(".keel/graph.db"))
        .unwrap()
        .query_row(sql, [], |r| r.get(0))
        .unwrap()
}

fn map(root: &Path) {
    let out = keel(root, &["map"]);
    assert!(out.status.success(), "fixture map: {out:?}");
}

fn fixture(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "src/idle.rs", IDLE);
    for (rel, content) in files {
        write(root, rel, content);
    }
    git(root, &["init", "-q", "-b", "base"]);
    assert!(keel(root, &["init"]).status.success());
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "update alpha"]);
    map(root);
    dir
}

fn clean_resync(root: &Path) {
    let out = keel(root, &["compile", "src/idle.rs"]);
    assert_eq!(out.status.code(), Some(0), "re-sync must succeed: {out:?}");
    assert!(
        out.stdout.is_empty() && out.stderr.is_empty(),
        "clean re-sync: {out:?}"
    );
    let marker: String = Connection::open(root.join(".keel/graph.db"))
        .unwrap()
        .query_row(
            "SELECT value FROM keel_meta WHERE key='last_map_commit'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(marker, head(root));
}

/// Stored `calls` edges from `Widget` in `pkg/beta.go` to `Alpha`.
fn widget_to_alpha(root: &Path) -> i64 {
    count(
        root,
        "SELECT count(*) FROM edges e JOIN nodes s ON s.id=e.source_id JOIN nodes t ON t.id=e.target_id WHERE e.kind='calls' AND s.file_path='pkg/beta.go' AND s.name='Widget' AND t.name='Alpha'",
    )
}

/// Alpha lives in `pkg/alpha.go` beside Beta; `pkg/beta.go` holds the caller text given.
fn caller_fixture(caller: &str) -> TempDir {
    fixture(&[
        ("go.mod", GO_MOD),
        ("pkg/alpha.go", &format!("{GO_ALPHA}{GO_BETA}")),
        ("pkg/beta.go", caller),
    ])
}

/// Commit ONLY the move of Alpha to `pkg/gadget.go`; the caller edit stays uncommitted.
fn move_alpha(root: &Path) {
    write(root, "pkg/alpha.go", GO_BETA);
    write(root, "pkg/gadget.go", GO_ALPHA);
    git(root, &["add", "pkg/alpha.go", "pkg/gadget.go"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "move alpha"]);
}

#[test]
fn a_line_shifted_dependent_keeps_one_edge() {
    let dir = caller_fixture(ONE_CALL);
    let root = dir.path();
    write(root, "pkg/beta.go", SHIFTED);
    move_alpha(root);
    clean_resync(root);
    assert_eq!(
        widget_to_alpha(root),
        1,
        "blob line and working-tree line are one call site, not two edges"
    );
    write(
        root,
        "pkg/gadget.go",
        &GO_ALPHA.replace("value int)", "value int, extra int)"),
    );
    let out = keel(root, &["compile", "pkg/gadget.go"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.code() == Some(1)
            && stdout.contains("E001")
            && stdout.contains("1 caller(s)")
            && !stdout.contains("2 caller(s)"),
        "E001 must report exactly one caller: {out:?}"
    );
}

#[test]
fn two_call_sites_in_the_working_tree_stay_two_edges() {
    let dir = caller_fixture(ONE_CALL);
    let root = dir.path();
    write(root, "pkg/beta.go", TWO_CALLS);
    let compiled = keel(root, &["compile", "pkg/beta.go"]);
    assert!(compiled.status.success(), "{compiled:?}");
    assert_eq!(widget_to_alpha(root), 2, "the compile stores both sites");
    move_alpha(root);
    clean_resync(root);
    assert_eq!(
        widget_to_alpha(root),
        2,
        "call sites within one text are never merged"
    );
}

#[test]
fn the_working_tree_wins_over_the_blob() {
    let dir = caller_fixture(TWO_CALLS);
    let root = dir.path();
    assert_eq!(widget_to_alpha(root), 2, "map stores the committed sites");
    write(root, "pkg/beta.go", ONE_CALL);
    let compiled = keel(root, &["compile", "pkg/beta.go"]);
    assert!(compiled.status.success(), "{compiled:?}");
    move_alpha(root);
    clean_resync(root);
    let resynced = widget_to_alpha(root);
    map(root);
    let oracle = widget_to_alpha(root);
    assert_eq!(oracle, 1, "the oracle sees one call in the working tree");
    assert_eq!(
        resynced, oracle,
        "the re-sync must store what a fresh map stores"
    );
}
