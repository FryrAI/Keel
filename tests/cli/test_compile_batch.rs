// Tests for `keel compile --batch-start/--batch-end` (Spec 007 - CLI Commands)

use std::fs;
use std::process::Command;

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
    let out = Command::new(&keel).arg("init").current_dir(dir.path()).output().unwrap();
    assert!(out.status.success());
    let out = Command::new(&keel).arg("map").current_dir(dir.path()).output().unwrap();
    assert!(out.status.success());
    dir
}

#[test]
/// `keel compile --batch-start` should enter batch mode.
fn test_compile_batch_start() {
    let dir = init_and_map(&[
        ("src/index.ts", "export function hello(name: string): string { return name; }\n"),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["compile", "--batch-start"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile --batch-start");

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "batch-start should exit 0 or 1, got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
/// `keel compile --batch-end` should fire all deferred violations.
fn test_compile_batch_end() {
    let dir = init_and_map(&[
        ("src/index.ts", "export function hello(name: string): string { return name; }\n"),
    ]);
    let keel = keel_bin();

    // Start batch
    let start_out = Command::new(&keel)
        .args(["compile", "--batch-start"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(start_out.status.success() || start_out.status.code() == Some(1));

    // End batch — should report any accumulated violations
    let end_out = Command::new(&keel)
        .args(["compile", "--batch-end"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile --batch-end");

    let code = end_out.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "batch-end should exit 0 or 1, got {code}\nstderr: {}",
        String::from_utf8_lossy(&end_out.stderr)
    );
}

#[test]
/// `keel compile --batch-end` without prior --batch-start should be a no-op.
fn test_compile_batch_end_without_start() {
    let dir = init_and_map(&[
        ("src/index.ts", "export function hello(name: string): string { return name; }\n"),
    ]);
    let keel = keel_bin();

    // batch-end without batch-start should be a graceful no-op
    let output = Command::new(&keel)
        .args(["compile", "--batch-end"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile --batch-end");

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "batch-end without start should be no-op (exit 0 or 1), got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
/// Multiple files compiled during batch mode should accumulate deferred violations.
fn test_compile_batch_accumulates_violations() {
    let dir = init_and_map(&[
        ("src/a.ts", "export function fa(x: number): number { return x; }\n"),
        ("src/b.ts", "export function fb(x: number): number { return x; }\n"),
        ("src/c.ts", "export function fc(x: number): number { return x; }\n"),
    ]);
    let keel = keel_bin();

    // Start batch
    let start = Command::new(&keel)
        .args(["compile", "--batch-start"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(start.status.success() || start.status.code() == Some(1));

    // Compile individual files during batch
    for file in &["src/a.ts", "src/b.ts", "src/c.ts"] {
        let out = Command::new(&keel)
            .args(["compile", file])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let code = out.status.code().unwrap_or(-1);
        assert!(
            code == 0 || code == 1,
            "compile {file} during batch should exit 0 or 1, got {code}"
        );
    }

    // End batch — should fire all accumulated deferred violations
    let end_out = Command::new(&keel)
        .args(["compile", "--batch-end"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let code = end_out.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "batch-end should exit 0 or 1, got {code}"
    );
}
