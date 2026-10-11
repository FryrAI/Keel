//! Repository layouts (T8, R1), the uncreatable graph directory (T9) and the
//! `keel.json` publish (T10) for the graph-per-worktree rule (issue #107).
//!
//! Each layout maps itself through the real `keel` binary and reports T1's
//! violation; where the graph database appears is asserted by path existence.

#[path = "common/mod.rs"]
mod common;
#[path = "common/worktree_graph.rs"]
mod wg;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use common::git;
use wg::*;

/// Every file named `name` under `root`.
fn find_files(root: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap().map(|e| e.unwrap()) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if entry.file_name() == name {
                found.push(path);
            }
        }
    }
    found
}

/// A pointer that fails R1 falls back to `keel_dir`: the one graph under `root`
/// sits beside `keel.json` and never under the git directory's `keel`.
fn assert_graph_beside_config(root: &Path, git_dir: &Path) {
    let graphs = find_files(root, "graph.db");
    assert_eq!(
        graphs.len(),
        1,
        "expected exactly one graph.db under {}: {graphs:?}",
        root.display()
    );
    let home = graphs[0].parent().unwrap();
    assert!(
        home.join("keel.json").is_file(),
        "graph.db is not beside keel.json: {}",
        graphs[0].display()
    );
    assert!(
        !git_dir.join("keel").exists(),
        "a graph_dir was created under the git directory"
    );
}

fn assert_reports_violation(dir: &Path) {
    write(dir, "alpha.py", ALPHA_BROKEN);
    let o = keel(dir, &["compile", "alpha.py"]);
    assert_eq!(o.status.code(), Some(1), "{}", describe(&o));
    assert!(out(&o).contains("E001"), "{}", describe(&o));
}

/// A `gitdir:` pointer written relative to the worktree.
#[test]
fn t8_relative_gitdir_pointer_keeps_the_graph_in_the_worktree_git_dir() {
    let p = pair_new_worktree();
    let name = abs_git_dir(&p.wt)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    fs::write(
        p.wt.join(".git"),
        format!("gitdir: ../main/.git/worktrees/{name}\n"),
    )
    .unwrap();
    assert_eq!(
        git_out(&p.wt, &["rev-parse", "--is-inside-work-tree"]),
        "true",
        "fixture: relative pointer"
    );
    keel_ok(&p.wt, &["map"]);
    assert_violation_isolated(&p.wt, &p.wt, "alpha.py", &p.main);
}

/// `keel map` and `keel compile` run from a subdirectory of the worktree.
#[test]
fn t8_worktree_subdirectory_as_cwd_uses_the_worktree_graph() {
    let p = pair_new_worktree();
    let sub = p.wt.join("sub");
    keel_ok(&sub, &["map"]);
    assert_violation_isolated(&p.wt, &sub, "../alpha.py", &p.main);
}

/// A bare repository with two linked worktrees: each has its own graph.
#[test]
fn t8_bare_repository_worktrees_each_have_their_own_graph() {
    let tmp = tempfile::TempDir::new().unwrap();
    let (seed, bare) = (tmp.path().join("seed"), tmp.path().join("bare.git"));
    seed_repo(&seed, false);
    git(
        tmp.path(),
        &[
            "clone",
            "-q",
            "--bare",
            seed.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );
    git(&bare, &["worktree", "add", "-q", "../w1", "main"]);
    git(
        &bare,
        &["worktree", "add", "-q", "-b", "w2", "../w2", "main"],
    );
    let (w1, w2) = (tmp.path().join("w1"), tmp.path().join("w2"));
    keel_ok(&w1, &["init"]);
    keel_ok(&w1, &["map"]);
    keel_ok(&w2, &["map"]);
    assert_ne!(
        graph_db(&w1),
        graph_db(&w2),
        "fixture: two worktrees, one graph path"
    );
    assert_violation_isolated(&w1, &w1, "alpha.py", &w2);
}

/// A submodule checkout is not a linked worktree: its graph stays in its own `.keel`.
#[test]
fn t8_submodule_graph_stays_in_its_own_keel_dir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let (src, sup) = (tmp.path().join("src"), tmp.path().join("super"));
    seed_repo(&src, false);
    fs::create_dir_all(&sup).unwrap();
    git(&sup, &["init", "-q", "-b", "main"]);
    write(&sup, "README.md", "super\n");
    commit_all(&sup, "super");
    git(
        &sup,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            src.to_str().unwrap(),
            "sub",
        ],
    );
    let sub = sup.join("sub");
    keel_ok(&sub, &["init"]);
    keel_ok(&sub, &["map"]);
    assert!(
        sub.join(".keel/graph.db").is_file(),
        "the submodule's graph is not in its own .keel"
    );
    assert!(
        !abs_git_dir(&sub).join("keel").exists(),
        "a graph_dir was created under the submodule's git directory"
    );
    assert_populated(&snap(&sub.join(".keel/graph.db")), false);
    assert_reports_violation(&sub);
}

/// A `.git` file whose `gitdir:` target does not exist: `graph_dir == keel_dir`.
#[test]
fn t8_gitdir_target_missing_keeps_the_graph_beside_keel_json() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    write(&dir, "alpha.py", ALPHA);
    write(&dir, "beta.py", BETA);
    let missing = tmp.path().join("missing-git-dir");
    fs::write(dir.join(".git"), format!("gitdir: {}\n", missing.display())).unwrap();
    keel_ok(&dir, &["init"]);
    keel_ok(&dir, &["map"]);
    assert_graph_beside_config(tmp.path(), &missing);
    assert_populated(&snap(&find_files(tmp.path(), "graph.db")[0]), false);
    assert_reports_violation(&dir);
}

/// A worktree pointer whose `commondir` target does not exist: `graph_dir == keel_dir`.
#[test]
fn t8_commondir_target_missing_keeps_the_graph_beside_keel_json() {
    let tmp = tempfile::TempDir::new().unwrap();
    let (main, wt) = (tmp.path().join("main"), tmp.path().join("wt"));
    seed_repo(&main, false);
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            wt.to_str().unwrap(),
        ],
    );
    let admin = abs_git_dir(&wt);
    fs::write(admin.join("commondir"), "../missing-common-dir\n").unwrap();
    keel_ok(&wt, &["init"]);
    keel_ok(&wt, &["map"]);
    assert_graph_beside_config(tmp.path(), &admin);
    assert_populated(&snap(&find_files(tmp.path(), "graph.db")[0]), false);
    assert_reports_violation(&wt);
}

// ------------------------------------------------------------------- T9

#[cfg(unix)]
mod readonly {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    pub fn running_as_root() -> bool {
        let o = std::process::Command::new("id").arg("-u").output().unwrap();
        out(&o).trim() == "0"
    }

    /// Makes a directory read-only, restoring it on drop so the tempdir can go.
    pub struct Locked(pub std::path::PathBuf);

    impl Locked {
        pub fn new(dir: &Path) -> Locked {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o555)).unwrap();
            Locked(dir.to_path_buf())
        }
    }

    impl Drop for Locked {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
        }
    }

    pub fn assert_one_line_naming(o: &Output, graph_dir: &Path) {
        assert_eq!(o.status.code(), Some(2), "{}", describe(o));
        let stderr = err(o);
        let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 1, "expected ONE stderr line\n{}", describe(o));
        assert!(
            lines[0].contains(graph_dir.to_str().unwrap()),
            "the line must name {}\n{}",
            graph_dir.display(),
            describe(o)
        );
    }
}

/// T9 (R4): a `graph_dir` that cannot be created stops `keel map` with one
/// stderr line naming the path, exit 2.
#[cfg(unix)]
#[test]
fn t9_map_with_uncreatable_graph_dir_is_one_line_exit_2() {
    if readonly::running_as_root() {
        return;
    }
    let p = pair_new_worktree();
    let git_dir = abs_git_dir(&p.wt);
    let _locked = readonly::Locked::new(&git_dir);
    let o = keel(&p.wt, &["map"]);
    readonly::assert_one_line_naming(&o, &git_dir.join("keel"));
}

/// T9 (R4, F05): the same for a nominal reader.
#[cfg(unix)]
#[test]
fn t9_reader_with_uncreatable_graph_dir_is_one_line_exit_2() {
    if readonly::running_as_root() {
        return;
    }
    let p = pair_new_worktree();
    let git_dir = abs_git_dir(&p.wt);
    let _locked = readonly::Locked::new(&git_dir);
    let o = keel(&p.wt, &["map", "--cached"]);
    readonly::assert_one_line_naming(&o, &git_dir.join("keel"));
}

// ------------------------------------------------------------------- T10

/// T10 (R7): a config write publishes `keel.json` by rename - a new inode, the
/// old one untouched until then - and leaves no temporary file beside it.
#[cfg(unix)]
#[test]
fn t10_config_write_replaces_keel_json_by_rename() {
    use std::os::unix::fs::MetadataExt;
    let tmp = tempfile::TempDir::new().unwrap();
    seed_repo(tmp.path(), true);
    let file = tmp.path().join(".keel/keel.json");
    let (files, inode) = (
        entries(&tmp.path().join(".keel")),
        fs::metadata(&file).unwrap().ino(),
    );
    keel_ok(tmp.path(), &["config", "enforce.max_file_lines", "500"]);
    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(
        parsed["enforce"]["max_file_lines"], 500,
        "the write did not land"
    );
    assert_ne!(
        fs::metadata(&file).unwrap().ino(),
        inode,
        "keel.json was rewritten in place, not renamed"
    );
    assert_eq!(
        entries(&tmp.path().join(".keel")),
        files,
        "a temporary file was left behind"
    );
}

/// T10 (R7): `keel init --merge` leaves no temporary file and `keel.json` parses.
/// GREEN-pin: nothing is left behind today and nothing may be.
#[test]
fn t10_init_merge_leaves_no_temporary_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    seed_repo(tmp.path(), true);
    let files = entries(&tmp.path().join(".keel"));
    keel_ok(tmp.path(), &["init", "--merge"]);
    let text = fs::read_to_string(tmp.path().join(".keel/keel.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(
        parsed["enforce"].is_object(),
        "keel.json lost its content: {text}"
    );
    assert_eq!(
        entries(&tmp.path().join(".keel")),
        files,
        "a temporary file was left behind"
    );
}
