//! The `.gitignore` entries `keel init` writes cover every file keel itself tells
//! an agent to produce, and `keel init --update-docs` brings an existing project's
//! ignore list up to date. Every test drives the real `keel` binary.
#![cfg(unix)]

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

use common::{git, keel_bin};

const CHECKPOINT_ENTRY: &str = ".keel/checkpoint.md";

const ALPHA: &str =
    "def alpha(value: int) -> int:\n    \"\"\"Return value.\"\"\"\n    return value\n";

fn keel_run(dir: &Path, args: &[&str]) -> Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .env("KEEL_NO_TELEMETRY", "true")
        .output()
        .unwrap_or_else(|e| panic!("failed to run `keel {}`: {e}", args.join(" ")))
}

fn describe(o: &Output) -> String {
    format!(
        "exit={:?}\nstdout: {}\nstderr: {}",
        o.status.code(),
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn keel_ok(dir: &Path, args: &[&str]) {
    let out = keel_run(dir, args);
    assert!(
        out.status.success(),
        "keel {}: {}",
        args.join(" "),
        describe(&out)
    );
}

/// `--untracked-files=all` lists each file: the default folds a lone untracked
/// directory such as `.keel/` into one line and would hide the checkpoint file.
fn git_status(dir: &Path) -> String {
    let out = Command::new("git")
        .args(["status", "--short", "--untracked-files=all"])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git status: {}", describe(&out));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A git repository holding one source file, initialised with `keel init --yes`.
fn initialised_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("alpha.py"), ALPHA).unwrap();
    git(dir.path(), &["init", "-q"]);
    keel_ok(dir.path(), &["init", "--yes"]);
    dir
}

/// The ignore file as the previous release wrote it: the checkpoint entry is
/// removed if the binary under test adds it, so the fixture is the same before
/// and after the change.
fn without_checkpoint_entry(gitignore: &str) -> String {
    gitignore
        .lines()
        .filter(|line| line.trim() != CHECKPOINT_ENTRY)
        .map(|line| format!("{line}\n"))
        .collect()
}

/// I1: following keel's own instruction leaves nothing untracked behind.
#[test]
fn i1_checkpoint_file_written_as_instructed_is_ignored() {
    let dir = initialised_repo();
    keel_ok(dir.path(), &["map"]);
    keel_ok(
        dir.path(),
        &["checkpoint", "--llm", "-o", ".keel/checkpoint.md"],
    );

    assert!(
        dir.path().join(CHECKPOINT_ENTRY).is_file(),
        "keel checkpoint wrote no {CHECKPOINT_ENTRY}"
    );
    let status = git_status(dir.path());
    assert!(
        !status.contains("checkpoint.md"),
        "{CHECKPOINT_ENTRY} is untracked after keel init:\n{status}"
    );
}

/// I2: an existing project gets the entry on its next `--update-docs`, once,
/// with the user's own lines untouched.
#[test]
fn i2_update_docs_adds_the_checkpoint_entry_once() {
    let dir = initialised_repo();
    let path = dir.path().join(".gitignore");
    let generated = without_checkpoint_entry(&fs::read_to_string(&path).unwrap());
    let before = format!("user-above.log\n{generated}user-below.log\n");
    fs::write(&path, &before).unwrap();

    keel_ok(dir.path(), &["init", "--update-docs"]);
    let after = fs::read_to_string(&path).unwrap();

    let count = after
        .lines()
        .filter(|line| line.trim() == CHECKPOINT_ENTRY)
        .count();
    assert_eq!(
        count, 1,
        "expected one {CHECKPOINT_ENTRY} line after --update-docs:\n{after}"
    );

    // Every previous line is still there, in the same order.
    let mut rest = after.lines();
    for line in before.lines() {
        assert!(
            rest.any(|l| l == line),
            "previous line {line:?} lost or moved by --update-docs:\n{after}"
        );
    }

    // The only new content line is the entry: blank lines and comments aside.
    let mut old: Vec<&str> = before.lines().collect();
    let added: Vec<&str> = after
        .lines()
        .filter(|line| match old.iter().position(|o| o == line) {
            Some(i) => {
                old.remove(i);
                false
            }
            None => true,
        })
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .collect();
    assert_eq!(added, [CHECKPOINT_ENTRY], "unexpected additions:\n{after}");

    keel_ok(dir.path(), &["init", "--update-docs"]);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        after,
        "a second --update-docs changed .gitignore"
    );
}

/// I3 control: a `.gitignore` that already has every entry is left as it is.
#[test]
fn i3_update_docs_leaves_a_complete_ignore_file_alone() {
    let dir = initialised_repo();
    let path = dir.path().join(".gitignore");
    let mut complete = fs::read_to_string(&path).unwrap();
    if !complete.lines().any(|line| line.trim() == CHECKPOINT_ENTRY) {
        complete.push_str(CHECKPOINT_ENTRY);
        complete.push('\n');
    }
    fs::write(&path, &complete).unwrap();

    keel_ok(dir.path(), &["init", "--update-docs"]);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        complete,
        "--update-docs rewrote a complete .gitignore"
    );
}
