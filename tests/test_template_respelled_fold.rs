//! Round-one CLI regressions, with hooks disabled by the common fixture helpers.
#[path = "common/mod.rs"]
mod common;
use std::{fs, path::Path};
const HOME: &str = "fn home() -> &'static str { \"DOMAIN fixed::segment with punctuation\" }\n";
const COPY: &str = "const COPY: &str = \"DOMAIN fixed::segment with punctuation\";\n";
fn setup() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    common::git(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join("home.rs"), HOME).unwrap();
    fs::write(dir.path().join("legacy.py"), b"# latin-1\nTEXT = '\xe9'\n").unwrap();
    common::git(dir.path(), &["add", "."]);
    common::git(dir.path(), &["commit", "-qm", "base"]);
    assert!(common::keel(dir.path(), &["init", "--yes"])
        .status
        .success());
    map(dir.path());
    dir
}
fn map(dir: &Path) {
    assert!(common::keel(dir, &["map"]).status.success());
}
fn review(dir: &Path, gate: bool) -> serde_json::Value {
    let mut args = vec!["review", "--base", "HEAD", "--json", "--verbose"];
    if gate {
        args.push("--gate");
    }
    let out = common::keel(dir, &args);
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).unwrap()
}
fn count(value: &serde_json::Value) -> usize {
    value["template_advisories"].as_array().map_or(0, Vec::len)
}
#[test]
fn template_review_skips_non_utf8_file_and_retains_other_advisories() {
    let dir = setup();
    fs::write(
        dir.path().join("legacy.py"),
        b"# changed latin-1\nTEXT = '\xe9'\n",
    )
    .unwrap();
    fs::write(dir.path().join("caller.rs"), COPY).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    for gate in [false, true] {
        assert_eq!(count(&review(dir.path(), gate)), 1);
    }
    let out = common::keel(dir.path(), &["review", "--base", "HEAD", "--verbose"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("legacy.py"));
}
#[test]
fn template_review_renamed_remapped_owner_does_not_cancel_external_copy() {
    let dir = setup();
    fs::write(
        dir.path().join("home.rs"),
        HOME.replace("home()", "renamed()"),
    )
    .unwrap();
    fs::write(dir.path().join("caller.rs"), COPY).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    map(dir.path());
    assert_eq!(count(&review(dir.path(), true)), 1);
}
#[test]
fn template_review_deleted_home_is_not_an_owner() {
    let dir = setup();
    fs::remove_file(dir.path().join("home.rs")).unwrap();
    fs::write(dir.path().join("caller.rs"), COPY).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    assert_eq!(count(&review(dir.path(), true)), 0);
}
