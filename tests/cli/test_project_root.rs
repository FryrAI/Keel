//! Compile must use the same graph identities from every worktree directory.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use tempfile::TempDir;

use crate::common::{git, keel};

const BASE: &str = "/// Double a value.\npub fn target(value: i32) -> i32 { value * 2 }\n";
const CALLER: &str =
    "use crate::target;\n/// Call the target.\npub fn caller() -> i32 { target(7) }\n";

fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/lib.rs"), BASE).unwrap();
    fs::write(dir.path().join("src/caller.rs"), CALLER).unwrap();
    git(dir.path(), &["init", "-q"]);
    keel(dir.path(), &["init"]);
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "base"]);
    assert!(keel(dir.path(), &["map"]).status.success());
    dir
}

fn graph_paths(root: &Path) -> BTreeSet<String> {
    let db = rusqlite::Connection::open(root.join(".keel/graph.db")).unwrap();
    let mut query = db.prepare("SELECT DISTINCT file_path FROM nodes").unwrap();
    query
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn parity(changed: &str, code: &str) {
    for mode in ["changed", "since", "bare", "explicit"] {
        let dir = fixture();
        let root = dir.path();
        let db = root.join(".keel/graph.db");
        let baseline = root.join("baseline.db");
        fs::copy(&db, &baseline).unwrap();
        let mapped_paths = graph_paths(root);
        fs::write(root.join("src/lib.rs"), changed).unwrap();
        if mode == "since" {
            git(root, &["add", "src/lib.rs"]);
            git(root, &["commit", "-q", "-m", "change"]);
        }
        let mut args = vec!["compile", "--llm"];
        match mode {
            "changed" => args.push("--changed"),
            "since" => args.extend(["--since", "HEAD~1"]),
            "explicit" => args.push("src/lib.rs"),
            _ => {}
        }
        let from_root = keel(root, &args);
        assert_eq!(from_root.status.code(), Some(1), "{mode}: {code}");
        let stdout = String::from_utf8_lossy(&from_root.stdout);
        assert!(stdout.contains(code), "{mode}: expected {code}: {stdout}");
        assert!(!stdout.contains("W002"), "phantom duplicate: {stdout}");
        let root_paths = graph_paths(root);
        assert_eq!(
            mapped_paths, root_paths,
            "root compile changed file identities"
        );
        fs::copy(&baseline, &db).unwrap();
        if mode == "explicit" {
            *args.last_mut().unwrap() = "lib.rs";
        }
        let from_subdir = keel(&root.join("src"), &args);
        assert_eq!(from_subdir.status.code(), from_root.status.code(), "{mode}");
        assert_eq!(from_subdir.stdout, from_root.stdout, "{mode}: {code}");
        assert_eq!(graph_paths(root), root_paths, "{mode}: {code}");
    }
}

#[test]
fn compile_subdirectory_docstrings_match_root() {
    parity("pub fn target(value: i32) -> i32 { value * 3 }\n", "E003");
}

#[test]
fn compile_subdirectory_removed_functions_match_root() {
    parity(
        "/// Replacement.\npub fn replacement() -> i32 { 21 }\n",
        "E004",
    );
}

#[test]
fn compile_subdirectory_changed_signatures_match_root() {
    parity(
        "/// Double a value.\npub fn target(value: i32, extra: i32) -> i32 { value * 2 + extra }\n",
        "E001",
    );
}

#[test]
fn compile_subdirectory_normalizes_explicit_parent_components() {
    let dir = fixture();
    fs::write(dir.path().join("src/lib.rs"), "pub fn undocumented() {}\n").unwrap();
    let out = keel(
        &dir.path().join("src"),
        &["compile", "../src/./lib.rs", "lib.rs", "--json"],
    );
    assert_eq!(out.status.code(), Some(1));
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["files_analyzed"], serde_json::json!(["src/lib.rs"]));
    assert!(graph_paths(dir.path())
        .iter()
        .all(|p| p.starts_with("src/")));
}

#[test]
fn compile_subdirectory_batch_keeps_root_relative_deferred_files() {
    let dir = fixture();
    fs::write(dir.path().join("src/lib.rs"), "pub fn undocumented() {}\n").unwrap();
    assert!(keel(dir.path(), &["compile", "--batch-start"])
        .status
        .success());
    let mid = keel(&dir.path().join("src"), &["compile", "lib.rs", "--json"]);
    let result: serde_json::Value = serde_json::from_slice(&mid.stdout).unwrap();
    assert!(crate::common::violations_with_code(&result, "E003").is_empty());
    let end = keel(dir.path(), &["compile", "--batch-end", "--json"]);
    let result: serde_json::Value = serde_json::from_slice(&end.stdout).unwrap();
    let hits = crate::common::violations_with_code(&result, "E003");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["file"], "src/lib.rs");
}

#[test]
fn compile_subdirectory_delta_matches_root_snapshot_keys() {
    let dir = fixture();
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn target(value: i32) -> i32 { value * 3 }\n",
    )
    .unwrap();
    assert_eq!(
        keel(dir.path(), &["compile", "src/lib.rs", "--json"])
            .status
            .code(),
        Some(1)
    );
    let out = keel(
        &dir.path().join("src"),
        &["compile", "lib.rs", "--delta", "--json"],
    );
    assert_eq!(out.status.code(), Some(0));
    let delta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        delta["new_errors"].as_array().unwrap().is_empty(),
        "{delta}"
    );
    assert!(
        delta["resolved_errors"].as_array().unwrap().is_empty(),
        "{delta}"
    );
}
