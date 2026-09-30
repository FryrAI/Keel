//! Phase-1 template study exporter: real CLI, map cache, and Git baselines.
#[path = "common/mod.rs"]
mod common;

use serde_json::Value;
use std::{fs, path::Path};

const HOME: &str =
    "fn berlin(x: &str) -> String { format!(\"({x} AT TIME ZONE 'Europe/Berlin')::date\") }\n";
const CALLER: &str = "const SQL: &str = \"SELECT (now() AT TIME ZONE 'Europe/Berlin')::date\";\n";

fn setup() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    common::git(dir.path(), &["init", "-q"]);
    common::git(
        dir.path(),
        &["commit", "--allow-empty", "-qm", "empty base"],
    );
    common::git(dir.path(), &["tag", "empty-base"]);
    fs::write(dir.path().join("home.rs"), HOME).unwrap();
    common::git(dir.path(), &["add", "home.rs"]);
    common::git(dir.path(), &["commit", "-qm", "home"]);
    assert!(common::keel(dir.path(), &["init", "--yes"])
        .status
        .success());
    dir
}

fn map(dir: &Path) {
    assert!(common::keel(dir, &["map", "--json"]).status.success());
}

fn review(dir: &Path, base: &str) -> Value {
    let output = common::keel(dir, &["review", "--base", base, "--json", "--gate"]);
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn count(result: &Value) -> usize {
    result["template_study"]["occurrences"]
        .as_array()
        .map_or(0, Vec::len)
}

#[test]
fn template_review_reads_unchanged_mapped_home_and_empty_base_exports_all() {
    let dir = setup();
    fs::write(dir.path().join("caller.rs"), CALLER).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    map(dir.path());
    let result = review(dir.path(), "HEAD");
    assert_eq!(count(&result), 1);
    let study = &result["template_study"];
    assert_eq!(study["template_functions"], 1);
    assert_eq!(study["kept_segments"], 1);
    assert_eq!(study["occurrences"][0]["homes"][0]["name"], "berlin");
    assert_eq!(study["occurrences"][0]["file"], "caller.rs");
    assert!(result["new_violations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["code"] != "W012"));
    assert_eq!(count(&review(dir.path(), "empty-base")), 1);
    common::git(dir.path(), &["commit", "-qm", "caller"]);
    assert_eq!(count(&review(dir.path(), "HEAD")), 0);
}

#[test]
fn template_review_subtracts_renames_moves_and_counts_extra_copies() {
    let dir = setup();
    fs::write(dir.path().join("caller.rs"), CALLER).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    common::git(dir.path(), &["commit", "-qm", "caller"]);
    map(dir.path());
    common::git(dir.path(), &["mv", "caller.rs", "moved.rs"]);
    common::git(dir.path(), &["mv", "home.rs", "new-home.rs"]);
    map(dir.path());
    assert_eq!(count(&review(dir.path(), "HEAD")), 0);
    fs::write(
        dir.path().join("moved.rs"),
        format!("{CALLER}{}", CALLER.replace("SQL", "COPY")),
    )
    .unwrap();
    assert_eq!(count(&review(dir.path(), "HEAD")), 1);
    fs::remove_file(dir.path().join("moved.rs")).unwrap();
    fs::write(dir.path().join("destination.rs"), CALLER).unwrap();
    common::git(dir.path(), &["add", "destination.rs", "moved.rs"]);
    assert_eq!(count(&review(dir.path(), "HEAD")), 0);
}

#[test]
fn template_cache_is_map_derived_and_ignored_homes_never_enter_it() {
    let dir = setup();
    fs::write(dir.path().join(".keelignore"), "ignored.rs\n").unwrap();
    fs::write(
        dir.path().join("ignored.rs"),
        HOME.replace("berlin", "ignored"),
    )
    .unwrap();
    map(dir.path());
    fs::write(dir.path().join("caller.rs"), CALLER).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    assert_eq!(count(&review(dir.path(), "HEAD")), 1);
    fs::write(dir.path().join("home.rs"), "fn empty() {}\n").unwrap();
    assert_eq!(
        count(&review(dir.path(), "HEAD")),
        1,
        "home cache stays as of last map"
    );
    map(dir.path());
    let result = review(dir.path(), "HEAD");
    assert_eq!(count(&result), 0);
    assert!(result.get("template_study").is_none());
}

#[cfg(unix)]
#[test]
fn template_review_skips_symlink_text_on_both_sides() {
    use std::os::unix::fs::symlink;
    let dir = setup();
    fs::write(dir.path().join("caller.rs"), CALLER).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    common::git(dir.path(), &["commit", "-qm", "caller"]);
    map(dir.path());
    fs::remove_file(dir.path().join("caller.rs")).unwrap();
    symlink("home.rs", dir.path().join("caller.rs")).unwrap();
    assert_eq!(count(&review(dir.path(), "HEAD")), 0);
    common::git(dir.path(), &["add", "caller.rs"]);
    common::git(dir.path(), &["commit", "-qm", "symlink"]);
    fs::remove_file(dir.path().join("caller.rs")).unwrap();
    fs::write(dir.path().join("caller.rs"), CALLER).unwrap();
    assert_eq!(count(&review(dir.path(), "HEAD")), 1);
}

#[test]
fn template_review_without_segments_does_not_read_unavailable_blobs() {
    let dir = setup();
    fs::write(dir.path().join("home.rs"), "fn empty() {}\n").unwrap();
    map(dir.path());
    fs::write(dir.path().join("caller.rs"), b"fn query() {}\n\xff\n").unwrap();
    common::git(dir.path(), &["add", "home.rs", "caller.rs"]);
    common::git(dir.path(), &["commit", "-qm", "unavailable base"]);
    fs::write(dir.path().join("caller.rs"), CALLER).unwrap();
    assert_eq!(count(&review(dir.path(), "HEAD")), 0);
}
