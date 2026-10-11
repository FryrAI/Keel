//! Acceptance tests for bringing a mapped graph to HEAD before checking an edit.

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use common::{git, keel_bin};
use tempfile::TempDir;

const ALPHA_ONE: &str =
    "/** Alpha. */\nexport function alpha(value: number): number { return value; }\n";
const ALPHA_TWO: &str = "/** Alpha. */\nexport function alpha(value: number, extra: number): number { return value + extra; }\n";
const CALLER_ONE: &str = "import { alpha } from './lib';\n/** Widget. */\nexport function widget(): number { return alpha(7); }\n";
const CALLER_TWO: &str = "import { alpha } from './lib';\n/** Widget. */\nexport function widget(): number { return alpha(7, 9); }\n";
const REPLACEMENT: &str = "/** Beta. */\nexport function beta(): number { return 83; }\n";
const IDLE: &str = "/** Idle. */\nexport function idle(): number { return 29; }\n";
const REFUSAL: &str = "keel compile: no usable graph — ";

fn keel(dir: &Path, args: &[&str]) -> Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run keel")
}

fn write(dir: &Path, rel: &str, content: &str) {
    let file = dir.join(rel);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, content).unwrap();
}

fn git_output(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git")
}

fn head(dir: &Path) -> String {
    let out = git_output(dir, &["rev-parse", "HEAD"]);
    assert!(out.status.success(), "fixture must have HEAD");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn meta(dir: &Path, key: &str) -> Option<String> {
    let conn = rusqlite::Connection::open_with_flags(
        dir.join(".keel/graph.db"),
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

fn commit(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--no-verify", "-m", message]);
}

fn map(dir: &Path) -> String {
    let out = keel(dir, &["map"]);
    assert!(out.status.success(), "fixture map failed: {out:?}");
    let mapped = head(dir);
    assert_eq!(meta(dir, "last_map_commit"), Some(mapped.clone()));
    mapped
}

fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(root, "src/lib.ts", ALPHA_ONE);
    write(root, "src/caller.ts", CALLER_ONE);
    write(root, "src/idle.ts", IDLE);
    git(root, &["init", "-q", "-b", "base"]);
    let out = keel(root, &["init"]);
    assert!(out.status.success(), "fixture init failed: {out:?}");
    commit(root, "base");
    map(root);
    dir
}

// Two feature commits ensure that a squash cannot accidentally have G's SHA.
fn feature_map(root: &Path) -> String {
    git(root, &["checkout", "-q", "-b", "feature"]);
    write(root, "src/idle.ts", &IDLE.replace("29", "31"));
    commit(root, "feature one");
    write(root, "src/idle.ts", &IDLE.replace("29", "37"));
    commit(root, "feature two");
    map(root)
}

fn checkout_base(root: &Path, mapped: &str) {
    git(root, &["checkout", "-q", "base"]);
    assert_eq!(
        git_output(root, &["merge-base", "--is-ancestor", mapped, "HEAD"])
            .status
            .code(),
        Some(1),
        "fixture requires G not to be an ancestor of H"
    );
}

fn advance_signature(root: &Path) {
    write(root, "src/lib.ts", ALPHA_TWO);
    write(root, "src/caller.ts", CALLER_TWO);
    commit(root, "alpha signature");
}

fn old_arity_edit(root: &Path) {
    write(root, "src/new.ts", "import { alpha } from './lib';\n/** Gadget. */\nexport function gadget(): number { return alpha(11); }\n");
}

fn finding(out: &Output, code: &str, message: &str) {
    assert_eq!(out.status.code(), Some(1), "{message}: {out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains(code),
        "{message}: stdout must contain {code}: {out:?}"
    );
}

fn search(root: &Path, term: &str) -> serde_json::Value {
    let out = keel(root, &["search", term, "--json"]);
    assert!(out.status.success(), "search failed: {out:?}");
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    result["results"].clone()
}

fn refusal(out: &Output, mapped: &str, fix: &str, message: &str) {
    assert_eq!(out.status.code(), Some(2), "{message}: {out:?}");
    assert!(out.stdout.is_empty(), "{message}: stdout must be empty");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.starts_with(REFUSAL),
        "{message}: refusal prefix: {stderr}"
    );
    assert!(
        stderr.contains(&mapped[..12]),
        "{message}: missing G12: {stderr}"
    );
    assert!(
        stderr.contains(fix),
        "{message}: missing fix {fix}: {stderr}"
    );
}

/// Control: a mapped caller must expose a changed callee signature to the editor.
#[test]
fn control_mapped_head_reports_broken_caller() {
    let dir = fixture();
    write(dir.path(), "src/lib.ts", ALPHA_TWO);
    finding(
        &keel(dir.path(), &["compile", "src/lib.ts"]),
        "E001",
        "mapped HEAD must expose the broken caller",
    );
}

/// Control: a mapped caller must expose removal of its callee to the editor.
#[test]
fn control_mapped_head_reports_removed_function() {
    let dir = fixture();
    write(dir.path(), "src/lib.ts", REPLACEMENT);
    finding(
        &keel(dir.path(), &["compile", "src/lib.ts"]),
        "E004",
        "mapped HEAD must expose the removed function",
    );
}

/// Control: a fresh map at the new signature must reject an old-arity new caller.
#[test]
fn control_mapped_head_reports_new_caller_arity() {
    let dir = fixture();
    advance_signature(dir.path());
    map(dir.path());
    old_arity_edit(dir.path());
    finding(
        &keel(dir.path(), &["compile", "src/new.ts"]),
        "E005",
        "mapped HEAD must expose the new caller's old arity",
    );
}

/// A branch checkout must still tell the editor that its edit breaks a base caller.
#[test]
fn base_branch_checkout_checks_the_edit() {
    let dir = fixture();
    let root = dir.path();
    let mapped = feature_map(root);
    checkout_base(root, &mapped);
    write(root, "src/lib.ts", ALPHA_TWO);
    finding(
        &keel(root, &["compile", "src/lib.ts"]),
        "E001",
        "base checkout must check the broken caller",
    );
}

/// Callers that exist only on the other branch must never become phantom findings.
#[test]
fn base_checkout_has_no_feature_only_phantom_callers() {
    let dir = fixture();
    let root = dir.path();
    let gadget = "/** Gadget. */\nexport function gadget(): number { return 43; }\n";
    write(root, "src/lib.ts", &format!("{ALPHA_ONE}{gadget}"));
    commit(root, "gadget");
    git(root, &["checkout", "-q", "-b", "feature"]);
    write(root, "src/feature_only.ts", "import { gadget } from './lib';\n/** Feature widget. */\nexport function feature_widget(): number { return gadget(); }\n");
    commit(root, "feature caller");
    let mapped = map(root);
    checkout_base(root, &mapped);
    write(root, "src/lib.ts", ALPHA_ONE);
    let out = keel(root, &["compile", "src/lib.ts"]);
    assert!(
        matches!(out.status.code(), Some(0 | 1)),
        "base checkout must check without refusal: {out:?}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        assert!(
            !line.contains("feature_only.ts"),
            "feature-only caller must not appear: {stdout}"
        );
        assert!(
            !(line.contains("gadget") && (line.contains("E004") || line.contains("E001"))),
            "gadget must have no phantom finding: {stdout}"
        );
    }
    assert!(
        !stdout.contains("E004") && !stdout.contains("E001"),
        "removing an uncalled gadget must have no removal or caller errors: {stdout}"
    );
    assert!(!String::from_utf8_lossy(&out.stderr).contains("feature_only.ts"));
}

/// A squash merge must leave the hook able to report a real broken caller.
#[test]
fn squash_merge_checks_the_edit() {
    let dir = fixture();
    let root = dir.path();
    let mapped = feature_map(root);
    checkout_base(root, &mapped);
    git(root, &["merge", "--squash", "feature"]);
    commit(root, "squash feature");
    assert_eq!(
        git_output(root, &["merge-base", "--is-ancestor", &mapped, "HEAD"])
            .status
            .code(),
        Some(1)
    );
    write(root, "src/lib.ts", ALPHA_TWO);
    finding(
        &keel(root, &["compile", "src/lib.ts"]),
        "E001",
        "squash merge must check the broken caller",
    );
}

/// A pull must update uncompiled signatures before checking a new editor-created caller.
#[test]
fn ancestor_move_checks_new_caller_against_head_signature() {
    let dir = fixture();
    let root = dir.path();
    advance_signature(root);
    old_arity_edit(root);
    finding(
        &keel(root, &["compile", "src/new.ts"]),
        "E005",
        "ancestor move must reject the old argument count",
    );
}

/// Search must stop offering deleted definitions and must show renamed paths after an edit.
#[test]
fn moved_head_prunes_deleted_files_and_indexes_renames() {
    let dir = fixture();
    let root = dir.path();
    write(
        root,
        "src/gone.ts",
        "/** Gone. */\nexport function gone_fn(): number { return 47; }\n",
    );
    write(
        root,
        "src/old.ts",
        "/** Moved. */\nexport function moved_fn(): number { return 53; }\n",
    );
    commit(root, "definitions");
    map(root);
    git(root, &["rm", "-q", "src/gone.ts"]);
    git(root, &["mv", "src/old.ts", "src/new_path.ts"]);
    commit(root, "move definitions");
    write(root, "src/idle.ts", &IDLE.replace("29", "59"));
    let out = keel(root, &["compile", "src/idle.ts"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "unrelated edit must compile: {out:?}"
    );
    assert_eq!(
        search(root, "gone_fn"),
        serde_json::json!([]),
        "deleted definition must disappear from search"
    );
    let moved = search(root, "moved_fn");
    assert!(
        moved
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["file"] == "src/new_path.ts"),
        "renamed definition must use its new path: {moved}"
    );
    assert!(
        !moved.to_string().contains("src/old.ts"),
        "old path must disappear: {moved}"
    );
}

/// The marker must follow the graph content on every move, including files never compiled.
#[test]
fn stored_commit_and_uncompiled_content_follow_two_head_moves() {
    let dir = fixture();
    let root = dir.path();
    let map_time = meta(root, "last_map_at");
    for (file, name, value) in [("src/beta.ts", "beta", 61), ("src/gamma.ts", "gamma", 67)] {
        write(
            root,
            file,
            &format!(
                "/** Definition. */\nexport function {name}(): number {{ return {value}; }}\n"
            ),
        );
        commit(root, "add definition");
        let current = head(root);
        write(root, "src/idle.ts", &IDLE.replace("29", &value.to_string()));
        let out = keel(root, &["compile", "src/idle.ts"]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "unrelated edit must compile: {out:?}"
        );
        assert_eq!(
            meta(root, "last_map_commit"),
            Some(current),
            "stored commit must follow the completed content move"
        );
        assert!(
            search(root, name)
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["file"] == file),
            "uncompiled added definition must be searchable"
        );
        assert_eq!(
            meta(root, "last_map_at"),
            map_time,
            "resync must preserve the full-map timestamp"
        );
        git(root, &["restore", "src/idle.ts"]);
    }
}

/// Folding the edited file into the baseline would hide removal of a live HEAD callee.
#[test]
fn nonancestor_move_uses_head_blob_before_checking_removal() {
    let dir = fixture();
    let root = dir.path();
    git(root, &["checkout", "-q", "-b", "feature"]);
    write(root, "src/lib.ts", REPLACEMENT);
    write(
        root,
        "src/caller.ts",
        "/** Widget. */\nexport function widget(): number { return 71; }\n",
    );
    commit(root, "replace alpha");
    let mapped = map(root);
    checkout_base(root, &mapped);
    write(root, "src/lib.ts", REPLACEMENT);
    finding(
        &keel(root, &["compile", "src/lib.ts"]),
        "E004",
        "HEAD baseline must expose removal before syncing the edit",
    );
}

/// A clean hook invocation after a checkout must produce no distracting output.
#[test]
fn nonancestor_move_clean_compile_stays_silent() {
    let dir = fixture();
    let root = dir.path();
    let mapped = feature_map(root);
    checkout_base(root, &mapped);
    let out = keel(root, &["compile", "src/idle.ts"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "clean moved compile must succeed: {out:?}"
    );
    assert!(
        out.stdout.is_empty(),
        "clean moved compile stdout must be empty: {out:?}"
    );
    assert!(
        out.stderr.is_empty(),
        "clean moved compile stderr must be empty: {out:?}"
    );
}

/// An ignored changed file must not become a searchable suggestion after catching up.
#[test]
fn head_move_keeps_ignored_files_out_of_search() {
    let dir = fixture();
    let root = dir.path();
    write(root, ".keelignore", "ignored/\n");
    write(
        root,
        "ignored/gadget.ts",
        "/** Gadget. */\nexport function ignored_gadget(): number { return 73; }\n",
    );
    commit(root, "ignore gadget");
    map(root);
    write(
        root,
        "ignored/gadget.ts",
        "/** Widget. */\nexport function ignored_widget(): number { return 79; }\n",
    );
    commit(root, "ignored change");
    let out = keel(root, &["compile", "src/idle.ts"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "ignored move must compile: {out:?}"
    );
    assert_eq!(
        search(root, "ignored_widget"),
        serde_json::json!([]),
        "ignored changed definition must stay out of search"
    );
}

/// A pruned stored commit must give the hook a loud refusal and a rebuild command.
#[test]
fn missing_stored_commit_refuses_without_moving_marker() {
    let dir = fixture();
    let root = dir.path();
    let mapped = feature_map(root);
    checkout_base(root, &mapped);
    git(root, &["branch", "-D", "feature"]);
    git(root, &["reflog", "expire", "--expire=now", "--all"]);
    git(root, &["gc", "--prune=now"]);
    assert!(
        !git_output(root, &["cat-file", "-e", &format!("{mapped}^{{commit}}")])
            .status
            .success(),
        "fixture must prune the stored commit"
    );
    let out = keel(root, &["compile", "src/idle.ts"]);
    refusal(
        &out,
        &mapped,
        "keel map",
        "missing stored commit must refuse",
    );
    assert_eq!(
        meta(root, "last_map_commit"),
        Some(mapped),
        "refusal must preserve the marker"
    );
}

/// A large pull must direct the hook to a bare catch-up, which then enables real checks.
#[test]
fn per_edit_limit_refuses_then_bare_compile_catches_up() {
    let dir = fixture();
    let root = dir.path();
    let mapped = head(root);
    for index in 0..40 {
        write(
            root,
            &format!("src/added_{index}.ts"),
            &format!(
                "/** Added. */ export function added_{index}(): number {{ return {}; }}\n",
                index + 100
            ),
        );
    }
    commit(root, "many definitions");
    let current = head(root);
    let out = keel(root, &["compile", "src/idle.ts"]);
    refusal(&out, &mapped, "keel compile", "per-edit limit must refuse");
    assert_eq!(
        meta(root, "last_map_commit"),
        Some(mapped),
        "per-edit refusal must preserve the marker"
    );
    let out = keel(root, &["compile"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "bare catch-up must succeed: {out:?}"
    );
    assert_eq!(
        meta(root, "last_map_commit"),
        Some(current),
        "bare catch-up must move the marker"
    );
    write(root, "src/lib.ts", REPLACEMENT);
    finding(
        &keel(root, &["compile", "src/lib.ts"]),
        "E004",
        "after catch-up the hook must expose the removal",
    );
}

/// A move above the hard cap must refuse rather than silently perform an unbounded catch-up.
#[test]
fn hard_limit_refuses_bare_compile_without_moving_marker() {
    let dir = fixture();
    let root = dir.path();
    let mapped = head(root);
    for index in 0..520 {
        write(
            root,
            &format!("src/added_{index}.ts"),
            &format!(
                "/** Added. */ export function added_{index}(): number {{ return {}; }}\n",
                index + 100
            ),
        );
    }
    commit(root, "many definitions");
    let out = keel(root, &["compile"]);
    refusal(&out, &mapped, "keel map", "hard limit must refuse");
    assert_eq!(
        meta(root, "last_map_commit"),
        Some(mapped),
        "hard-limit refusal must preserve the marker"
    );
}

/// Control: committed-range checking must retain the existing nonancestor refusal.
#[test]
fn control_since_nonancestor_keeps_existing_refusal() {
    let dir = fixture();
    let root = dir.path();
    let base = head(root);
    let mapped = feature_map(root);
    checkout_base(root, &mapped);
    let out = keel(root, &["compile", "--since", &base]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "since must retain nonancestor refusal: {out:?}"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not an ancestor of HEAD"),
        "since must retain the ancestry diagnostic: {out:?}"
    );
    assert_eq!(meta(root, "last_map_commit"), Some(mapped));
}

/// Control: committed-range checking must not advance its own stored comparison baseline.
#[test]
fn control_since_ancestor_leaves_stored_commit_unchanged() {
    let dir = fixture();
    let root = dir.path();
    let mapped = head(root);
    write(root, "src/idle.ts", &IDLE.replace("29", "89"));
    commit(root, "idle change");
    let out = keel(root, &["compile", "--since", &mapped]);
    assert!(
        matches!(out.status.code(), Some(0 | 1)),
        "since ancestor must be checked: {out:?}"
    );
    assert_eq!(
        meta(root, "last_map_commit"),
        Some(mapped),
        "since must leave the marker unchanged"
    );
}
