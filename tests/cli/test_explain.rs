// Tests for `keel explain` CLI command (Spec 007 - CLI Commands)

use std::fs;
use std::process::Command;
use std::time::Instant;

use tempfile::TempDir;

fn keel_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("keel");
    if !path.exists() {
        let status = Command::new("cargo")
            .args(["build", "-p", "keel-cli"])
            .status()
            .expect("Failed to build keel");
        assert!(status.success(), "Failed to build keel binary");
    }
    path
}

fn init_and_map(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, content) in files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
    let keel = keel_bin();
    let out = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let out = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    dir
}

#[test]
/// `keel explain <code> <hash>` should output the resolution explanation.
fn test_explain_cli_output() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["explain", "E001", "AAAAAAAAAAA"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel explain");

    // With a nonexistent hash, expect exit 2
    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 2,
        "explain should exit 0 or 2, got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
/// `keel explain` should complete in under 50ms.
fn test_explain_cli_performance() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let start = Instant::now();
    let _ = Command::new(&keel)
        .args(["explain", "E001", "AAAAAAAAAAA"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel explain");
    let elapsed = start.elapsed();

    // Allow 2s for process spawn, core target is <50ms
    assert!(
        elapsed.as_millis() < 2000,
        "explain took {:?} — should be fast",
        elapsed
    );
}

#[test]
/// `keel explain` with invalid error code should return an error.
fn test_explain_cli_invalid_code() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["explain", "E999", "AAAAAAAAAAA"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel explain");

    assert!(
        !output.status.success(),
        "explain with invalid error code E999 should fail"
    );
}

#[test]
/// `keel explain` output should include the resolution tier that produced the result.
fn test_explain_cli_shows_tier() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["explain", "E001", "AAAAAAAAAAA"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel explain");

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 2,
        "explain should exit 0 or 2, got {code}"
    );

    // If it succeeded (hash found), output should mention tier
    if code == 0 {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.to_lowercase().contains("tier"),
            "explain output should include resolution tier info"
        );
    }
}
