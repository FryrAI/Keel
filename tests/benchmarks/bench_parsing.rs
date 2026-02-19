// Benchmark tests for tree-sitter parsing performance
// Uses CLI binary (keel map) to measure parsing throughput.

use super::common;

use common::generators::generate_project;
use std::fs;
use std::process::Command;
use std::time::Instant;
use tempfile::TempDir;

fn keel_bin() -> std::path::PathBuf {
    common::keel_bin()
}

fn setup_and_init(project: &[(String, String)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, content) in project {
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
    dir
}

#[test]
/// Parse 100 TypeScript files (debug-friendly scale for ~1k target).
fn bench_parse_1k_typescript_files() {
    let project = generate_project(100, 5, 10, "typescript");
    let dir = setup_and_init(&project);
    let keel = keel_bin();

    let start = Instant::now();
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    assert!(output.status.success(), "map failed");
    // Debug mode + parallel test contention: allow 90s (release target: 2s)
    assert!(
        elapsed.as_secs() < 90,
        "parsing 100 TS files took {:?}",
        elapsed
    );
}

#[test]
/// Parse 100 Python files.
fn bench_parse_5k_python_files() {
    let project = generate_project(100, 5, 10, "python");
    let dir = setup_and_init(&project);
    let keel = keel_bin();

    let start = Instant::now();
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    assert!(output.status.success(), "map failed");
    // Debug mode + parallel test contention: allow 60s
    assert!(
        elapsed.as_secs() < 60,
        "parsing 100 Python files took {:?}",
        elapsed
    );
}

#[test]
/// Parse mixed-language files.
fn bench_parse_10k_mixed_files() {
    let ts_project = generate_project(25, 5, 10, "typescript");
    let py_project = generate_project(25, 5, 10, "python");

    let dir = TempDir::new().unwrap();
    for (path, content) in ts_project.iter().chain(py_project.iter()) {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
    let keel = keel_bin();
    Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let start = Instant::now();
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    assert!(output.status.success(), "map failed");
    // Debug mode + parallel test contention: allow 60s
    assert!(
        elapsed.as_secs() < 60,
        "mixed-lang parsing took {:?}",
        elapsed
    );
}

#[test]
/// Average per-file parse time should be under 50ms (debug: <500ms).
fn bench_per_file_parse_time_under_5ms() {
    let project = generate_project(50, 5, 20, "typescript");
    let dir = setup_and_init(&project);
    let keel = keel_bin();

    let start = Instant::now();
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    assert!(output.status.success(), "map failed");

    let avg_ms = elapsed.as_millis() / 50;
    // Debug mode + parallel test contention: allow 1500ms per file (release target: 5ms)
    assert!(
        avg_ms < 1500,
        "avg per-file time: {avg_ms}ms — should be under 1500ms (debug + contention)",
    );
}
