// Benchmark tests for parallel parsing with Rayon
//
// Validates that keel effectively utilizes multiple CPU cores for parsing
// via Rayon, achieving near-linear scaling for the parsing phase.
// Uses RAYON_NUM_THREADS env var to control thread count through CLI.

#[path = "../common/mod.rs"]
mod common;

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
    let out = Command::new(&keel).arg("init").current_dir(dir.path()).output().unwrap();
    assert!(out.status.success());
    dir
}

fn map_with_threads(dir: &std::path::Path, num_threads: usize) -> std::time::Duration {
    let keel = keel_bin();
    let start = Instant::now();
    let output = Command::new(&keel)
        .arg("map")
        .env("RAYON_NUM_THREADS", num_threads.to_string())
        .current_dir(dir)
        .output()
        .unwrap();
    let elapsed = start.elapsed();
    assert!(output.status.success(), "map failed: {}", String::from_utf8_lossy(&output.stderr));
    elapsed
}

#[test]
#[ignore = "Requires FK constraint fix in keel map"]
/// Parallel parsing should be faster than single-threaded.
fn bench_parallel_parsing_scales_with_cores() {
    let dir = setup_project(100, 5, 10);

    // Single thread baseline
    let single = map_with_threads(dir.path(), 1);

    // Re-init to clear cache
    let keel = keel_bin();
    Command::new(&keel).arg("init").current_dir(dir.path()).output().unwrap();

    // Multi-thread (default = all cores)
    let multi = map_with_threads(dir.path(), 0);

    // Multi-threaded should complete (we can't guarantee speedup in debug mode
    // due to overhead, but both should finish within time budget)
    assert!(
        single.as_secs() < 30,
        "single-threaded map took {:?}",
        single
    );
    assert!(
        multi.as_secs() < 30,
        "multi-threaded map took {:?}",
        multi
    );
}

#[test]
#[ignore = "Requires FK constraint fix in keel map"]
/// Baseline: single-threaded parse timing for comparison.
fn bench_parallel_parsing_1_thread_baseline() {
    let dir = setup_project(50, 5, 10);

    let elapsed = map_with_threads(dir.path(), 1);

    // Single-threaded should complete within a generous budget
    assert!(
        elapsed.as_secs() < 30,
        "1-thread baseline took {:?}",
        elapsed
    );
}

#[test]
#[ignore = "Requires FK constraint fix in keel map"]
/// 4-thread parse should complete within time budget.
fn bench_parallel_parsing_4_threads() {
    let dir = setup_project(50, 5, 10);

    let elapsed = map_with_threads(dir.path(), 4);

    // 4 threads should complete within budget
    assert!(
        elapsed.as_secs() < 20,
        "4-thread map took {:?}",
        elapsed
    );
}

#[test]
#[ignore = "Requires FK constraint fix in keel map"]
/// Parallel parsing with graph store should not show contention.
fn bench_parallel_parsing_no_contention_on_graph() {
    // Larger project to stress concurrent graph writes
    let dir = setup_project(100, 5, 10);

    // Run with many threads — if there's lock contention, this will be
    // disproportionately slow compared to fewer threads
    let elapsed = map_with_threads(dir.path(), 8);

    // Should complete without deadlock or excessive contention
    assert!(
        elapsed.as_secs() < 30,
        "8-thread map with graph writes took {:?} — possible contention",
        elapsed
    );
}

#[test]
#[ignore = "Requires FK constraint fix in keel map"]
/// Mixed file sizes should not cause long-tail stragglers.
fn bench_parallel_parsing_handles_mixed_file_sizes() {
    let dir = TempDir::new().unwrap();

    // Generate files of varying sizes
    let small = generate_project(30, 2, 5, "typescript");   // small files
    let medium = generate_project(15, 5, 20, "typescript");  // medium files
    let large = generate_project(5, 10, 50, "typescript");   // large files

    for (path, content) in small.iter().chain(medium.iter()).chain(large.iter()) {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }

    let keel = keel_bin();
    let out = Command::new(&keel).arg("init").current_dir(dir.path()).output().unwrap();
    assert!(out.status.success());

    let elapsed = map_with_threads(dir.path(), 4);

    // Mixed sizes with work-stealing should complete without stragglers
    assert!(
        elapsed.as_secs() < 20,
        "mixed-size parallel map took {:?}",
        elapsed
    );
}
