//! Tests for `.keel` directory resolution (issue #29 — worktree-aware graph).

use super::{confine, keel_dir, make_relative, worktree_root};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// `make_relative` strips the root for a path underneath it, and passes through
/// both out-of-root and already-relative paths unchanged.
#[test]
fn make_relative_strips_root_and_passes_through() {
    let root = Path::new("/home/x/repo");
    assert_eq!(
        make_relative(root, Path::new("/home/x/repo/src/a.rs")),
        "src/a.rs"
    );
    assert_eq!(make_relative(root, Path::new("src/a.rs")), "src/a.rs");
    assert_eq!(make_relative(root, Path::new("/other/b.rs")), "/other/b.rs");
    // A hash-like non-path argument is returned verbatim.
    assert_eq!(make_relative(root, Path::new("a1b2c3")), "a1b2c3");
}

/// Run a git command in `cwd`, asserting success.
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

/// (a) A normal checkout resolves `.keel` to the repo root.
#[test]
fn normal_repo_resolves_to_repo_root_keel() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(&["init"], root);

    assert_eq!(keel_dir(root), root.join(".keel"));
}

/// (c) With no `.git` anywhere, fall back to `<start>/.keel`.
#[test]
fn no_git_falls_back_to_cwd_keel() {
    let dir = TempDir::new().unwrap();
    let start = dir.path();

    assert_eq!(keel_dir(start), start.join(".keel"));
}

/// (b) A linked worktree resolves `.keel` to the MAIN checkout, so every
/// worktree of the repo shares one `.keel/graph.db`.
#[test]
fn linked_worktree_resolves_to_main_checkout_keel() {
    let dir = TempDir::new().unwrap();

    // Main checkout with one commit (required before `git worktree add`).
    let main = dir.path().join("main");
    std::fs::create_dir_all(&main).unwrap();
    git(&["init"], &main);
    git(&["config", "user.email", "test@test.com"], &main);
    git(&["config", "user.name", "Test"], &main);
    std::fs::write(main.join("f.txt"), "hi").unwrap();
    git(&["add", "."], &main);
    git(&["commit", "-m", "init"], &main);

    // Add a linked worktree (creates a `.git` FILE inside `wt/`).
    let wt = dir.path().join("wt");
    git(&["worktree", "add", wt.to_str().unwrap()], &main);
    assert!(wt.join(".git").is_file(), "worktree .git should be a file");

    let resolved = keel_dir(&wt);
    assert_eq!(resolved.file_name().unwrap(), ".keel");
    assert_eq!(
        resolved.parent().unwrap(),
        main.canonicalize().unwrap(),
        "worktree must share the main checkout's .keel",
    );
}

/// `worktree_root` is the nearest `.git`-holding ancestor: the repo root for a
/// normal checkout, the worktree itself for a linked worktree, `None` outside
/// any repository — never the main checkout `keel_dir` resolves to.
#[test]
fn worktree_root_is_the_nearest_dot_git_ancestor() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main");
    std::fs::create_dir_all(main.join("sub")).unwrap();
    git(&["init"], &main);
    git(&["config", "user.email", "test@test.com"], &main);
    git(&["config", "user.name", "Test"], &main);
    std::fs::write(main.join("f.txt"), "hi").unwrap();
    git(&["add", "."], &main);
    git(&["commit", "-m", "init"], &main);
    let wt = dir.path().join("wt");
    git(&["worktree", "add", wt.to_str().unwrap()], &main);

    assert_eq!(worktree_root(&main.join("sub")), Some(main.clone()));
    assert_eq!(worktree_root(&wt), Some(wt.clone()));
    assert_eq!(worktree_root(dir.path()), None);
}

/// An empty directory named `.git` (e.g. a sandbox marker, or a stray
/// `mkdir .git`) is not a repository and must not hijack root resolution for
/// a project living beneath it: `keel_dir` falls back to the no-git default
/// and `worktree_root` is `None`.
#[test]
fn empty_dot_git_directory_is_not_a_repository() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).unwrap();

    assert_eq!(keel_dir(&project), project.join(".keel"));
    assert_eq!(worktree_root(&project), None);
}

/// A real repo nested below a fake (empty-directory) `.git` still resolves to
/// its own root — the walk must skip the fake entry and keep going up... but
/// stop at the first REAL one, not fall through past it too.
#[test]
fn nested_project_below_a_fake_dot_git_still_finds_its_own_repo() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    git(&["init"], &project);

    assert_eq!(worktree_root(&project.join("src")), Some(project.clone()));
    assert_eq!(keel_dir(&project), project.join(".keel"));
}

// --- confine: path confinement for server-side surfaces ---
// (moved here from keel-server::http_confine, which was a pure delegate)

fn root() -> PathBuf {
    PathBuf::from("/home/user/project")
}

#[test]
fn accepts_relative_inside_root() {
    let got = confine(&root(), "src/main.rs").unwrap();
    assert_eq!(got, PathBuf::from("/home/user/project/src/main.rs"));
}

#[test]
fn accepts_nested_relative_inside_root() {
    let got = confine(&root(), "a/b/../c.rs").unwrap();
    assert_eq!(got, PathBuf::from("/home/user/project/a/c.rs"));
}

#[test]
fn rejects_parent_escape() {
    assert!(confine(&root(), "../../etc/passwd").is_none());
}

#[test]
fn rejects_absolute_outside_root() {
    assert!(confine(&root(), "/etc/passwd").is_none());
}

#[test]
fn accepts_absolute_inside_root() {
    let got = confine(&root(), "/home/user/project/src/lib.rs").unwrap();
    assert_eq!(got, PathBuf::from("/home/user/project/src/lib.rs"));
}

#[test]
fn rejects_sneaky_prefix_sibling() {
    // A sibling dir that merely shares a name prefix must not pass.
    assert!(confine(&root(), "/home/user/project-evil/x.rs").is_none());
}

/// A symlink inside the root pointing outside it must be rejected even
/// though it passes the lexical check (the walker/read would follow it).
#[cfg(unix)]
#[test]
fn rejects_symlink_escaping_root() {
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "s").unwrap();
    let project = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(project.path()).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join("evil")).unwrap();
    std::fs::write(root.join("ok.rs"), "fn a() {}").unwrap();

    assert!(confine(&root, "evil").is_none(), "symlinked dir must fail");
    assert!(
        confine(&root, "evil/secret.txt").is_none(),
        "file through symlinked dir must fail"
    );
    assert!(confine(&root, "ok.rs").is_some(), "real file still passes");
}
