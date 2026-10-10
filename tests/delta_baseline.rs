//! `--delta` must compare against what the reader last saw (issue #107, design §5):
//! only `--delta` compiles advance the snapshot, and `keel map` resets it.
#![cfg(unix)]

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

use common::{git, keel_bin};

const CALLER_TS: &str = "/** Calls. */\nimport { target } from \"./target\";\nexport function caller(): void { target(); }\n";
const TARGET_TS: &str = "/** T. */\nexport function target(): void {}\n";
/// Same function with a new parameter: its caller breaks (E001), reliably, on every compile.
const TARGET_TS_EDITED: &str = "/** T. */\nexport function target(x: number): void {}\n";

fn keel_run(dir: &Path, args: &[&str]) -> Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to run `keel {}`: {e}", args.join(" ")))
}

fn out_s(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn describe(o: &Output) -> String {
    format!(
        "exit={:?}\nstdout: {}\nstderr: {}",
        o.status.code(),
        out_s(o),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// A committed, mapped two-file TypeScript repo.
fn mapped_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    for (rel, content) in [("src/caller.ts", CALLER_TS), ("src/target.ts", TARGET_TS)] {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    git(dir.path(), &["init", "-q"]);
    assert!(keel_run(dir.path(), &["init"]).status.success());
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "--no-verify", "-m", "first"]);
    map(dir.path());
    dir
}

fn map(dir: &Path) {
    let m = keel_run(dir, &["map"]);
    assert!(m.status.success(), "keel map failed: {}", describe(&m));
}

fn edit_target(dir: &Path) {
    fs::write(dir.join("src/target.ts"), TARGET_TS_EDITED).unwrap();
}

/// A whole-tree compile nobody reads must not become the `--delta` baseline: the
/// hook's next compile said "no changes" and exited 0 over a real E001.
#[test]
fn whole_tree_compile_does_not_silence_the_next_delta() {
    let dir = mapped_repo();
    edit_target(dir.path());

    let bare = keel_run(dir.path(), &["compile", "--llm"]);
    assert_eq!(
        bare.status.code(),
        Some(1),
        "bare compile\n{}",
        describe(&bare)
    );
    assert!(
        out_s(&bare).contains("E001"),
        "bare compile\n{}",
        describe(&bare)
    );

    let delta = keel_run(
        dir.path(),
        &["compile", "--delta", "--llm", "--", "src/target.ts"],
    );
    assert_eq!(
        delta.status.code(),
        Some(1),
        "delta after bare\n{}",
        describe(&delta)
    );
    assert!(
        out_s(&delta).contains("E001"),
        "delta after bare\n{}",
        describe(&delta)
    );
}

/// `keel map` resets the baseline: a finding reported before a re-map, then
/// reintroduced, must be reported again, not swallowed as already seen.
#[test]
fn map_resets_the_delta_baseline() {
    let dir = mapped_repo();
    edit_target(dir.path());
    let first = keel_run(
        dir.path(),
        &["compile", "--delta", "--llm", "--", "src/target.ts"],
    );
    assert_eq!(
        first.status.code(),
        Some(1),
        "first delta\n{}",
        describe(&first)
    );
    assert!(
        out_s(&first).contains("E001"),
        "first delta\n{}",
        describe(&first)
    );

    git(dir.path(), &["checkout", "--", "src/target.ts"]);
    map(dir.path());
    edit_target(dir.path());

    let again = keel_run(
        dir.path(),
        &["compile", "--delta", "--llm", "--", "src/target.ts"],
    );
    assert_eq!(
        again.status.code(),
        Some(1),
        "delta after re-map\n{}",
        describe(&again)
    );
    assert!(
        out_s(&again).contains("E001"),
        "delta after re-map\n{}",
        describe(&again)
    );
}

/// Control: with no map in between, the second `--delta` of the same unfixed
/// edit has nothing new to say and exits 0.
#[test]
fn control_second_delta_of_the_same_edit_is_quiet() {
    let dir = mapped_repo();
    edit_target(dir.path());
    let args = ["compile", "--delta", "--llm", "--", "src/target.ts"];
    let first = keel_run(dir.path(), &args);
    assert_eq!(
        first.status.code(),
        Some(1),
        "first delta\n{}",
        describe(&first)
    );
    let second = keel_run(dir.path(), &args);
    assert_eq!(
        second.status.code(),
        Some(0),
        "second delta\n{}",
        describe(&second)
    );
}
