use super::*;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Run a git command in `cwd`, asserting it succeeds.
fn git(args: &[&str], cwd: &Path) {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to spawn git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr),
    );
}

fn init_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    git(&["init"], dir.path());
    git(&["config", "user.email", "test@test.com"], dir.path());
    git(&["config", "user.name", "Test"], dir.path());
    dir
}

/// Initial commit (no `HEAD` yet): a `Since(None)` diff must fall back to the
/// staged index so the very first commit's files are still reported. Before the
/// shared fallback existed, `git diff --name-only HEAD` failed and this returned
/// empty — the bug this test guards.
#[test]
fn initial_commit_falls_back_to_staged() {
    let dir = init_repo();
    std::fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    git(&["add", "a.rs"], dir.path());
    // Deliberately NOT committed: there is no HEAD.

    let files = changed_files(dir.path(), &DiffMode::Since(None), true);
    assert_eq!(files, vec!["a.rs".to_string()]);
}

/// The supported-language filter drops paths keel cannot parse.
#[test]
fn supported_filter_drops_unparseable_files() {
    let dir = init_repo();
    std::fs::write(dir.path().join("keep.rs"), "fn a() {}\n").unwrap();
    std::fs::write(dir.path().join("notes.txt"), "hello\n").unwrap();
    git(&["add", "."], dir.path());

    let filtered = changed_files(dir.path(), &DiffMode::Since(None), true);
    assert_eq!(filtered, vec!["keep.rs".to_string()]);

    let unfiltered = changed_files(dir.path(), &DiffMode::Since(None), false);
    assert!(unfiltered.contains(&"keep.rs".to_string()));
    assert!(unfiltered.contains(&"notes.txt".to_string()));
}

/// Working-tree edits to a committed file show up under `Since(None)`.
#[test]
fn since_head_reports_working_tree_edits() {
    let dir = init_repo();
    std::fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    git(&["add", "."], dir.path());
    git(&["commit", "-m", "init"], dir.path());
    std::fs::write(dir.path().join("a.rs"), "fn a() { let _ = 1; }\n").unwrap();

    let files = changed_files(dir.path(), &DiffMode::Since(None), true);
    assert_eq!(files, vec!["a.rs".to_string()]);
}

/// A `--since` base git cannot resolve is a user error (a typo, a branch that
/// was never fetched), NOT an initial-commit repo. Falling back to the staged
/// diff there made `keel compile --since <typo>` in CI compile zero files and
/// exit 0 — a green build that checked nothing. It must surface as an error.
#[test]
fn range_with_unresolvable_base_is_an_error() {
    let dir = init_repo();
    std::fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    git(&["add", "a.rs"], dir.path());
    git(&["commit", "-m", "init"], dir.path());
    // Something staged, so the old --cached fallback would have returned files.
    std::fs::write(dir.path().join("b.rs"), "fn b() {}\n").unwrap();
    git(&["add", "b.rs"], dir.path());

    let err = changed_files_checked(
        dir.path(),
        &DiffMode::Range("no-such-ref-xyz".to_string()),
        true,
    )
    .expect_err("an unresolvable --since base must not silently diff something else");
    assert!(
        err.contains("no-such-ref-xyz"),
        "the error must name the base that failed, got: {err}"
    );
}

/// A `Range` base that DOES resolve still works normally.
#[test]
fn range_with_resolvable_base_lists_committed_changes() {
    let dir = init_repo();
    std::fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    git(&["add", "a.rs"], dir.path());
    git(&["commit", "-m", "init"], dir.path());
    let base = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    std::fs::write(dir.path().join("b.rs"), "fn b() {}\n").unwrap();
    git(&["add", "b.rs"], dir.path());
    git(&["commit", "-m", "second"], dir.path());

    let files = changed_files_checked(dir.path(), &DiffMode::Range(base), true).unwrap();
    assert_eq!(files, vec!["b.rs".to_string()]);
}
