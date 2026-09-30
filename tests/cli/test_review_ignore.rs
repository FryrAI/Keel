// Issue #90 round 3: `keel review` judges each side of a diff by its ROLE. A
// base-side path (a deletion, a rename's source) is decided by the ignore model
// alone, never by what occupies the path now; a head-side source path takes the
// walker's verdict; a file keel cannot parse is filtered by explicit ignore
// rules only, so it still reaches the `unanalyzed` list.

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

const CLEAN: &str = "def app(x: int) -> int:\n    \"\"\"Doc.\"\"\"\n    return x\n";
const VIOLATION: &str = "def compute(value):\n    return value\n";

/// A committed, initialised, mapped repo holding `files`.
fn repo(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    for (rel, content) in files {
        write(root, rel, content);
    }
    git(root, &["init", "-q"]);
    assert!(keel(root, &["init"]).status.success(), "keel init failed");
    git(root, &["add", "-f", "-A"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "base"]);
    assert!(keel(root, &["map"]).status.success(), "keel map failed");
    dir
}

fn gate_on_type_and_doc_errors(root: &Path) {
    let config_path = root.join(".keel/keel.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["review"] = serde_json::json!({ "gate": ["E002", "E003"] });
    fs::write(&config_path, config.to_string()).unwrap();
}

fn review_json(root: &Path, extra: &[&str]) -> (Option<i32>, serde_json::Value) {
    let mut args = vec!["review", "--base", "HEAD", "--json"];
    args.extend_from_slice(extra);
    let out = keel(root, &args);
    let json = serde_json::from_slice(&out.stdout).expect("review --json must emit JSON");
    (out.status.code(), json)
}

/// A file moved out of a hidden directory (never mapped) is new code at its
/// destination, so its violations gate.
fn assert_moved_in_is_new_code(root: &Path, dest: &str) {
    gate_on_type_and_doc_errors(root);
    assert_gates_as_new_code(root, dest);
}

/// The gate config is already in place (a linked worktree reads the main
/// checkout's `.keel`).
fn assert_gates_as_new_code(root: &Path, dest: &str) {
    let (code, json) = review_json(root, &["--gate"]);
    assert_eq!(code, Some(1), "new violations at {dest} must gate: {json}");
    assert!(
        !json["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["kind"] == "moved"),
        "a move out of an unmapped path is not a move: {json}"
    );
}

#[test]
fn rename_out_of_a_hidden_directory_reads_as_added() {
    let dir = repo(&[("src/app.py", CLEAN), (".github/scripts/h.py", VIOLATION)]);
    let root = dir.path();
    git(root, &["mv", ".github/scripts/h.py", "src/h.py"]);
    assert_moved_in_is_new_code(root, "src/h.py");
}

#[test]
fn rename_out_of_a_git_info_excluded_path_reads_as_added() {
    let dir = repo(&[("src/app.py", CLEAN), ("secret/h.py", VIOLATION)]);
    let root = dir.path();
    fs::write(root.join(".git/info/exclude"), "secret/\n").unwrap();
    git(root, &["mv", "secret/h.py", "src/h.py"]);
    assert_moved_in_is_new_code(root, "src/h.py");
}

#[test]
fn deleted_file_recreated_as_a_directory_is_still_a_deletion() {
    let dir = repo(&[
        ("src/app.py", CLEAN),
        ("src/gone.py", CLEAN.replace("app", "gone").as_str()),
    ]);
    let root = dir.path();
    fs::remove_file(root.join("src/gone.py")).unwrap();
    write(root, "src/gone.py/notes.txt", "now a directory\n");
    let (_, json) = review_json(root, &[]);
    assert!(
        json["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "gone" && c["kind"] == "removed"),
        "the deletion must stay in the ledger: {json}"
    );
}

#[test]
fn rename_source_recreated_as_a_directory_is_still_a_move() {
    let dir = repo(&[
        ("src/app.py", CLEAN),
        ("src/old.py", CLEAN.replace("app", "old").as_str()),
    ]);
    let root = dir.path();
    git(root, &["mv", "src/old.py", "src/new.py"]);
    write(root, "src/old.py/notes.txt", "now a directory\n");
    let (_, json) = review_json(root, &[]);
    let changes = json["changes"].as_array().unwrap();
    assert!(
        changes
            .iter()
            .any(|c| c["name"] == "old" && c["kind"] == "moved" && c["from"] == "src/old.py"),
        "the rename must stay a move: {json}"
    );
}

#[test]
fn changed_workflow_file_is_listed_unanalyzed() {
    let dir = repo(&[
        ("src/app.py", CLEAN),
        (".github/workflows/ci.yml", "on: push\n"),
    ]);
    let root = dir.path();
    write(
        root,
        ".github/workflows/ci.yml",
        "on: [push, pull_request]\n",
    );
    let out = keel(root, &["review", "--base", "HEAD", "--llm"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("UNANALYZED .github/workflows/ci.yml [unparsed]"),
        "a hidden non-source file must still be named as unanalyzed: {stdout}"
    );
}

/// A tracked regular file replaced by a symlink is a git type change: the head
/// side is not a regular file, so only the base contracts remain, as removals.
#[cfg(unix)]
#[test]
fn type_change_to_a_symlink_keeps_the_base_removal() {
    let dir = repo(&[
        ("src/app.py", CLEAN),
        ("src/old.py", CLEAN.replace("app", "old_fn").as_str()),
    ]);
    let root = dir.path();
    fs::remove_file(root.join("src/old.py")).unwrap();
    std::os::unix::fs::symlink("app.py", root.join("src/old.py")).unwrap();
    let (_, json) = review_json(root, &[]);
    assert!(
        json["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "old_fn" && c["kind"] == "removed"),
        "the type change must keep the removal: {json}"
    );
}

/// A linked worktree reads the main repository's `info/exclude`, as the walker
/// does, so a rename out of a path it excludes is new code.
#[test]
fn rename_out_of_a_linked_worktree_excluded_path_reads_as_added() {
    let dir = repo(&[("src/app.py", CLEAN), ("excl/e.py", VIOLATION)]);
    let main = dir.path();
    fs::write(main.join(".git/info/exclude"), "excl/\n").unwrap();
    let wt_holder = TempDir::new().unwrap();
    let wt = wt_holder.path().join("wt");
    git(
        main,
        &["worktree", "add", "-q", "-b", "wtb", wt.to_str().unwrap()],
    );
    git(&wt, &["mv", "excl/e.py", "src/e.py"]);
    assert!(keel(&wt, &["map"]).status.success(), "keel map failed");
    gate_on_type_and_doc_errors(main);
    assert_gates_as_new_code(&wt, "src/e.py");
}
