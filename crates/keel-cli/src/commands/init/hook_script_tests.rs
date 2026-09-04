//! Behaviour of the installed `post-edit.sh` (issue #73).
//!
//! The hook used to swallow every diagnostic: `RESULT=$(keel …)` under
//! `set -e` aborted the script on any non-zero exit, before the captured
//! output could be written to stderr — so the editor reported a block with
//! "No stderr output". These tests run the real template against a fake
//! `keel` first on `PATH`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

use super::install_post_edit_hook;

/// Whether `tool` is on `PATH`. The hook needs `bash` and `jq`; environments
/// without them skip these tests, as the plan-hook suite does.
fn have(tool: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool}"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// A repo root with the hook installed, one source file, and a fake `keel`
/// that exits `code`, prints `out`, and touches `keel-ran` when invoked.
fn fixture(code: i32, out: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    install_post_edit_hook(dir.path(), false);
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.ts"), "export const a = 1;\n").unwrap();

    let bin = dir.path().join("fakebin");
    fs::create_dir_all(&bin).unwrap();
    let marker = dir.path().join("keel-ran");
    let echo = if out.is_empty() {
        String::new()
    } else {
        format!("echo '{out}'\n")
    };
    let fake = bin.join("keel");
    fs::write(
        &fake,
        format!(
            "#!/bin/bash\ntouch '{}'\n{echo}exit {code}\n",
            marker.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}

/// Feed the hook a Claude Code `PostToolUse` payload for `file_path`.
fn run_hook(root: &Path, file_path: &Path) -> Output {
    let payload = serde_json::json!({
        "tool_input": { "file_path": file_path.to_string_lossy() },
    })
    .to_string();
    let path = format!(
        "{}:{}",
        root.join("fakebin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut child = Command::new("bash")
        .arg(".keel/hooks/post-edit.sh")
        .arg("claude-code")
        .current_dir(root)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn hook");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// The tracked file, with the root resolved the way the hook resolves it.
fn target(root: &Path) -> PathBuf {
    root.canonicalize().unwrap().join("src/a.ts")
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn test_violations_block_and_carry_the_diagnostic() {
    if !have("bash") || !have("jq") {
        eprintln!("skipping: bash/jq not available");
        return;
    }
    let dir = fixture(1, "VIOLATION E001 broken_caller");
    let out = run_hook(dir.path(), &target(dir.path()));

    let stderr = stderr_of(&out);
    assert_eq!(
        out.status.code(),
        Some(2),
        "violations must block: {stderr}"
    );
    assert!(
        stderr.contains("VIOLATION E001"),
        "keel's output must reach stderr: {stderr}"
    );
}

#[test]
fn test_internal_error_surfaces_without_blocking() {
    if !have("bash") || !have("jq") {
        eprintln!("skipping: bash/jq not available");
        return;
    }
    let dir = fixture(2, "keel compile: stale graph");
    let out = run_hook(dir.path(), &target(dir.path()));

    let stderr = stderr_of(&out);
    assert_eq!(
        out.status.code(),
        Some(1),
        "an internal error is not the agent's to fix: {stderr}"
    );
    assert!(
        stderr.contains("stale graph"),
        "the reason must reach stderr: {stderr}"
    );
}

#[test]
fn test_silent_failure_still_says_something() {
    if !have("bash") || !have("jq") {
        eprintln!("skipping: bash/jq not available");
        return;
    }
    let dir = fixture(1, "");
    let out = run_hook(dir.path(), &target(dir.path()));
    let stderr = stderr_of(&out);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr.contains("compile exited 1 with no output"),
        "a block with no stderr is the bug: {stderr}"
    );

    let dir = fixture(124, "");
    let out = run_hook(dir.path(), &target(dir.path()));
    let stderr = stderr_of(&out);
    assert_eq!(out.status.code(), Some(1), "a timeout must not block");
    assert!(
        stderr.contains("timed out after 5s"),
        "a timeout must name itself: {stderr}"
    );
}

#[test]
fn test_paths_outside_the_repository_are_skipped() {
    if !have("bash") || !have("jq") {
        eprintln!("skipping: bash/jq not available");
        return;
    }
    let dir = fixture(1, "SHOULD NOT RUN");
    let elsewhere = TempDir::new().unwrap();
    let outside = elsewhere.path().canonicalize().unwrap().join("memory.md");
    fs::write(&outside, "notes\n").unwrap();

    let out = run_hook(dir.path(), &outside);
    assert_eq!(
        out.status.code(),
        Some(0),
        "an out-of-tree write must not block: {}",
        stderr_of(&out)
    );
    assert!(
        !dir.path().join("keel-ran").exists(),
        "keel must not be invoked for a file outside the repo"
    );
}

/// The scope check must run before the character whitelist: an external path
/// with a space used to be *rejected* (blocking) before it could be skipped.
#[test]
fn test_outside_paths_with_spaces_are_skipped_not_rejected() {
    if !have("bash") || !have("jq") {
        eprintln!("skipping: bash/jq not available");
        return;
    }
    let dir = fixture(1, "SHOULD NOT RUN");
    let elsewhere = TempDir::new().unwrap();
    let notes = elsewhere
        .path()
        .canonicalize()
        .unwrap()
        .join("Outside Notes");
    fs::create_dir_all(&notes).unwrap();
    let outside = notes.join("example.ts");
    fs::write(&outside, "export {}\n").unwrap();

    let out = run_hook(dir.path(), &outside);
    assert_eq!(
        out.status.code(),
        Some(0),
        "an out-of-tree path with a space must be skipped: {}",
        stderr_of(&out)
    );
    assert!(!dir.path().join("keel-ran").exists());
}

/// An in-tree path keel refuses to pass along is a file it could not check —
/// surfaced with its reason, never a block the agent is told to fix.
#[test]
fn test_rejected_in_tree_path_surfaces_without_blocking() {
    if !have("bash") || !have("jq") {
        eprintln!("skipping: bash/jq not available");
        return;
    }
    let dir = fixture(1, "SHOULD NOT RUN");
    let spaced = dir.path().canonicalize().unwrap().join("src/my file.ts");
    fs::write(&spaced, "export {}\n").unwrap();

    let out = run_hook(dir.path(), &spaced);
    let stderr = stderr_of(&out);
    assert_eq!(
        out.status.code(),
        Some(1),
        "a rejection must not block: {stderr}"
    );
    assert!(
        stderr.contains("unexpected characters"),
        "the rejection must say why: {stderr}"
    );
    assert!(!dir.path().join("keel-ran").exists());
}
