//! Every CLI writer refuses a held disk graph before reading its baseline.

use std::path::Path;
use std::process::{Command, Output};

use keel_core::graph_lock;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use rusqlite::Connection;
use tempfile::TempDir;

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(crate::common::keel_bin())
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

fn fixture() -> TempDir {
    let (dir, root) = crate::common::setup_test_project("rust");
    for args in [&["init", "--yes"][..], &["map"][..]] {
        let out = run(&root, args);
        assert!(
            out.status.success(),
            "{:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    std::fs::write(
        root.join("plan.md"),
        "1. Call `missing_helper()` from `src/lib.rs`.\n",
    )
    .unwrap();
    std::fs::write(
        root.join("history.jsonl"),
        serde_json::json!({
            "id": 1, "commit_sha": "imported", "captured_at": "2026-09-01",
            "metrics": "{\"metrics_version\":1}"
        })
        .to_string(),
    )
    .unwrap();
    dir
}

fn state(root: &Path) -> Vec<Vec<String>> {
    let db = Connection::open_with_flags(
        root.join(".keel/graph.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    // Compare every graph/state table. Read-only SQLite connections cannot
    // initialize schema and accidentally mask an early writer-store opening.
    let mut names = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let tables: Vec<String> = names
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut result = vec![];
    for table in tables {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{table}\" ORDER BY rowid"))
            .unwrap();
        let columns = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                Ok((0..columns)
                    .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                    .collect::<Vec<_>>())
            })
            .unwrap();
        result.extend(rows.map(Result::unwrap));
    }
    result
}

fn refuses_then_runs(args: &[&str], expected: i32) {
    let dir = fixture();
    let root = dir.path();
    let before = state(root);
    let held = graph_lock::try_acquire(&root.join(".keel")).unwrap();
    let out = run(root, args);
    assert_eq!(
        out.status.code(),
        Some(expected),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains(&format!("keel {}:", args[0])));
    assert!(String::from_utf8_lossy(&out.stderr).contains("graph lock"));
    assert!(out.stdout.is_empty());
    assert_eq!(state(root), before, "blocked {:?} mutated rows", args);
    drop(held);
    let out = run(root, args);
    assert_ne!(
        out.status.code(),
        Some(2),
        "released {:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!String::from_utf8_lossy(&out.stderr).contains("graph lock"));
}

#[test]
fn cli_compile_busy_skips_then_updates_after_release() {
    let dir = fixture();
    let root = dir.path();
    let store = SqliteGraphStore::open(root.join(".keel/graph.db").to_str().unwrap()).unwrap();
    let before = store
        .get_nodes_in_file("src/lib.rs")
        .into_iter()
        .find(|n| n.name == "hello")
        .unwrap()
        .hash;
    std::fs::write(
        root.join("src/lib.rs"),
        "/// Greet a name.\npub fn hello(name: &str) -> String { format!(\"Hello {name}\") }\n",
    )
    .unwrap();
    let held = graph_lock::try_acquire(&root.join(".keel")).unwrap();
    let out = run(root, &["compile", "src/lib.rs"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr)
        .contains("another keel process holds the graph lock, skipping"));
    assert!(out.stdout.is_empty());
    assert!(store.get_node(&before).is_some());
    drop(held);
    let out = run(root, &["compile", "src/lib.rs"]);
    assert_ne!(out.status.code(), Some(2));
    assert_ne!(store.get_node(&before).unwrap().hash, before);
}

#[test]
fn cli_map_busy_refuses_then_runs() {
    refuses_then_runs(&["map"], 2);
}
#[test]
fn cli_fix_busy_refuses_then_runs() {
    refuses_then_runs(&["fix", "--file", "src/lib.rs"], 2);
}
#[test]
fn cli_fix_apply_busy_refuses_then_runs() {
    refuses_then_runs(&["fix", "--apply", "--file", "src/lib.rs"], 2);
}
#[test]
fn cli_checkpoint_busy_refuses_then_runs() {
    refuses_then_runs(&["checkpoint"], 2);
}
#[test]
fn cli_validate_plan_busy_refuses_then_runs() {
    refuses_then_runs(&["validate-plan", "plan.md"], 2);
}
#[test]
fn cli_quality_snapshot_busy_refuses_then_runs() {
    refuses_then_runs(&["quality", "--snapshot"], 2);
}
#[test]
fn cli_quality_import_busy_refuses_then_runs() {
    refuses_then_runs(&["quality", "--import", "history.jsonl"], 2);
}
#[test]
fn cli_init_merge_busy_refuses_then_runs() {
    refuses_then_runs(&["init", "--merge", "--yes"], 2);
}
#[test]
fn cli_deinit_busy_refuses_then_runs() {
    let dir = fixture();
    let held = graph_lock::try_acquire(&dir.path().join(".keel")).unwrap();
    let before = state(dir.path());
    let out = run(dir.path(), &["deinit"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(state(dir.path()), before);
    drop(held);
    assert!(run(dir.path(), &["deinit"]).status.success());
    assert!(!dir.path().join(".keel").exists());
}

#[test]
fn cli_lock_io_is_error_for_every_writer() {
    let dir = fixture();
    let before = state(dir.path());
    std::fs::remove_file(dir.path().join(".keel/compile.lock")).unwrap();
    std::fs::create_dir(dir.path().join(".keel/compile.lock")).unwrap();
    for args in [
        vec!["compile", "src/lib.rs"],
        vec!["map"],
        vec!["fix"],
        vec!["checkpoint"],
        vec!["validate-plan", "plan.md"],
        vec!["quality", "--snapshot"],
        vec!["quality", "--import", "history.jsonl"],
        vec!["init", "--merge", "--yes"],
    ] {
        let out = run(dir.path(), &args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("graph lock I/O error"),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
        assert_eq!(state(dir.path()), before);
    }
}

#[test]
fn cli_map_locks_before_opening_and_migrating_store() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".keel")).unwrap();
    let held = graph_lock::try_acquire(&dir.path().join(".keel")).unwrap();
    let out = run(dir.path(), &["map"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!dir.path().join(".keel/graph.db").exists());
    drop(held);
    assert!(run(dir.path(), &["map"]).status.success());
    assert!(dir.path().join(".keel/graph.db").exists());
}

#[test]
fn cli_deinit_lock_io_reports_error_and_proceeds_with_cleanup() {
    let dir = fixture();
    std::fs::remove_file(dir.path().join(".keel/compile.lock")).unwrap();
    std::fs::create_dir(dir.path().join(".keel/compile.lock")).unwrap();
    let out = run(dir.path(), &["deinit"]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("graph lock I/O error"), "{stderr}");
    assert!(stderr.contains("proceeding with cleanup"), "{stderr}");
    assert!(!dir.path().join(".keel").exists());
}

#[test]
fn cli_uninitialized_writers_keep_init_hint_before_lock_access() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("plan.md"), "1. Call `hello()`.\n").unwrap();
    for args in [
        vec!["compile", "hello.rs"],
        vec!["map"],
        vec!["fix"],
        vec!["checkpoint"],
        vec!["validate-plan", "plan.md"],
        vec!["quality", "--snapshot"],
        vec!["quality", "--import", "missing.jsonl"],
        vec!["serve", "--mcp"],
        vec!["serve", "--http"],
        vec!["watch"],
    ] {
        let out = run(dir.path(), &args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("Run `keel init` first"),
            "{args:?}: {stderr}"
        );
        assert!(
            !stderr.contains("graph lock I/O error"),
            "{args:?}: {stderr}"
        );
        assert!(!dir.path().join(".keel").exists());
    }
}
