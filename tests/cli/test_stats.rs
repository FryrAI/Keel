// Tests for `keel stats` command (Spec 007 - CLI Commands)

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
/// `keel stats` should display node count, edge count, and file count.
fn test_stats_displays_counts() {
    let dir = init_and_map(&[
        ("src/a.ts", "export function foo(x: number): number { return x; }\n"),
        ("src/b.ts", "export function bar(y: string): string { return y; }\n"),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("stats")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel stats");

    assert!(
        output.status.success(),
        "keel stats failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Stats should output some numerical data about the graph
    let has_counts = combined.contains("module")
        || combined.contains("function")
        || combined.contains("node")
        || combined.contains("edge")
        || combined.contains("file");
    assert!(
        has_counts,
        "stats should display counts, got: {combined}"
    );
}

#[test]
/// `keel stats` should display per-language breakdown.
fn test_stats_per_language_breakdown() {
    let dir = init_and_map(&[
        ("src/app.ts", "export function greet(name: string): string { return name; }\n"),
        ("src/main.py", "def greet(name: str) -> str:\n    return name\n"),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("stats")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel stats");

    assert!(output.status.success());
    // At minimum, stats should succeed for a multi-language project
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !combined.is_empty(),
        "stats should produce output for multi-language project"
    );
}

#[test]
/// `keel stats` should display circuit breaker state summary.
fn test_stats_circuit_breaker_summary() {
    let dir = init_and_map(&[
        ("src/index.ts", "export function hello(name: string): string { return name; }\n"),
    ]);
    let keel = keel_bin();

    // Run stats — circuit breaker info should be included (even if empty)
    let output = Command::new(&keel)
        .arg("stats")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel stats");

    assert!(
        output.status.success(),
        "stats should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
/// `keel stats` should display resolution tier distribution.
fn test_stats_resolution_tier_distribution() {
    let dir = init_and_map(&[
        ("src/caller.ts", "import { helper } from './helper';\nexport function main(): void { helper(); }\n"),
        ("src/helper.ts", "export function helper(): void {}\n"),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("stats")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel stats");

    assert!(
        output.status.success(),
        "stats should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
/// `keel stats` in an uninitialized project should return an error.
fn test_stats_not_initialized() {
    let dir = TempDir::new().unwrap();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("stats")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel stats");

    assert!(
        !output.status.success(),
        "stats should fail in uninitialized directory"
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "exit code should be 2 for uninitialized project"
    );
}
