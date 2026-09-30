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
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
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

#[test]
fn checkpoint_git_fixture_ignores_inherited_config_and_repository_overrides() {
    const CHILD: &str = "KEEL_CHECKPOINT_GIT_FIXTURE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let dir = tempfile::tempdir().unwrap();
        let edited = "fn hello() -> i32 { 2 }\n";
        std::fs::write(dir.path().join("hello.rs"), edited).unwrap();
        checkpoint_git_fixture(dir.path());
        assert!(dir.path().join(".git/HEAD").is_file());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("hello.rs")).unwrap(),
            edited
        );
        return;
    }
    // Use a child test process so hostile Git environment does not affect
    // other tests executing concurrently in this process.
    let dir = tempfile::tempdir().unwrap();
    let excludes = dir.path().join("excludes");
    std::fs::write(&excludes, "hello.rs\n").unwrap();
    let config = dir.path().join("gitconfig");
    std::fs::write(
        &config,
        format!("[core]\nexcludesFile = {}\n", excludes.display()),
    )
    .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "writer_test_support::checkpoint_git_fixture_ignores_inherited_config_and_repository_overrides", "--nocapture"])
        .env(CHILD, "1")
        .env("GIT_CONFIG_GLOBAL", config)
        .env("GIT_DIR", dir.path().join("absent.git"))
        .env("GIT_WORK_TREE", dir.path().join("absent-worktree"))
        .env("GIT_INDEX_FILE", dir.path().join("absent/index"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"),
        "child selector must run its test"
    );
}
