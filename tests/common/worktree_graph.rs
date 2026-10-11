//! Fixtures and oracles shared by `tests/graph_per_worktree.rs` and
//! `tests/graph_per_worktree_layouts.rs` (issue #107, item C).
//!
//! Oracles are logical: a dump of nodes (name, kind, file, signature) and
//! reference edges (source name, target name, kind) plus the two map markers,
//! read through a fresh read-only connection - never file bytes - and always
//! checked to be non-empty, so two empty graphs can never compare equal.
#![allow(dead_code)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rusqlite::{Connection, OpenFlags};
use tempfile::TempDir;

use crate::common::{git, keel_bin};

pub const ALPHA: &str = "def alpha(value: int) -> int:\n    \"\"\"Alpha.\"\"\"\n    return value\n";
pub const ALPHA_BROKEN: &str =
    "def alpha(value: int, extra: int) -> int:\n    \"\"\"Alpha.\"\"\"\n    return value + extra\n";
pub const BETA: &str =
    "from alpha import alpha\n\n\ndef beta() -> int:\n    \"\"\"Beta.\"\"\"\n    return alpha(1)\n";
pub const GAMMA: &str = "def gamma() -> int:\n    \"\"\"Gamma.\"\"\"\n    return 3\n";
pub const REFUSAL_PREFIX: &str = "keel compile: no usable graph — ";

pub fn keel(dir: &Path, args: &[&str]) -> Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .env("KEEL_NO_TELEMETRY", "true")
        .output()
        .unwrap_or_else(|e| panic!("failed to run `keel {}`: {e}", args.join(" ")))
}

pub fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

pub fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

pub fn describe(o: &Output) -> String {
    format!(
        "exit={:?}\nstdout: {}\nstderr: {}",
        o.status.code(),
        out(o),
        err(o)
    )
}

pub fn keel_ok(dir: &Path, args: &[&str]) -> Output {
    let o = keel(dir, args);
    assert!(
        o.status.success(),
        "`keel {}` failed in {}\n{}",
        args.join(" "),
        dir.display(),
        describe(&o)
    );
    o
}

pub fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

pub fn git_out(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "git {args:?} failed in {}: {}",
        dir.display(),
        err(&o)
    );
    out(&o).trim().to_string()
}

pub fn commit_all(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--no-verify", "-m", msg]);
}

/// A committed repository holding `alpha.py`, `beta.py` (calls `alpha(1)`) and
/// `sub/notes.txt`; `keel init` runs first when `init_keel`.
pub fn seed_repo(dir: &Path, init_keel: bool) {
    fs::create_dir_all(dir).unwrap();
    write(dir, "alpha.py", ALPHA);
    write(dir, "beta.py", BETA);
    write(dir, "sub/notes.txt", "notes\n");
    git(dir, &["init", "-q", "-b", "main"]);
    if init_keel {
        keel_ok(dir, &["init"]);
    }
    commit_all(dir, "base");
}

/// A main checkout (initialised, mapped) and a linked worktree on a branch that
/// diverges by one commit (it adds `gamma.py`).
pub struct Pair {
    pub tmp: TempDir,
    pub main: PathBuf,
    pub wt: PathBuf,
}

/// `map_wt`: map the worktree too. `untrack_config`: `.keel/keel.json` is
/// untracked before the worktree exists.
pub fn pair(map_wt: bool, untrack_config: bool) -> Pair {
    let tmp = TempDir::new().unwrap();
    let main = tmp.path().join("main");
    let wt = tmp.path().join("wt");
    seed_repo(&main, true);
    if untrack_config {
        git(&main, &["rm", "--cached", "-q", ".keel/keel.json"]);
        git(
            &main,
            &["commit", "-q", "--no-verify", "-m", "untrack config"],
        );
    }
    keel_ok(&main, &["map"]);
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
    write(&wt, "gamma.py", GAMMA);
    commit_all(&wt, "add gamma");
    if map_wt {
        keel_ok(&wt, &["map"]);
    }
    Pair { tmp, main, wt }
}

pub fn pair_mapped() -> Pair {
    pair(true, false)
}

/// `main` mapped, the worktree never mapped.
pub fn pair_new_worktree() -> Pair {
    pair(false, false)
}

fn canon(p: PathBuf) -> PathBuf {
    fs::canonicalize(&p).unwrap_or(p)
}

pub fn abs_git_dir(dir: &Path) -> PathBuf {
    canon(PathBuf::from(git_out(
        dir,
        &["rev-parse", "--absolute-git-dir"],
    )))
}

pub fn abs_common_dir(dir: &Path) -> PathBuf {
    let p = PathBuf::from(git_out(dir, &["rev-parse", "--git-common-dir"]));
    canon(if p.is_absolute() { p } else { dir.join(p) })
}

/// Where the graph database of the checkout at `dir` must live: under its own
/// git directory for a linked worktree, in `.keel` otherwise.
pub fn graph_db(dir: &Path) -> PathBuf {
    let git_dir = abs_git_dir(dir);
    if git_dir != abs_common_dir(dir) {
        git_dir.join("keel").join("graph.db")
    } else {
        dir.join(".keel").join("graph.db")
    }
}

/// File names directly inside `dir`.
pub fn entries(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect()
}

/// A logical dump of one graph database plus its two map markers.
#[derive(Debug, PartialEq)]
pub struct Snap {
    pub nodes: Vec<String>,
    pub edges: Vec<String>,
    pub last_map_at: Option<String>,
    pub last_map_commit: Option<String>,
}

pub fn snap(db: &Path) -> Snap {
    assert!(
        db.is_file(),
        "no graph database at {} - a linked worktree's graph must live under its own git directory",
        db.display()
    );
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let rows = |sql: &str| -> Vec<String> {
        conn.prepare(sql)
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    let marker = |key: &str| -> Option<String> {
        conn.query_row("SELECT value FROM keel_meta WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .ok()
    };
    Snap {
        nodes: rows("SELECT json_array(name,kind,file_path,signature) FROM nodes ORDER BY 1"),
        edges: rows(
            "SELECT json_array(s.name,t.name,e.kind) FROM edges e \
             JOIN nodes s ON s.id=e.source_id JOIN nodes t ON t.id=e.target_id ORDER BY 1",
        ),
        last_map_at: marker("last_map_at"),
        last_map_commit: marker("last_map_commit"),
    }
}

/// The dump is a real graph: `beta` calls `alpha`, and `gamma` is present
/// exactly when the checkout has `gamma.py`.
pub fn assert_populated(s: &Snap, expect_gamma: bool) {
    let has = |list: &[String], needle: &str| list.iter().any(|l| l == needle);
    assert!(
        has(
            &s.nodes,
            r#"["alpha","function","alpha.py","alpha(value: int) -> int"]"#
        ),
        "{s:?}"
    );
    assert!(has(&s.edges, r#"["beta","alpha","calls"]"#), "{s:?}");
    assert_eq!(
        s.nodes.iter().any(|n| n.starts_with(r#"["gamma","#)),
        expect_gamma,
        "{s:?}"
    );
    assert!(s.last_map_at.is_some(), "{s:?}");
}

/// T1's pin: `alpha` gains a required parameter in `violator` and is compiled
/// there (`E001`); the same compile in `other` is clean and `other`'s logical
/// dump and both markers are unchanged. `cwd`/`alpha_arg` let a caller run the
/// violating compile from a subdirectory.
pub fn assert_violation_isolated(violator: &Path, cwd: &Path, alpha_arg: &str, other: &Path) {
    let before = snap(&graph_db(other));
    assert_populated(&before, other.join("gamma.py").exists());
    assert!(
        graph_db(violator).is_file(),
        "violator has no graph at {}",
        graph_db(violator).display()
    );
    write(violator, "alpha.py", ALPHA_BROKEN);
    let v = keel(cwd, &["compile", alpha_arg]);
    assert_eq!(
        v.status.code(),
        Some(1),
        "violator compile\n{}",
        describe(&v)
    );
    assert!(
        out(&v).contains("E001"),
        "violator compile must report E001\n{}",
        describe(&v)
    );
    let c = keel(other, &["compile", "alpha.py"]);
    assert_eq!(
        c.status.code(),
        Some(0),
        "the same compile in the other checkout\n{}",
        describe(&c)
    );
    assert!(
        out(&c).is_empty(),
        "the other checkout must compile clean\n{}",
        describe(&c)
    );
    assert_eq!(
        snap(&graph_db(other)),
        before,
        "the other checkout's graph changed"
    );
}
