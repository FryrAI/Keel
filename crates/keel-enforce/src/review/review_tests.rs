//! End-to-end tests for `review()` against real temporary git repositories.
//!
//! These exercise the part that unit tests cannot: `git show <base>:<path>`
//! feeding `LanguageResolver::parse_file` with no checkout, and rename
//! detection surviving `--name-status -M`.

use std::path::Path;
use std::process::Command;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};
use tempfile::TempDir;

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
        .expect("git failed to run");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

/// A git repo with one commit, ready to be modified in the working tree.
fn repo(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q"]);
    for (rel, content) in files {
        write(dir.path(), rel, content);
    }
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "base"]);
    dir
}

fn node(id: u64, name: &str, file: &str) -> GraphNode {
    GraphNode {
        id,
        hash: format!("hash{:07}", id),
        kind: NodeKind::Function,
        name: name.into(),
        signature: format!("fn {name}()"),
        file_path: file.into(),
        line_start: 1,
        line_end: 3,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

/// A store where `caller` (in `caller_file`) calls `callee` (in `callee_file`).
fn store_with_caller(
    callee: &str,
    callee_file: &str,
    caller: &str,
    caller_file: &str,
) -> SqliteGraphStore {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store.insert_node(&node(1, callee, callee_file)).unwrap();
    store.insert_node(&node(2, caller, caller_file)).unwrap();
    store
        .update_edges(vec![EdgeChange::Add(GraphEdge {
            id: 1,
            source_id: 2,
            target_id: 1,
            kind: EdgeKind::Calls,
            file_path: caller_file.into(),
            line: 2,
            confidence: 1.0,
        })])
        .unwrap();
    store
}

fn empty_store() -> SqliteGraphStore {
    SqliteGraphStore::in_memory().unwrap()
}

/// The same check toggles `keel compile` runs with.
fn enforce() -> keel_core::config::EnforceConfig {
    keel_core::config::EnforceConfig::default()
}

#[test]
fn signature_change_leads_with_its_callers_outside_the_diff() {
    let dir = repo(&[
        (
            "src/commands.rs",
            "pub fn execute(cmd: u8) -> u8 {\n    cmd\n}\n",
        ),
        (
            "src/main.rs",
            "fn main() {\n    let _ = crate::commands::execute(1);\n}\n",
        ),
    ]);
    // Only src/commands.rs is touched — src/main.rs stays outside the diff.
    write(
        dir.path(),
        "src/commands.rs",
        "pub fn execute(cmd: u8, dry_run: bool) -> u8 {\n    if dry_run { 0 } else { cmd }\n}\n",
    );

    let store = store_with_caller("execute", "src/commands.rs", "main", "src/main.rs");
    let result = review(&store, dir.path(), "HEAD", &enforce()).unwrap();

    assert_eq!(result.contract_change_count, 1);
    let top = &result.changes[0];
    assert_eq!(top.name, "execute");
    assert_eq!(top.kind, ChangeKind::SignatureChanged);
    assert_eq!(top.callers_outside_diff_count, 1);
    assert_eq!(top.callers_outside_diff[0].name, "main");
    assert!(!top.sig_base.as_deref().unwrap().contains("dry_run"));
    assert!(top.sig_head.as_deref().unwrap().contains("dry_run"));
    assert_ne!(top.hash_base, top.hash_head);
    assert_eq!(result.resolution, "tier1");

    let headline = render::headline(&result).expect("a contract change has a headline");
    assert!(headline.starts_with("execute()"), "got: {headline}");
    assert!(headline.contains("src/commands.rs"));
    assert!(headline.contains("1 caller(s) outside the diff"));
}

#[test]
fn callers_inside_the_diff_do_not_count() {
    let dir = repo(&[
        (
            "src/commands.rs",
            "pub fn execute(cmd: u8) -> u8 {\n    cmd\n}\n",
        ),
        (
            "src/main.rs",
            "fn main() {\n    let _ = crate::commands::execute(1);\n}\n",
        ),
    ]);
    // This PR updates the call site too.
    write(
        dir.path(),
        "src/commands.rs",
        "pub fn execute(cmd: u8, dry_run: bool) -> u8 {\n    if dry_run { 0 } else { cmd }\n}\n",
    );
    write(
        dir.path(),
        "src/main.rs",
        "fn main() {\n    let _ = crate::commands::execute(1, false);\n}\n",
    );

    let store = store_with_caller("execute", "src/commands.rs", "main", "src/main.rs");
    let result = review(&store, dir.path(), "HEAD", &enforce()).unwrap();

    let exec = result.changes.iter().find(|c| c.name == "execute").unwrap();
    assert_eq!(exec.callers_outside_diff_count, 0);
    assert!(exec.callers_outside_diff.is_empty());
}

#[test]
fn a_body_only_pr_is_silent() {
    let dir = repo(&[(
        "src/lib.rs",
        "pub fn add(a: u8, b: u8) -> u8 {\n    a + b\n}\n",
    )]);
    write(
        dir.path(),
        "src/lib.rs",
        "pub fn add(a: u8, b: u8) -> u8 {\n    let sum = a + b;\n    sum\n}\n",
    );

    let result = review(&empty_store(), dir.path(), "HEAD", &enforce()).unwrap();
    assert_eq!(result.contract_change_count, 0);
    assert_eq!(result.body_only_count, 1);
    assert_eq!(result.functions_touched, 1);
    assert!(render::is_silent(&result));
}

#[test]
fn a_docstring_only_change_is_doc_only_not_body_only() {
    let dir = repo(&[(
        "src/lib.rs",
        "/// Old.\npub fn add(a: u8, b: u8) -> u8 {\n    a + b\n}\n",
    )]);
    write(
        dir.path(),
        "src/lib.rs",
        "/// New wording entirely.\npub fn add(a: u8, b: u8) -> u8 {\n    a + b\n}\n",
    );

    let result = review(&empty_store(), dir.path(), "HEAD", &enforce()).unwrap();
    assert_eq!(result.doc_only_count, 1);
    assert_eq!(result.body_only_count, 0);
    assert!(render::is_silent(&result));
}

#[test]
fn a_pure_rename_reports_moved_not_add_plus_remove() {
    let dir = repo(&[(
        "src/old_name.rs",
        "pub fn render(x: u8) -> u8 {\n    x\n}\n",
    )]);
    std::fs::rename(
        dir.path().join("src/old_name.rs"),
        dir.path().join("src/new_name.rs"),
    )
    .unwrap();
    // -M rename detection needs the deletion staged alongside the addition.
    git(dir.path(), &["add", "-A"]);

    let result = review(&empty_store(), dir.path(), "HEAD", &enforce()).unwrap();
    assert_eq!(result.changes.len(), 1, "{:?}", result.changes);
    let moved = &result.changes[0];
    assert_eq!(moved.name, "render");
    assert_eq!(
        moved.kind,
        ChangeKind::Moved {
            from: "src/old_name.rs".into()
        }
    );
    assert_eq!(moved.file, "src/new_name.rs");
    assert_eq!(moved.hash_base, moved.hash_head);
}

#[test]
fn unparsed_structural_files_are_named_not_omitted() {
    let dir = repo(&[("src/lib.rs", "pub fn a() -> u8 {\n    1\n}\n")]);
    write(
        dir.path(),
        "migrations/001_init.sql",
        "CREATE TABLE t(x);\n",
    );
    write(
        dir.path(),
        "baml_src/main.baml",
        "function F() -> string {}\n",
    );
    write(dir.path(), "README.md", "# docs\n");
    git(dir.path(), &["add", "-A"]);

    let result = review(&empty_store(), dir.path(), "HEAD", &enforce()).unwrap();
    let paths: Vec<&str> = result.unanalyzed.iter().map(|u| u.path.as_str()).collect();
    assert_eq!(paths, vec!["baml_src/main.baml", "migrations/001_init.sql"]);
    assert_eq!(result.unanalyzed[0].class, "boundary");
    assert_eq!(result.unanalyzed[1].class, "data");
    // Unanalyzed files alone are enough to break the clean-output silence.
    assert!(!render::is_silent(&result));
}

#[test]
fn a_deleted_file_reports_its_symbols_as_removed() {
    let dir = repo(&[
        ("src/lib.rs", "pub fn gone(x: u8) -> u8 {\n    x\n}\n"),
        ("src/main.rs", "fn main() {\n    let _ = gone(1);\n}\n"),
    ]);
    std::fs::remove_file(dir.path().join("src/lib.rs")).unwrap();

    let store = store_with_caller("gone", "src/lib.rs", "main", "src/main.rs");
    let result = review(&store, dir.path(), "HEAD", &enforce()).unwrap();

    let removed = result.changes.iter().find(|c| c.name == "gone").unwrap();
    assert_eq!(removed.kind, ChangeKind::Removed);
    assert!(removed.sig_head.is_none());
    assert_eq!(removed.callers_outside_diff_count, 1);
}

#[test]
fn a_whitespace_only_reformat_introduces_no_new_violations() {
    // Two public, undocumented functions — E003 on both sides. The reformat
    // reindents them and pushes them 4 lines down; nothing was introduced.
    let dir = repo(&[(
        "src/lib.rs",
        "pub fn one(x: u8) -> u8 {\n    x\n}\n\npub fn two(y: u8) -> u8 {\n    y\n}\n",
    )]);
    write(
        dir.path(),
        "src/lib.rs",
        "\n\n\n\npub fn one(x: u8) -> u8 {\n\n    x\n\n}\n\npub fn two(y: u8) -> u8 {\n\n    y\n\n}\n",
    );

    let result = review(&empty_store(), dir.path(), "HEAD", &enforce()).unwrap();
    assert!(
        result.new_violations.is_empty(),
        "reformat produced: {:?}",
        result
            .new_violations
            .iter()
            .map(|v| format!("{} {}", v.code, v.message))
            .collect::<Vec<_>>()
    );
    assert!(result.pre_existing_violations >= 2);
}

#[test]
fn a_newly_added_undocumented_function_is_a_new_violation() {
    let dir = repo(&[(
        "src/lib.rs",
        "/// Documented.\npub fn one(x: u8) -> u8 {\n    x\n}\n",
    )]);
    write(
        dir.path(),
        "src/lib.rs",
        "/// Documented.\npub fn one(x: u8) -> u8 {\n    x\n}\n\npub fn two(y: u8) -> u8 {\n    y\n}\n",
    );

    let result = review(&empty_store(), dir.path(), "HEAD", &enforce()).unwrap();
    assert_eq!(
        result.new_violations.len(),
        1,
        "{:?}",
        result.new_violations
    );
    assert_eq!(result.new_violations[0].code, "E003");
    assert!(result.new_violations[0].message.contains("two"));
    assert!(
        !render::is_silent(&result),
        "a new violation breaks silence"
    );
}

#[test]
fn an_unresolvable_base_is_an_error_not_an_empty_review() {
    let dir = repo(&[("src/lib.rs", "pub fn a() -> u8 {\n    1\n}\n")]);
    let err = review(&empty_store(), dir.path(), "no-such-ref", &enforce()).unwrap_err();
    assert!(err.contains("no-such-ref"), "got: {err}");
}

#[test]
fn json_round_trips_including_the_moved_payload() {
    let dir = repo(&[("src/old.rs", "pub fn r(x: u8) -> u8 {\n    x\n}\n")]);
    std::fs::rename(dir.path().join("src/old.rs"), dir.path().join("src/new.rs")).unwrap();
    git(dir.path(), &["add", "-A"]);

    let result = review(&empty_store(), dir.path(), "HEAD", &enforce()).unwrap();
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"kind\":\"moved\""), "{json}");
    assert!(json.contains("\"from\":\"src/old.rs\""), "{json}");
    let back: ReviewResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back.changes[0].kind, result.changes[0].kind);
}
