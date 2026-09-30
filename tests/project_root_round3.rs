//! Round-three regressions for local audit checks and original skeleton reads.

#[path = "common/mod.rs"]
mod common;

use common::{git, keel};
use serde_json::Value;
use std::fs;
use std::process::Command;

#[test]
fn package_audit_keeps_agent_verification_and_workflow_checks_local() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let pkg = root.join("pkg");
    fs::create_dir_all(pkg.join(".claude")).unwrap();
    fs::write(
        pkg.join("lib.rs"),
        "//! Purpose: local API.\n/// Local.\npub fn local() {}\n",
    )
    .unwrap();
    fs::write(pkg.join("Makefile"), "test:\n\tcargo test\n").unwrap();
    fs::write(pkg.join(".eslintrc.json"), "{}").unwrap();
    fs::create_dir_all(pkg.join("tests")).unwrap();
    git(root, &["init", "-q"]);
    assert!(keel(&pkg, &["init", "--yes"]).status.success());
    assert!(keel(&pkg, &["map"]).status.success());
    let out = keel(&pkg, &["audit", "--json"]);
    assert!(out.status.success());
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    let findings: Vec<_> = result["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|dim| dim["findings"].as_array().unwrap())
        .collect();
    for rule in [
        "no_agent_instructions",
        "no_agent_dir",
        "missing_file_header",
        "no_dev_workflow_tool",
        "has_tests",
        "has_lint_config",
    ] {
        assert!(
            !findings.iter().any(|f| f["check"] == rule),
            "{rule}: {result}"
        );
    }
}

#[test]
fn package_skeleton_preserves_successful_argument_spelling() {
    let dir = tempfile::tempdir().unwrap();
    let pkg = dir.path().join("pkg");
    fs::create_dir_all(pkg.join("src")).unwrap();
    fs::write(
        pkg.join("src/lib.rs"),
        "/// Present.\npub fn present() {}\n",
    )
    .unwrap();
    git(dir.path(), &["init", "-q"]);
    for argument in ["src/lib.rs", "./src/lib.rs"] {
        let out = keel(&pkg, &["skeleton", argument, "--json"]);
        assert!(out.status.success());
        let result: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(result["file"], argument);
        assert_eq!(result["symbols"][0]["name"], "present");
    }
}

#[test]
fn skeleton_preserves_unreadable_argument_spelling_and_os_resolution() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("pkg/src")).unwrap();
    fs::write(
        dir.path().join("pkg/src/lib.rs"),
        "/// Present.\npub fn present() {}\n",
    )
    .unwrap();
    git(dir.path(), &["init", "-q"]);
    for argument in ["missing/../pkg/src/lib.rs", "./missing.rs"] {
        // These literal messages and exit codes match the 0.6.2 comparator.
        // Obtain only the platform-specific OS error text from the actual read.
        let error = fs::read_to_string(dir.path().join(argument)).unwrap_err();
        let out = Command::new(common::keel_bin())
            .args(["skeleton", argument, "--json"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{argument}");
        assert!(out.stdout.is_empty(), "{argument}");
        assert_eq!(
            String::from_utf8(out.stderr).unwrap(),
            format!("keel skeleton: cannot read {argument}: {error}\n")
        );
    }
}
