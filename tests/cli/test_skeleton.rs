// Tests for `keel skeleton` command (issue #21).

use std::fs;
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

fn project_with(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, content) in files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
    dir
}

const SAMPLE_TS: &str = "import { helper } from './helper';\n\
     export function publicApi(a: number): string { const x = a + 1; return `${x}`; }\n\
     function privateHelper(): void { console.log('body text here'); }\n";

#[test]
/// `keel skeleton <file>` compresses a file to signatures (no bodies), no map needed.
fn test_skeleton_signatures_only() {
    let dir = project_with(&[("src/index.ts", SAMPLE_TS)]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["skeleton", "src/index.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel skeleton");

    assert!(
        output.status.success(),
        "skeleton should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("publicApi"),
        "should list the public signature"
    );
    // No body text leaks into the signature view.
    assert!(!stdout.contains("console.log"), "bodies must not appear");
    // Public-only by default.
    assert!(
        !stdout.contains("privateHelper"),
        "private hidden by default"
    );
}

#[test]
/// `keel skeleton <file> --json` routes through the formatter and emits valid JSON.
fn test_skeleton_json_is_valid() {
    let dir = project_with(&[("src/index.ts", SAMPLE_TS)]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["skeleton", "src/index.ts", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel skeleton");

    assert!(output.status.success(), "skeleton --json should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\n---\n{stdout}"));
    assert_eq!(parsed["command"], "skeleton");
    assert_eq!(parsed["language"], "typescript");
    assert!(parsed["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["name"] == "publicApi"));
}

#[test]
/// `--private` includes private symbols; `--docs` includes docstrings.
fn test_skeleton_private_and_docs() {
    let dir = project_with(&[(
        "m.py",
        "def public_fn(a: int) -> str:\n    \"\"\"A documented function.\"\"\"\n    return str(a)\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["skeleton", "m.py", "--docs", "--private"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel skeleton");

    assert!(output.status.success(), "skeleton should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("public_fn"));
    assert!(
        stdout.contains("A documented function"),
        "--docs should include the docstring, got:\n{stdout}"
    );
}

#[test]
/// Unsupported file types exit 2 with a clear error.
fn test_skeleton_unsupported_file() {
    let dir = project_with(&[("notes.txt", "just text\n")]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["skeleton", "notes.txt"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel skeleton");

    assert_eq!(output.status.code(), Some(2));
}
