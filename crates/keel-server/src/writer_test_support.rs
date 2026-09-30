//! Disk-row snapshots and an isolated Git fixture for writer regressions.

use rusqlite::{Connection, OpenFlags};
use std::path::Path;

/// Snapshot primary-key row contents in every graph and persistent state table.
pub(super) fn state(root: &Path) -> Vec<(String, Vec<Vec<String>>)> {
    let db = Connection::open_with_flags(
        root.join(".keel/graph.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut names = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let tables: Vec<String> = names
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    tables
        .into_iter()
        .map(|table| {
            let mut statement = db
                .prepare(&format!("SELECT * FROM \"{table}\" ORDER BY rowid"))
                .unwrap();
            let columns = statement.column_count();
            let rows = statement
                .query_map([], |row| {
                    Ok((0..columns)
                        .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                        .collect())
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            (table, rows)
        })
        .collect()
}

/// Commit the original hello.rs in an isolated Git repository, then restore its edit.
pub(super) fn checkpoint_git_fixture(root: &Path) {
    let source = root.join("hello.rs");
    let edited = std::fs::read(&source).unwrap();
    std::fs::write(&source, "fn hello() -> i32 { 1 }\n").unwrap();
    for args in [
        &["init"][..],
        &["add", "hello.rs"][..],
        &["commit", "-m", "checkpoint baseline"][..],
    ] {
        let output = std::process::Command::new("git")
            .args([
                "-c",
                "user.email=test@keel.dev",
                "-c",
                "user.name=keel test",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .current_dir(root)
            .output()
            .expect("git command failed to run");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::write(source, edited).unwrap();
}
