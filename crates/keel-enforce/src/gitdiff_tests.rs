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

/// Issue #70: git lists a tracked-but-ignored file in its diff, the walker
/// never does. Every git-diff-driven command (compile, audit, checkpoint) must
/// see the same scope the graph has, or a vendored tree raises violations
/// against third-party source in the pre-commit hook.
#[test]
fn keelignore_drops_ignored_paths() {
    let dir = init_repo();
    std::fs::write(dir.path().join(".keelignore"), "vendor/\n").unwrap();
    std::fs::create_dir(dir.path().join("vendor")).unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("vendor/lib.rs"), "fn v() {}\n").unwrap();
    std::fs::write(dir.path().join("src/app.rs"), "fn a() {}\n").unwrap();
    // `-f`: git itself would skip the ignored path, and the bug only shows on
    // a tracked one.
    git(&["add", "-f", "."], dir.path());

    let files = changed_files(dir.path(), &DiffMode::Since(None), true);
    assert_eq!(files, vec!["src/app.rs".to_string()]);
}

/// `git diff` prints repository-root-relative paths whatever directory it is
/// run from, so the ignore rules must be read from the repo root — not from the
/// caller's cwd, where a nested invocation (a hook run in a subdirectory) would
/// find no `.keelignore` at all and check the vendored tree anyway.
#[test]
fn keelignore_is_read_from_the_repo_root_not_the_cwd() {
    let dir = init_repo();
    std::fs::write(dir.path().join(".keelignore"), "vendor/\n").unwrap();
    std::fs::create_dir(dir.path().join("vendor")).unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("vendor/lib.rs"), "fn v() {}\n").unwrap();
    std::fs::write(dir.path().join("src/app.rs"), "fn a() {}\n").unwrap();
    git(&["add", "-f", "."], dir.path());

    let files = changed_files(&dir.path().join("src"), &DiffMode::Since(None), true);
    assert_eq!(files, vec!["src/app.rs".to_string()]);
}

/// A rename out of an ignored tree is an ADDITION: the base side is not in the
/// graph, so `keel review` must score the arriving symbols as new rather than
/// parse the ignored blob and call them relocated.
#[test]
fn a_rename_out_of_an_ignored_tree_becomes_an_addition() {
    let dir = renamed_repo("vendor/old.py", "src/new.py");

    let paths = changed_paths(dir.path(), "HEAD").unwrap();
    assert_eq!(
        paths,
        vec![ChangedPath {
            path: "src/new.py".to_string(),
            status: ChangeStatus::Added,
        }]
    );
}

/// A rename into an ignored tree is a DELETION at the old path: dropping the
/// record whole would hide the contracts the move removed from the graph.
#[test]
fn a_rename_into_an_ignored_tree_becomes_a_deletion() {
    let dir = renamed_repo("src/old.py", "vendor/new.py");

    let paths = changed_paths(dir.path(), "HEAD").unwrap();
    assert_eq!(
        paths,
        vec![ChangedPath {
            path: "src/old.py".to_string(),
            status: ChangeStatus::Deleted,
        }]
    );
}

/// A repo ignoring `vendor/` where `from` has been committed and then `git mv`d
/// to `to`, staged so `git diff -M HEAD` sees both sides of the rename.
fn renamed_repo(from: &str, to: &str) -> TempDir {
    let dir = init_repo();
    std::fs::write(dir.path().join(".keelignore"), "vendor/\n").unwrap();
    std::fs::create_dir(dir.path().join("vendor")).unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join(from),
        "def moved(value):\n    return value\n",
    )
    .unwrap();
    git(&["add", "-f", "."], dir.path());
    git(&["commit", "-m", "init"], dir.path());
    git(&["mv", from, to], dir.path());
    dir
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

/// Every exit status git can produce for `merge-base --is-ancestor`, and the
/// one rule that matters: anything that is not a clear "no" must read as
/// `Unknown`, so a missing git or an unknown object never fails a build.
#[test]
fn ancestry_classification_only_trusts_zero_and_one() {
    assert_eq!(classify_ancestry(Some(0)), Ancestry::Ancestor);
    assert_eq!(classify_ancestry(Some(1)), Ancestry::NotAncestor);
    assert_eq!(classify_ancestry(Some(128)), Ancestry::Unknown);
    assert_eq!(classify_ancestry(Some(129)), Ancestry::Unknown);
    assert_eq!(classify_ancestry(None), Ancestry::Unknown);
}

/// Real git: a commit on the current history is an ancestor, a commit on a
/// history HEAD was rewritten away from is not, and an object this repo never
/// heard of is unknowable rather than "not an ancestor".
#[test]
fn ancestry_against_a_real_repository() {
    let dir = init_repo();
    std::fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    git(&["add", "a.rs"], dir.path());
    git(&["commit", "-m", "first"], dir.path());
    let first = head_commit(dir.path()).expect("HEAD after the first commit");

    std::fs::write(dir.path().join("b.rs"), "fn b() {}\n").unwrap();
    git(&["add", "b.rs"], dir.path());
    git(&["commit", "-m", "second"], dir.path());

    assert_eq!(is_ancestor(dir.path(), &first, "HEAD"), Ancestry::Ancestor);
    let second = head_commit(dir.path()).unwrap();
    assert_eq!(is_ancestor(dir.path(), &second, "HEAD"), Ancestry::Ancestor);

    // Rewrite history: `second` is now unreachable from HEAD.
    git(&["reset", "--hard", &first], dir.path());
    std::fs::write(dir.path().join("c.rs"), "fn c() {}\n").unwrap();
    git(&["add", "c.rs"], dir.path());
    git(&["commit", "-m", "divergent"], dir.path());
    assert_eq!(
        is_ancestor(dir.path(), &second, "HEAD"),
        Ancestry::NotAncestor
    );

    assert_eq!(
        is_ancestor(
            dir.path(),
            "0000000000000000000000000000000000000000",
            "HEAD"
        ),
        Ancestry::Unknown,
        "an object this repo does not have is unknowable, not a stale graph"
    );
}

/// A repository with no commits has no HEAD — and `keel map` must not stamp a
/// commit marker it would then have to defend.
#[test]
fn head_commit_is_none_before_the_first_commit() {
    let dir = init_repo();
    assert!(head_commit(dir.path()).is_none());
}
