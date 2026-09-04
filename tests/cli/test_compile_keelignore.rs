// Issue #70: `keel compile --changed` takes its file list from `git diff`,
// which happily lists tracked files the walker never sees. Before the ignore
// filter moved into the shared git-diff helper, a vendored tree excluded by
// `.keelignore` was still compiled — the pre-commit hook raised errors and
// warnings against third-party source that is not in the graph at all.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use crate::common::{git, keel_bin};

fn keel(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run keel")
}

fn write(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// A committed, mapped repo whose `.keelignore` excludes `vendor/`, holding one
/// clean function on each side of that line. Both files are tracked by git
/// (`.keelignore` is keel's, not git's), so editing either shows up in
/// `git diff --name-only HEAD`.
fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(root, ".keelignore", "vendor/\n");
    write(
        root,
        "src/app.py",
        "def app(x: int) -> int:\n    \"\"\"Doc.\"\"\"\n    return x\n",
    );
    write(
        root,
        "vendor/lib.py",
        "def lib(x: int) -> int:\n    \"\"\"Doc.\"\"\"\n    return x\n",
    );
    git(root, &["init", "-q"]);
    assert!(keel(root, &["init"]).status.success(), "keel init failed");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "first"]);
    assert!(keel(root, &["map"]).status.success(), "keel map failed");
    dir
}

/// The whole of #70 in one test: the same violation is invisible inside the
/// ignored tree and still fatal outside it.
#[test]
fn compile_changed_honors_keelignore() {
    let dir = fixture();
    let root = dir.path();
    // Missing type hints and docstring: E002 + E003 on any graded file.
    let violation = "def compute(value):\n    return value\n";

    write(root, "vendor/lib.py", violation);
    let out = keel(root, &["compile", "--changed"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "a changed file under .keelignore must not be checked; stdout: {stdout} stderr: {stderr}"
    );
    assert!(
        stdout.trim().is_empty() && stderr.trim().is_empty(),
        "an ignored file must produce no output at all; stdout: {stdout} stderr: {stderr}"
    );

    write(root, "src/app.py", violation);
    let out = keel(root, &["compile", "--changed"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "the same violation in tracked source must still fire; stdout: {stdout}"
    );
    assert!(
        stdout.contains("E002"),
        "the src/ violation must be reported: {stdout}"
    );
    assert!(
        !stdout.contains("vendor/"),
        "the ignored tree must stay out of the report: {stdout}"
    );
}
