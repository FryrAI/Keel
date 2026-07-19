// Tests for `keel checkpoint` — compact session-state summary from git + graph.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn keel_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("keel");
    if path.exists() {
        return path;
    }
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fallback = workspace.join("target/debug/keel");
    if fallback.exists() {
        return fallback;
    }
    let status = Command::new("cargo")
        .args(["build", "-p", "keel-cli"])
        .current_dir(&workspace)
        .status()
        .expect("Failed to build keel");
    assert!(status.success(), "Failed to build keel binary");
    fallback
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "user.email=test@keel.dev",
            "-c",
            "user.name=keel test",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git command failed to run");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn keel(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run keel")
}

/// A committed + mapped cross-file fixture: lib.ts::foo called by main.ts::run.
fn committed_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("lib.ts"),
        "export function foo(x: number): number {\n  return x + 1;\n}\n",
    )
    .unwrap();
    fs::write(
        src.join("main.ts"),
        "import { foo } from './lib';\nexport function run(): number {\n  return foo(1);\n}\n",
    )
    .unwrap();

    git(root, &["init"]);
    let init = keel(root, &["init"]);
    assert!(init.status.success());
    // Commit AFTER keel init so .keel/ is part of the baseline commit.
    // `--no-verify`: keel installs a pre-commit hook that would reject this
    // fixture (missing docstrings) — we only need a HEAD to diff against.
    git(root, &["add", "-A"]);
    git(root, &["commit", "--no-verify", "-m", "baseline"]);

    let map = keel(root, &["map"]);
    assert!(
        map.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&map.stderr)
    );
    dir
}

#[test]
fn test_checkpoint_reports_changed_symbols_and_callers() {
    let dir = committed_fixture();
    let root = dir.path();

    // Edit foo's body (→ changed) and append a new function (→ added).
    fs::write(
        root.join("src/lib.ts"),
        "export function foo(x: number): number {\n  return x + 2;\n}\n\
         export function baz(): number {\n  return 0;\n}\n",
    )
    .unwrap();

    let out = keel(root, &["checkpoint", "--llm"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "checkpoint should exit 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("CHECKPOINT"), "missing header: {stdout}");
    assert!(
        stdout.contains("src/lib.ts"),
        "missing changed file: {stdout}"
    );
    assert!(stdout.contains("foo"), "foo should be reported: {stdout}");
    assert!(
        stdout.contains("baz"),
        "new baz should be reported as added: {stdout}"
    );
    // Structural impact: main.ts calls foo, so it is a caller at risk.
    assert!(
        stdout.contains("RISK foo") && stdout.contains("src/main.ts"),
        "foo's caller in main.ts should be at risk: {stdout}"
    );
    // The violation summary counts are always present.
    assert!(
        stdout.contains("errors=") && stdout.contains("warnings="),
        "checkpoint header should carry violation counts: {stdout}"
    );
}

#[test]
fn test_checkpoint_staged_mode() {
    let dir = committed_fixture();
    let root = dir.path();

    fs::write(
        root.join("src/lib.ts"),
        "export function foo(x: number): number {\n  return x + 99;\n}\n",
    )
    .unwrap();
    git(root, &["add", "src/lib.ts"]);

    let out = keel(root, &["checkpoint", "--staged", "--llm"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("range=staged"),
        "should be staged range: {stdout}"
    );
    assert!(
        stdout.contains("src/lib.ts"),
        "should show staged file: {stdout}"
    );
    assert!(stdout.contains("foo"), "should show changed foo: {stdout}");
}

#[test]
fn test_checkpoint_writes_to_file() {
    let dir = committed_fixture();
    let root = dir.path();

    fs::write(
        root.join("src/lib.ts"),
        "export function foo(x: number): number {\n  return x + 5;\n}\n",
    )
    .unwrap();

    let out_path = root.join("cp.md");
    let out = keel(
        root,
        &["checkpoint", "--llm", "-o", out_path.to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(0));
    // Nothing on stdout when writing to a file.
    assert!(String::from_utf8_lossy(&out.stdout).trim().is_empty());
    let written = fs::read_to_string(&out_path).expect("checkpoint file should exist");
    assert!(written.contains("CHECKPOINT"), "file content: {written}");
    assert!(written.contains("foo"));
}

#[test]
fn test_checkpoint_not_initialized_exits_2() {
    let dir = TempDir::new().unwrap();
    let out = keel(dir.path(), &["checkpoint"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not initialized") || stderr.contains("init"));
}
