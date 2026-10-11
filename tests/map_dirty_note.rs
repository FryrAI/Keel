//! `keel map` on a dirty tree says so (design S6n, rule N1/N2). Every test drives
//! the real `keel` binary. The note is ONE stderr line starting
//! `keel map: <m> modified and <u> untracked source file(s) were mapped as they are`;
//! a failing git listing gives ONE line `keel map: could not list uncommitted files: <error>`.
#![cfg(unix)]

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

use common::{git, keel_bin};

const MARK: &str = "were mapped as they are";
const COULD_NOT: &str = "keel map: could not list uncommitted files:";

const ALPHA: &str =
    "def alpha(value: int) -> int:\n    \"\"\"Return value.\"\"\"\n    return value\n";
const ALPHA_EDITED: &str = "def alpha(value: int, extra: int) -> int:\n    \"\"\"Return value.\"\"\"\n    return value + extra\n";
const BETA: &str =
    "from alpha import alpha\n\n\ndef beta() -> int:\n    \"\"\"Call alpha.\"\"\"\n    return alpha(1)\n";
const EXTRA: &str = "def extra() -> int:\n    \"\"\"Return one.\"\"\"\n    return 1\n";

fn keel_run(dir: &Path, args: &[&str]) -> Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .env("KEEL_NO_TELEMETRY", "true")
        .output()
        .unwrap_or_else(|e| panic!("failed to run `keel {}`: {e}", args.join(" ")))
}

fn out_s(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn err_s(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn describe(o: &Output) -> String {
    format!(
        "exit={:?}\nstdout: {}\nstderr: {}",
        o.status.code(),
        out_s(o),
        err_s(o)
    )
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn commit_all(dir: &Path) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--no-verify", "-m", "first"]);
}

/// The design's fixture: `alpha` and its caller `beta`, `keel init --yes`, committed.
fn committed_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "alpha.py", ALPHA);
    write(dir.path(), "beta.py", BETA);
    git(dir.path(), &["init", "-q"]);
    let init = keel_run(dir.path(), &["init", "--yes"]);
    assert!(init.status.success(), "keel init: {}", describe(&init));
    commit_all(dir.path());
    dir
}

/// The committed fixture with `alpha` gaining a required parameter, uncommitted.
fn dirty_repo() -> TempDir {
    let dir = committed_repo();
    write(dir.path(), "alpha.py", ALPHA_EDITED);
    dir
}

/// Stderr lines holding the dirty-tree note.
fn note_lines(o: &Output) -> Vec<String> {
    err_s(o)
        .lines()
        .filter(|l| l.contains(MARK))
        .map(str::to_owned)
        .collect()
}

fn assert_map_ok(o: &Output) {
    assert_eq!(
        o.status.code(),
        Some(0),
        "keel map must exit 0\n{}",
        describe(o)
    );
}

/// Exactly one note line, starting with `prefix`; returns it.
fn assert_one_note(o: &Output, prefix: &str) -> String {
    assert_map_ok(o);
    let lines = note_lines(o);
    assert_eq!(
        lines.len(),
        1,
        "stderr must hold exactly one `{MARK}` line\n{}",
        describe(o)
    );
    assert!(
        lines[0].starts_with(prefix),
        "the line must start with `{prefix}`, got `{}`\n{}",
        lines[0],
        describe(o)
    );
    lines[0].clone()
}

fn assert_no_note(o: &Output) {
    assert_map_ok(o);
    assert!(
        note_lines(o).is_empty(),
        "no note expected\n{}",
        describe(o)
    );
}

// ------------------------------------------------------------------ T1, T2

#[test]
fn one_modified_tracked_file_is_noted_once() {
    let dir = dirty_repo();
    let out = keel_run(dir.path(), &["map"]);
    let line = assert_one_note(
        &out,
        "keel map: 1 modified and 0 untracked source file(s) were mapped as they are",
    );
    assert!(line.contains("were not checked"), "{line}");
    assert_eq!(
        line,
        "keel map: 1 modified and 0 untracked source file(s) were mapped as they are — \
         uncommitted changes are now baseline and were not checked against their callers"
    );
}

#[test]
fn the_note_is_printed_with_verbose_after_the_summary() {
    let dir = dirty_repo();
    let out = keel_run(dir.path(), &["map", "--verbose"]);
    assert_one_note(&out, "keel map: 1 modified and 0 untracked");
    let last = err_s(&out).lines().last().unwrap_or("").to_owned();
    assert!(
        last.contains(MARK),
        "the note must be the last stderr line, after any verbose summary: `{last}`\n{}",
        describe(&out)
    );
}

#[test]
fn one_modified_and_two_untracked_files_are_counted_in_one_line() {
    let dir = dirty_repo();
    write(dir.path(), "gamma.py", EXTRA);
    write(dir.path(), "delta.py", EXTRA);
    let out = keel_run(dir.path(), &["map"]);
    assert_one_note(
        &out,
        "keel map: 1 modified and 2 untracked source file(s) were mapped as they are",
    );
}

// ---------------------------------------------------------------------- T3

#[test]
fn a_clean_tree_prints_no_note() {
    let dir = committed_repo();
    assert_no_note(&keel_run(dir.path(), &["map"]));
}

// ---------------------------------------------------------------------- T4

#[test]
fn outside_git_prints_no_note() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "alpha.py", ALPHA);
    write(dir.path(), "beta.py", BETA);
    let init = keel_run(dir.path(), &["init", "--yes"]);
    assert!(init.status.success(), "keel init: {}", describe(&init));
    assert!(
        !dir.path().join(".git").exists(),
        "fixture must be outside git"
    );
    write(dir.path(), "alpha.py", ALPHA_EDITED);
    write(dir.path(), "gamma.py", EXTRA);
    assert_no_note(&keel_run(dir.path(), &["map"]));
}

#[test]
fn a_repository_without_a_commit_prints_no_note() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "alpha.py", ALPHA);
    write(dir.path(), "beta.py", BETA);
    git(dir.path(), &["init", "-q"]);
    let init = keel_run(dir.path(), &["init", "--yes"]);
    assert!(init.status.success(), "keel init: {}", describe(&init));
    git(dir.path(), &["add", "-A"]);
    let head = Command::new("git")
        .args(["rev-parse", "--verify", "-q", "HEAD"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(!head.status.success(), "fixture must have no commit");
    assert_no_note(&keel_run(dir.path(), &["map"]));
}

#[test]
fn map_cached_on_a_dirty_tree_prints_no_note() {
    let dir = committed_repo();
    assert_map_ok(&keel_run(dir.path(), &["map"]));
    write(dir.path(), "alpha.py", ALPHA_EDITED);
    write(dir.path(), "gamma.py", EXTRA);
    let out = keel_run(dir.path(), &["map", "--cached"]);
    assert_no_note(&out);
    assert!(
        out_s(&out).contains("alpha"),
        "a stored map is printed\n{}",
        describe(&out)
    );
}

// ---------------------------------------------------------------------- T5

#[test]
fn json_stdout_stays_json_and_free_of_the_note() {
    let dir = dirty_repo();
    let out = keel_run(dir.path(), &["map", "--json"]);
    assert_map_ok(&out);
    let stdout = out_s(&out);
    serde_json::from_str::<serde_json::Value>(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{}", describe(&out)));
    assert!(
        !stdout.contains(MARK),
        "the note goes to stderr only\n{}",
        describe(&out)
    );
}

// ---------------------------------------------------------------------- T6

#[test]
fn a_non_source_untracked_file_does_not_count() {
    let dir = dirty_repo();
    write(dir.path(), "notes.md", "# notes\n");
    let out = keel_run(dir.path(), &["map"]);
    assert_one_note(
        &out,
        "keel map: 1 modified and 0 untracked source file(s) were mapped as they are",
    );
}

#[test]
fn a_keelignored_untracked_source_file_does_not_count() {
    let dir = dirty_repo();
    write(dir.path(), ".keelignore", "ignored.py\n");
    write(dir.path(), "ignored.py", EXTRA);
    let out = keel_run(dir.path(), &["map"]);
    assert_one_note(
        &out,
        "keel map: 1 modified and 0 untracked source file(s) were mapped as they are",
    );
}

// ---------------------------------------------------------------------- T7

/// A `git` that exits 128 on `ls-files --others` while `switch` exists and
/// otherwise runs the real one.
struct FakeGit {
    dir: TempDir,
}

impl FakeGit {
    fn new() -> Self {
        let real = Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .unwrap();
        let real = String::from_utf8(real.stdout).unwrap().trim().to_owned();
        assert!(real.starts_with('/'), "git resolves to an absolute path");
        let dir = TempDir::new().unwrap();
        let script = format!(
            "#!/bin/sh\nif [ -e '{}' ]; then\n  case \"$*\" in\n    *ls-files*) case \"$*\" in *--others*) exit 128;; esac;;\n  esac\nfi\nexec '{real}' \"$@\"\n",
            dir.path().join("fail-ls-files").display()
        );
        let git_path = dir.path().join("git");
        fs::write(&git_path, script).unwrap();
        fs::set_permissions(&git_path, fs::Permissions::from_mode(0o755)).unwrap();
        FakeGit { dir }
    }

    fn switch(&self) -> PathBuf {
        self.dir.path().join("fail-ls-files")
    }

    fn set(&self, on: bool) {
        if on {
            fs::write(self.switch(), "on").unwrap();
        } else if self.switch().exists() {
            fs::remove_file(self.switch()).unwrap();
        }
    }

    fn code(&self, root: &Path, args: &[&str]) -> Option<i32> {
        Command::new(self.dir.path().join("git"))
            .args(["-C", "."])
            .args(args)
            .current_dir(root)
            .output()
            .unwrap()
            .status
            .code()
    }

    fn path(&self) -> String {
        format!(
            "{}:{}",
            self.dir.path().display(),
            std::env::var("PATH").unwrap()
        )
    }
}

#[test]
fn a_failing_untracked_listing_is_one_could_not_list_line() {
    let dir = dirty_repo();
    let root = dir.path();
    let fake = FakeGit::new();
    // Positive control: fails exactly `ls-files --others`, and only while on.
    fake.set(true);
    assert_eq!(fake.code(root, &["ls-files", "--others", "-z"]), Some(128));
    assert_eq!(fake.code(root, &["ls-files", "-z"]), Some(0));
    assert_eq!(fake.code(root, &["rev-parse", "HEAD"]), Some(0));
    fake.set(false);
    assert_eq!(fake.code(root, &["ls-files", "--others", "-z"]), Some(0));

    fake.set(true);
    let out = Command::new(keel_bin())
        .arg("map")
        .current_dir(root)
        .env("KEEL_NO_TELEMETRY", "true")
        .env("PATH", fake.path())
        .output()
        .unwrap();
    assert_map_ok(&out);
    let lines: Vec<String> = err_s(&out)
        .lines()
        .filter(|l| l.starts_with(COULD_NOT))
        .map(str::to_owned)
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "stderr must hold exactly one `{COULD_NOT}` line\n{}",
        describe(&out)
    );
    assert!(
        note_lines(&out).is_empty(),
        "no count line on error\n{}",
        describe(&out)
    );
}
