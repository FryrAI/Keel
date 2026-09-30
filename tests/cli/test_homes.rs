//! W011/E007 edit-loop and committed review-gate regressions.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{json, Value};
use tempfile::TempDir;

use crate::common::{compile_json, git, keel, keel_bin, violations_with_code};

const BASE: &str = "fn query() {\n    let _ = \"CURRENT_DATE\";\n}\n";
const ADDED: &str =
    "fn query() {\n    let _ = \"CURRENT_DATE\";\n    let _ = \"CURRENT_DATE\";\n}\n";

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn config(root: &Path, severity: &str, gate: &[&str]) {
    write(root, ".keel/keel.json", &json!({
        "version": env!("CARGO_PKG_VERSION"), "languages": ["rust"],
        "enforce": {"docstrings": false, "dead_code": false, "duplication": false, "homes": severity},
        "telemetry": {"enabled": false}, "review": {"gate": gate},
        "homes": [{"name": "civil-day", "patterns": ["CURRENT_DATE"],
            "home": "src/time.rs", "scope": "src"}]
    }).to_string());
}

fn fixture(committed: bool) -> TempDir {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "src/lib.rs", BASE);
    write(dir.path(), "src/time.rs", BASE);
    git(dir.path(), &["init", "-q"]);
    keel(dir.path(), &["init"]);
    config(dir.path(), "warning", &[]);
    if committed {
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-q", "-m", "base"]);
    }
    keel(dir.path(), &["map"]);
    dir
}

fn parse(out: &Output) -> Value {
    if out.stdout.is_empty() {
        json!({"errors": [], "warnings": [], "new_violations": []})
    } else {
        serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)))
    }
}

fn review(root: &Path, base: &str, gate: bool) -> Output {
    let mut args = vec!["review", "--base", base, "--json", "--verbose"];
    if gate {
        args.push("--gate");
    }
    Command::new(keel_bin())
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

fn review_hits(out: &Output, code: &str) -> Vec<Value> {
    parse(out)["new_violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["code"] == code)
        .cloned()
        .collect()
}

#[test]
fn homes_compile_grandfathers_committed_lines_and_reports_new_line() {
    let dir = fixture(true);
    assert!(violations_with_code(&compile_json(dir.path(), "src/lib.rs"), "W011").is_empty());
    write(dir.path(), "src/lib.rs", ADDED);
    let hits = violations_with_code(&compile_json(dir.path(), "src/lib.rs"), "W011");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["line"], 3);
    assert_eq!(hits[0]["file"], "src/lib.rs");
    assert_eq!(hits[0]["hash"], "");
    assert_eq!(hits[0]["confidence"], 1.0);
}

#[test]
fn homes_compile_home_and_out_of_scope_are_silent_but_untracked_source_fires() {
    let dir = fixture(true);
    write(dir.path(), "src/time.rs", ADDED);
    write(dir.path(), "outside/lib.rs", ADDED);
    write(dir.path(), "src/new.rs", BASE);
    for path in ["src/time.rs", "outside/lib.rs"] {
        assert!(violations_with_code(&compile_json(dir.path(), path), "W011").is_empty());
    }
    assert_eq!(
        violations_with_code(&compile_json(dir.path(), "src/new.rs"), "W011").len(),
        1
    );
}

#[test]
fn homes_compile_line_move_and_whitespace_are_silent() {
    let dir = fixture(true);
    write(
        dir.path(),
        "src/lib.rs",
        "\n\nfn query() {\n\tlet   _ =  \"CURRENT_DATE\";\n}\n",
    );
    assert!(violations_with_code(&compile_json(dir.path(), "src/lib.rs"), "W011").is_empty());
}

#[test]
fn homes_compile_suppression_and_warning_batch_use_ordinary_pipeline() {
    let dir = fixture(true);
    write(dir.path(), "src/lib.rs", ADDED);
    let out = keel(
        dir.path(),
        &["compile", "src/lib.rs", "--json", "--suppress", "W011"],
    );
    let result = parse(&out);
    let hits = violations_with_code(&result, "S001");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["suppressed"], true);
    assert!(out.status.success());
    keel(dir.path(), &["compile", "--batch-start"]);
    assert!(violations_with_code(&compile_json(dir.path(), "src/lib.rs"), "W011").is_empty());
    let out = keel(dir.path(), &["compile", "--batch-end", "--json"]);
    assert_eq!(violations_with_code(&parse(&out), "W011").len(), 1);
}

#[test]
fn homes_compile_error_gates_immediately_even_in_batch_and_delta_tracks_lines() {
    let dir = fixture(true);
    config(dir.path(), "error", &[]);
    write(dir.path(), "src/lib.rs", ADDED);
    keel(dir.path(), &["compile", "--batch-start"]);
    let out = keel(dir.path(), &["compile", "src/lib.rs", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let hits = violations_with_code(&parse(&out), "E007");
    assert_eq!(hits.len(), 1);
    assert!(hits[0]["fix_hint"].is_string());
    keel(dir.path(), &["compile", "--batch-end"]);
    let out = keel(dir.path(), &["compile", "src/lib.rs", "--delta", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let out = keel(dir.path(), &["compile", "src/lib.rs", "--delta", "--json"]);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn homes_compile_since_and_subdirectory_use_repository_paths() {
    let dir = fixture(true);
    write(dir.path(), "src/lib.rs", ADDED);
    git(dir.path(), &["add", "src/lib.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "add occurrence"]);
    assert!(violations_with_code(&compile_json(dir.path(), "src/lib.rs"), "W011").is_empty());
    let out = keel(dir.path(), &["compile", "--since", "HEAD~1", "--json"]);
    let hits = violations_with_code(&parse(&out), "W011");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["line"], 3);
    assert_eq!(hits[0]["file"], "src/lib.rs");
    let out = keel(
        &dir.path().join("src"),
        &["compile", "--since", "HEAD~1", "--json"],
    );
    assert_eq!(violations_with_code(&parse(&out), "W011"), hits);
    write(dir.path(), "src/new.rs", BASE);
    let hits = violations_with_code(&compile_json(&dir.path().join("src"), "./new.rs"), "W011");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["file"], "src/new.rs");
}

#[test]
fn homes_compile_no_repository_and_unborn_head_are_silent_with_one_note() {
    let unborn = fixture(false);
    write(unborn.path(), "src/lib.rs", ADDED);
    let out = keel(
        unborn.path(),
        &[
            "compile",
            "src/lib.rs",
            "src/time.rs",
            "--json",
            "--verbose",
        ],
    );
    assert!(violations_with_code(&parse(&out), "W011").is_empty());
    assert_eq!(
        String::from_utf8_lossy(&out.stderr)
            .matches("homes skipped:")
            .count(),
        1
    );
    let dir = TempDir::new().unwrap();
    write(dir.path(), "src/lib.rs", ADDED);
    keel(dir.path(), &["init"]);
    config(dir.path(), "warning", &[]);
    keel(dir.path(), &["map"]);
    let out = keel(
        dir.path(),
        &["compile", "src/lib.rs", "--json", "--verbose"],
    );
    assert!(violations_with_code(&parse(&out), "W011").is_empty());
    assert_eq!(
        String::from_utf8_lossy(&out.stderr)
            .matches("homes skipped:")
            .count(),
        1
    );
}

#[test]
fn homes_review_committed_surplus_is_exact_and_warning_gate_is_load_bearing() {
    let dir = fixture(true);
    write(dir.path(), "src/lib.rs", ADDED);
    git(dir.path(), &["add", "src/lib.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "add occurrence"]);
    // A fresh head-side map must not grandfather the PR's own additions.
    keel(dir.path(), &["map"]);
    let out = review(dir.path(), "HEAD~1", true);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(review_hits(&out, "W011").len(), 1);
    config(dir.path(), "warning", &["W011"]);
    let out = review(dir.path(), "HEAD~1", true);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(review_hits(&out, "W011")[0]["line"], 3);
    assert_eq!(review(dir.path(), "HEAD~1", false).status.code(), Some(0));
}

#[test]
fn homes_review_error_gate_preserves_duplicate_line_findings() {
    let dir = fixture(true);
    config(dir.path(), "error", &["E007"]);
    write(dir.path(), "src/lib.rs", "fn query() {\n    let _ = \"CURRENT_DATE\";\n    let _ = \"CURRENT_DATE\";\n    let _ = \"CURRENT_DATE\";\n}\n");
    let out = review(dir.path(), "HEAD", true);
    assert_eq!(out.status.code(), Some(1));
    let hits = review_hits(&out, "E007");
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0]["line"], 3);
    assert_eq!(hits[1]["line"], 4);
}

#[test]
fn homes_review_rename_unchanged_symbol_less_source_is_silent() {
    let dir = fixture(true);
    write(
        dir.path(),
        "src/plain.rs",
        "const SQL: &str = \"CURRENT_DATE\";\n",
    );
    git(dir.path(), &["add", "src/plain.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "symbol-less source"]);
    fs::rename(
        dir.path().join("src/plain.rs"),
        dir.path().join("src/moved.rs"),
    )
    .unwrap();
    git(dir.path(), &["add", "src/plain.rs", "src/moved.rs"]);
    let out = review(dir.path(), "HEAD", false);
    assert!(out.status.success());
    assert!(review_hits(&out, "W011").is_empty());
}

#[test]
fn homes_review_rename_out_of_home_or_scope_fires() {
    for from in ["src/time.rs", "outside/lib.rs"] {
        let dir = fixture(true);
        if from.starts_with("outside") {
            write(dir.path(), from, BASE);
            git(dir.path(), &["add", from]);
            git(dir.path(), &["commit", "-q", "-m", "outside scope"]);
        }
        fs::rename(dir.path().join(from), dir.path().join("src/moved.rs")).unwrap();
        git(dir.path(), &["add", from, "src/moved.rs"]);
        let out = review(dir.path(), "HEAD", false);
        assert_eq!(review_hits(&out, "W011").len(), 1);
    }
}

#[test]
fn homes_review_unknown_base_returns_an_error() {
    let dir = fixture(true);
    let out = review(dir.path(), "no-such-ref", false);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot resolve base ref"));
}

#[test]
fn homes_config_malformed_rules_warn_once_each_and_keep_valid_rules() {
    let dir = fixture(true);
    let path = dir.path().join(".keel/keel.json");
    let mut cfg: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    cfg["homes"].as_array_mut().unwrap().extend([
        json!({"name": "bad", "patterns": ["x"], "scope": "["}),
        json!({"name": "civil-day", "patterns": ["x"]}),
    ]);
    fs::write(path, cfg.to_string()).unwrap();
    write(dir.path(), "src/lib.rs", ADDED);
    let out = keel(dir.path(), &["compile", "src/lib.rs", "--json"]);
    assert_eq!(violations_with_code(&parse(&out), "W011").len(), 1);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(stderr.matches("skipping home rule").count(), 2);
    assert!(stderr.contains("bad") && stderr.contains("duplicate name"));
    assert!(!stderr.contains("using defaults"));
}

#[test]
fn homes_compile_llm_names_location_pattern_rule_and_home() {
    let dir = fixture(true);
    write(dir.path(), "src/lib.rs", ADDED);
    let out = keel(dir.path(), &["compile", "src/lib.rs", "--llm"]);
    let text = String::from_utf8(out.stdout).unwrap();
    for expected in [
        "W011",
        "src/lib.rs:3",
        "civil-day",
        "CURRENT_DATE",
        "src/time.rs",
    ] {
        assert!(text.contains(expected), "{text}");
    }
}

#[test]
fn homes_git_missing_blob_is_empty_but_invalid_utf8_or_git_failure_is_unavailable() {
    let dir = fixture(true);
    fs::write(dir.path().join("src/lib.rs"), b"fn query() {}\n\xff\n").unwrap();
    git(dir.path(), &["add", "src/lib.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "non-UTF-8 base"]);
    let commit = keel_enforce::gitdiff::resolve_commit(dir.path(), "HEAD").unwrap();
    assert_eq!(
        keel_enforce::gitdiff::blob_at_checked(dir.path(), &commit, "src/missing.rs").unwrap(),
        None
    );
    assert!(keel_enforce::gitdiff::blob_at_checked(dir.path(), &commit, "src/lib.rs").is_err());
    assert!(
        keel_enforce::gitdiff::blob_at_checked(dir.path(), "not-a-commit", "src/lib.rs").is_err()
    );
    write(dir.path(), "src/lib.rs", ADDED);
    let out = keel(
        dir.path(),
        &["compile", "src/lib.rs", "--json", "--verbose"],
    );
    assert!(violations_with_code(&parse(&out), "W011").is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("base blob is not UTF-8"));
    let out = review(dir.path(), "HEAD", false);
    assert!(out.status.success());
    assert!(review_hits(&out, "W011").is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("base blob is not UTF-8"));
}

#[test]
fn homes_git_pins_base_once_and_does_not_resolve_before_a_match() {
    let dir = fixture(true);
    let cfg = keel_core::config::KeelConfig::load(&dir.path().join(".keel"));
    let file = dir.path().join("src/lib.rs");
    let mut context = keel_enforce::homes_git::GitHomes::new(dir.path(), "HEAD", &cfg, false);
    assert!(context.check(&file, "fn query() {}", None).is_empty());
    let mut lazy = keel_enforce::homes_git::GitHomes::new(dir.path(), "HEAD", &cfg, false);
    assert!(lazy.check(&file, "fn query() {}", None).is_empty());
    // The first match resolves the base, once, at this HEAD.
    assert_eq!(context.check(&file, ADDED, None).len(), 1);
    write(dir.path(), "src/lib.rs", ADDED);
    git(dir.path(), &["add", "src/lib.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "advance HEAD"]);
    assert_eq!(context.check(&file, ADDED, None).len(), 1);
    assert!(lazy.check(&file, ADDED, None).is_empty());
    let mut fresh = keel_enforce::homes_git::GitHomes::new(dir.path(), "HEAD", &cfg, false);
    assert!(fresh.check(&file, ADDED, None).is_empty());
}

#[test]
fn homes_linked_worktree_uses_its_own_paths_and_head() {
    let dir = fixture(true);
    let holder = TempDir::new().unwrap();
    let worktree = holder.path().join("linked");
    git(
        dir.path(),
        &[
            "worktree",
            "add",
            "--detach",
            worktree.to_str().unwrap(),
            "HEAD",
        ],
    );
    write(&worktree, "src/lib.rs", ADDED);
    let out = keel(&worktree.join("src"), &["compile", "./lib.rs", "--json"]);
    let hits = violations_with_code(&parse(&out), "W011");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["file"], "src/lib.rs");
    assert_eq!(
        review_hits(&review(&worktree.join("src"), "HEAD", false), "W011").len(),
        1
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
        BASE
    );
}

#[test]
fn homes_subdirectory_breaker_clears_the_same_repository_scope() {
    let dir = fixture(true);
    config(dir.path(), "error", &[]);
    let subdir = dir.path().join("src");
    for value in 1..=3 {
        write(
            dir.path(),
            "src/lib.rs",
            &format!("fn query() {{ let _ = \"CURRENT_DATE\"; let _ = {value}; }}\n"),
        );
        let out = keel(&subdir, &["compile", "lib.rs", "--json"]);
        assert_eq!(out.status.code(), Some(if value < 3 { 1 } else { 0 }));
    }
    write(dir.path(), "src/lib.rs", "fn query() {}\n");
    keel(&subdir, &["compile", "lib.rs"]);
    write(
        dir.path(),
        "src/lib.rs",
        "fn query() { let _ = \"CURRENT_DATE\"; }\n",
    );
    assert_eq!(
        keel(dir.path(), &["compile", "src/lib.rs", "--json"])
            .status
            .code(),
        Some(1)
    );
}

#[cfg(unix)]
#[test]
fn homes_committed_source_symlink_is_skipped_and_target_checked_once() {
    let dir = fixture(true);
    std::os::unix::fs::symlink("lib.rs", dir.path().join("src/alias.rs")).unwrap();
    git(dir.path(), &["add", "src/alias.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "committed alias"]);
    let args = ["compile", "src/alias.rs", "src/lib.rs", "--json"];
    assert!(violations_with_code(&parse(&keel(dir.path(), &args)), "W011").is_empty());
    assert!(review_hits(&review(dir.path(), "HEAD~1", false), "W011").is_empty());
    write(dir.path(), "src/lib.rs", ADDED);
    let hits = violations_with_code(&parse(&keel(dir.path(), &args)), "W011");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["file"], "src/lib.rs");
    assert_eq!(
        review_hits(&review(dir.path(), "HEAD", false), "W011").len(),
        1
    );
}

#[path = "test_homes_fold.rs"]
mod fold;

#[path = "test_homes_87.rs"]
mod issue87;

#[cfg(unix)]
#[path = "test_homes_base_symlink.rs"]
mod base_symlink;
