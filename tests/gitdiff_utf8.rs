//! Unix regressions for changed paths that cannot be represented as UTF-8.
#![cfg(unix)]

#[path = "common/mod.rs"]
mod common;

use common::{git, keel};
use keel_enforce::gitdiff::{
    changed_files_checked, changed_paths, head_commit, ChangeStatus, ChangedPath, DiffMode,
};
use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use tempfile::TempDir;

const ALIAS: &str = "src/\u{FFFD}.rs";
const INVALID: &[u8] = b"src/\xff.rs";
const WARNING: &str = "keel: skipping a changed path that is not valid UTF-8: src/\\xff.rs";

fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q"]);
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/base.rs"), "fn base() {}\n").unwrap();
    keel(dir.path(), &["init"]);
    let config_path = dir.path().join(".keel/keel.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["homes"] = serde_json::json!([
        {"name":"sql","patterns":["CURRENT_DATE"],"home":"src/home/**","scope":"src/**"}
    ]);
    fs::write(config_path, config.to_string()).unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "base"]);
    dir
}

fn stage_invalid(root: &Path, alias: bool) {
    fs::write(root.join(OsStr::from_bytes(INVALID)), "fn invalid() {}\n").unwrap();
    if alias {
        // Untracked, so it must never be selected on behalf of the staged byte path.
        fs::write(root.join(ALIAS), "const SQL: &str = \"CURRENT_DATE\";\n").unwrap();
    }
    // Stage by directory because common::git deliberately accepts UTF-8 arguments.
    git(root, &["add", "src"]);
    if alias {
        git(root, &["restore", "--staged", ALIAS]);
    }
}

#[test]
fn non_utf8_name_only_paths_never_alias_a_valid_sibling_in_any_diff_mode() {
    let dir = fixture();
    let root = dir.path();
    let base = head_commit(root).unwrap();
    stage_invalid(root, true);
    fs::write(root.join("src/ok.rs"), "fn ok() {}\n").unwrap();
    git(root, &["add", "src/ok.rs"]);
    for supported in [false, true] {
        for mode in [DiffMode::Since(None), DiffMode::Staged] {
            assert_eq!(
                changed_files_checked(root, &mode, supported).unwrap(),
                ["src/ok.rs"]
            );
        }
    }
    git(root, &["commit", "-q", "-m", "byte path"]);
    assert_eq!(
        changed_files_checked(root, &DiffMode::Range(base), false).unwrap(),
        ["src/ok.rs"]
    );
}

#[test]
fn non_utf8_initial_commit_fallback_keeps_only_valid_paths() {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q"]);
    fs::create_dir(dir.path().join("src")).unwrap();
    stage_invalid(dir.path(), false);
    fs::write(dir.path().join(ALIAS), "fn sibling() {}\n").unwrap();
    fs::write(dir.path().join("src/ok.rs"), "fn ok() {}\n").unwrap();
    git(dir.path(), &["add", "src/ok.rs"]);
    assert_eq!(
        changed_files_checked(dir.path(), &DiffMode::Since(None), true).unwrap(),
        ["src/ok.rs"]
    );
}

#[test]
fn non_utf8_name_status_paths_never_alias_a_valid_sibling() {
    let dir = fixture();
    stage_invalid(dir.path(), true);
    fs::write(dir.path().join("src/ok.rs"), "fn ok() {}\n").unwrap();
    git(dir.path(), &["add", "src/ok.rs"]);
    let paths = changed_paths(dir.path(), "HEAD").unwrap();
    assert_eq!(paths.len(), 1, "{paths:?}");
    assert_eq!(paths[0].path, "src/ok.rs");
}

#[test]
fn utf8_replacement_character_sibling_remains_a_distinct_changed_path_control() {
    let dir = fixture();
    stage_invalid(dir.path(), true);
    git(dir.path(), &["add", ALIAS]);
    assert_eq!(
        changed_files_checked(dir.path(), &DiffMode::Staged, true).unwrap(),
        [ALIAS]
    );
    let paths = changed_paths(dir.path(), "HEAD").unwrap();
    assert_eq!(paths.len(), 1, "{paths:?}");
    assert_eq!(paths[0].path, ALIAS);
}

#[test]
fn non_utf8_rename_endpoints_keep_the_valid_side_and_preserve_adjacent_records() {
    for invalid_base in [false, true] {
        let dir = fixture();
        let root = dir.path();
        let invalid = root.join(OsStr::from_bytes(b"src/a\xff.rs"));
        let valid = root.join("src/a-renamed.rs");
        let (from, to) = if invalid_base {
            (&invalid, &valid)
        } else {
            (&valid, &invalid)
        };
        fs::write(from, "fn moved() {}\n").unwrap();
        git(root, &["add", "src"]);
        git(root, &["commit", "-q", "-m", "rename base"]);
        fs::rename(from, to).unwrap();
        fs::write(root.join("src/ok.rs"), "fn ok() {}\n").unwrap();
        git(root, &["add", "-A", "src"]);
        let paths = changed_paths(root, "HEAD").unwrap();
        assert_eq!(
            paths,
            [
                ChangedPath {
                    path: "src/a-renamed.rs".into(),
                    status: if invalid_base {
                        ChangeStatus::Added
                    } else {
                        ChangeStatus::Deleted
                    },
                },
                ChangedPath {
                    path: "src/ok.rs".into(),
                    status: ChangeStatus::Added,
                },
            ]
        );
        let out = keel(root, &["review", "--base", "HEAD", "--json"]);
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(
            stderr
                .matches("keel: skipping a changed path that is not valid UTF-8: src/a\\xff.rs")
                .count(),
            1,
            "{stderr}"
        );
    }
}

fn assert_warning_once(out: &std::process::Output) {
    let stderr = String::from_utf8(out.stderr.clone()).unwrap();
    assert_eq!(stderr.matches(WARNING).count(), 1, "{out:?}");
    assert!(
        !stderr.contains(ALIAS),
        "warning must preserve byte identity"
    );
}

fn assert_cli_skips_invalid(alias: bool) {
    let dir = fixture();
    stage_invalid(dir.path(), alias);
    let out = keel(dir.path(), &["compile", "--changed", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        out.stdout.is_empty(),
        "an alias must not be compiled: {out:?}"
    );
    assert_warning_once(&out);
    let out = keel(dir.path(), &["review", "--base", "HEAD", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_warning_once(&out);
    let review: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(review["files_changed"], 0, "{review}");
    assert_eq!(review["files_analyzed"], 0, "{review}");
    for args in [
        vec!["audit", "--changed", "--json"],
        vec!["checkpoint", "--json"],
    ] {
        let out = keel(dir.path(), &args);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert_warning_once(&out);
        assert!(!String::from_utf8_lossy(&out.stdout).contains(ALIAS));
    }
}

#[test]
fn non_utf8_cli_changed_path_warns_without_reading_replacement_character_sibling() {
    assert_cli_skips_invalid(true);
}

#[test]
fn non_utf8_cli_lone_changed_path_warns_instead_of_silently_disappearing() {
    assert_cli_skips_invalid(false);
}

const MOVED: &str =
    "/// Query the civil day.\npub fn moved() {\n    let _ = \"CURRENT_DATE\";\n}\n";

#[test]
fn non_utf8_rename_source_review_gate_checks_the_valid_destination() {
    let dir = fixture();
    let root = dir.path();
    fs::write(root.join(OsStr::from_bytes(INVALID)), MOVED).unwrap();
    git(root, &["add", "src"]);
    git(root, &["commit", "-q", "-m", "rename base"]);
    fs::rename(
        root.join(OsStr::from_bytes(INVALID)),
        root.join("src/new.rs"),
    )
    .unwrap();
    fs::write(
        root.join("src/new.rs"),
        MOVED.replace("}\n", "    let _ = \"CURRENT_DATE\";\n}\n"),
    )
    .unwrap();
    let config_path = root.join(".keel/keel.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["review"]["gate"] = serde_json::json!(["W011"]);
    fs::write(config_path, config.to_string()).unwrap();
    git(root, &["add", "src"]);
    let out = keel(root, &["review", "--base", "HEAD", "--gate", "--json"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_warning_once(&out);
    let review: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(review["files_analyzed"], 1, "{review}");
    let hits = review["new_violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["code"] == "W011")
        .collect::<Vec<_>>();
    assert_eq!(hits.len(), 2, "{review}");
    assert!(hits.iter().all(|v| v["file"] == "src/new.rs"));
    assert!(
        review["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| { c["name"] == "moved" && c["kind"] == "added" && c["file"] == "src/new.rs" }),
        "{review}"
    );
}

#[test]
fn non_utf8_rename_destination_review_lists_the_valid_source_removal() {
    let dir = fixture();
    let root = dir.path();
    fs::write(root.join("src/old.rs"), MOVED).unwrap();
    git(root, &["add", "src"]);
    git(root, &["commit", "-q", "-m", "rename base"]);
    fs::rename(
        root.join("src/old.rs"),
        root.join(OsStr::from_bytes(INVALID)),
    )
    .unwrap();
    git(root, &["add", "-A", "src"]);
    let out = keel(root, &["review", "--base", "HEAD", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_warning_once(&out);
    let review: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(review["files_analyzed"], 1, "{review}");
    assert!(
        review["changes"].as_array().unwrap().iter().any(|c| {
            c["name"] == "moved" && c["kind"] == "removed" && c["file"] == "src/old.rs"
        }),
        "{review}"
    );
}

fn matching_sibling_fixture() -> TempDir {
    let dir = fixture();
    fs::write(dir.path().join("src/lib.rs"), MOVED).unwrap();
    git(dir.path(), &["add", "src/lib.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "homes baseline"]);
    fs::write(dir.path().join("src/lib.rs"), format!("{MOVED}\n")).unwrap();
    dir
}

fn assert_clean_warning_once(out: &std::process::Output) {
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    assert_warning_once(out);
}

#[test]
fn non_utf8_compile_changed_with_matching_sibling_warns_once() {
    let dir = matching_sibling_fixture();
    stage_invalid(dir.path(), false);
    assert_clean_warning_once(&keel(dir.path(), &["compile", "--changed", "--json"]));
}

#[test]
fn non_utf8_compile_since_with_matching_sibling_warns_once() {
    let dir = matching_sibling_fixture();
    let base = head_commit(dir.path()).unwrap();
    stage_invalid(dir.path(), false);
    git(dir.path(), &["commit", "-q", "-m", "changed paths"]);
    assert_clean_warning_once(&keel(dir.path(), &["compile", "--since", &base, "--json"]));
}

fn assert_filtered_path_is_silent(root: &Path) {
    let base = head_commit(root).unwrap();
    git(root, &["add", "-f", "."]);
    git(root, &["commit", "-q", "-m", "filtered path"]);
    for args in [
        vec!["compile", "--since", &base, "--json"],
        vec!["review", "--base", &base, "--json"],
    ] {
        let out = keel(root, &args);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert!(
            !String::from_utf8_lossy(&out.stderr).contains("not valid UTF-8"),
            "{out:?}"
        );
    }
    // Exercise selection and the homes rename scan against working-tree edits,
    // including the hook-style invocation with an explicitly selected sibling.
    fs::write(root.join("src/lib.rs"), format!("{MOVED}\n\n")).unwrap();
    for path in [b"gen/\xff.rs".as_slice(), b"docs/\xff.md".as_slice()] {
        let path = root.join(OsStr::from_bytes(path));
        if path.exists() {
            fs::write(path, "changed\n").unwrap();
        }
    }
    for args in [
        vec!["compile", "--changed", "--json"],
        vec!["compile", "src/lib.rs", "--json"],
        vec!["review", "--base", "HEAD", "--json"],
        vec!["audit", "--changed", "--json"],
        vec!["checkpoint", "--json"],
    ] {
        let out = keel(root, &args);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert!(
            !String::from_utf8_lossy(&out.stderr).contains("not valid UTF-8"),
            "{out:?}"
        );
        if args[0] == "compile" {
            assert!(out.stdout.is_empty(), "{out:?}");
        }
    }
}

#[test]
fn non_utf8_ignored_source_paths_do_not_warn() {
    for ignore_file in [".gitignore", ".keelignore"] {
        let dir = matching_sibling_fixture();
        fs::write(dir.path().join(ignore_file), "gen/\n").unwrap();
        fs::create_dir(dir.path().join("gen")).unwrap();
        fs::write(dir.path().join(OsStr::from_bytes(b"gen/\xff.rs")), MOVED).unwrap();
        assert_filtered_path_is_silent(dir.path());
    }
}

#[test]
fn non_utf8_non_source_paths_do_not_warn() {
    let dir = matching_sibling_fixture();
    fs::create_dir(dir.path().join("docs")).unwrap();
    fs::write(
        dir.path().join(OsStr::from_bytes(b"docs/\xff.md")),
        "notes\n",
    )
    .unwrap();
    assert_filtered_path_is_silent(dir.path());
}
