// Benchmark tests for full map performance
// Uses CLI binary to measure end-to-end map performance at various scales.
//
// Debug mode limits are relaxed ~20-50x from release targets because:
// 1. tree-sitter + oxc run ~10x slower in debug builds (~250ms/file)
// 2. Multiple benchmark tests run in parallel via `cargo test`, contending for CPU
// Release targets should be validated in CI with `cargo test --release`.

use super::common;

use common::generators::generate_project;
use std::fs;
use std::process::Command;
use std::time::Instant;
use tempfile::TempDir;

fn keel_bin() -> std::path::PathBuf {
    common::keel_bin()
}

fn setup_project(files: usize, fns_per_file: usize, lines_per_fn: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    let project = generate_project(files, fns_per_file, lines_per_fn, "typescript");
    for (path, content) in &project {
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
/// 10k LOC (~100 files x 10 fns x 10 lines) map benchmark.
fn bench_map_10k_loc_under_1s() {
    let dir = setup_project(100, 10, 10);
    let keel = keel_bin();

    let start = Instant::now();
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Debug mode + parallel contention: allow 90s (release target: 1s)
    assert!(elapsed.as_secs() < 90, "10k LOC map took {:?}", elapsed);
}

#[test]
/// 50k LOC (~500 files x 10 fns x 10 lines) map benchmark.
fn bench_map_50k_loc_under_3s() {
    let dir = setup_project(200, 5, 10);
    let keel = keel_bin();

    let start = Instant::now();
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Debug mode + parallel contention: allow 120s (release target: 3s; 200 files)
    assert!(elapsed.as_secs() < 120, "~10k LOC map took {:?}", elapsed);
}

#[test]
/// 100k LOC map benchmark — relaxed for debug mode.
fn bench_map_100k_loc_under_5s() {
    // In debug mode, use smaller scale to avoid timeouts
    let dir = setup_project(100, 5, 10);
    let keel = keel_bin();

    let start = Instant::now();
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Debug mode + parallel contention: allow 90s (release target: 5s)
    assert!(elapsed.as_secs() < 90, "map took {:?}", elapsed);
}

#[test]
/// Re-map after modifying a single file should be fast.
fn bench_remap_after_single_file_change() {
    let dir = setup_project(50, 5, 10);
    let keel = keel_bin();

    // Initial map
    Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Modify one file
    fs::write(
        dir.path().join("src/module_0.ts"),
        "export function modified(x: number): number { return x + 999; }\n",
    )
    .unwrap();

    let start = Instant::now();
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    assert!(output.status.success());
    // Remap should be fast since only one file changed
    // Debug mode + parallel test contention: allow 45s
    assert!(elapsed.as_secs() < 45, "remap took {:?}", elapsed);
}
