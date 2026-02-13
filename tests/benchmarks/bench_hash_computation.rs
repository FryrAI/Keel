// Benchmark tests for hash computation performance

use keel_core::hash::compute_hash;
use std::time::Instant;

#[test]
fn bench_hash_1k_functions() {
    let start = Instant::now();
    for i in 0..1_000 {
        let sig = format!("fn func_{i}(x: i32) -> i32");
        let body = format!("x + {i}");
        let doc = format!("Computes value {i}");
        let _hash = compute_hash(&sig, &body, &doc);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 100,
        "1k hashes took {:?} — should be under 100ms",
        elapsed
    );
}

#[test]
fn bench_hash_10k_functions() {
    let start = Instant::now();
    for i in 0..10_000 {
        let sig = format!("fn func_{i}(x: i32) -> i32");
        let body = format!("x + {i}");
        let doc = format!("Computes value {i}");
        let _hash = compute_hash(&sig, &body, &doc);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 500,
        "10k hashes took {:?} — should be under 500ms",
        elapsed
    );
}

#[test]
fn bench_hash_100k_functions() {
    let start = Instant::now();
    for i in 0..100_000 {
        let sig = format!("fn func_{i}(x: i32) -> i32");
        let body = format!("x + {i}");
        let doc = format!("Computes value {i}");
        let _hash = compute_hash(&sig, &body, &doc);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 5,
        "100k hashes took {:?} — should be under 5s in debug",
        elapsed
    );
}

#[test]
fn bench_hash_determinism_across_runs() {
    let mut hashes_run1 = Vec::with_capacity(1_000);
    let mut hashes_run2 = Vec::with_capacity(1_000);

    for i in 0..1_000 {
        let sig = format!("fn func_{i}(x: i32) -> i32");
        let body = format!("x + {i}");
        let doc = format!("Computes value {i}");
        hashes_run1.push(compute_hash(&sig, &body, &doc));
    }

    for i in 0..1_000 {
        let sig = format!("fn func_{i}(x: i32) -> i32");
        let body = format!("x + {i}");
        let doc = format!("Computes value {i}");
        hashes_run2.push(compute_hash(&sig, &body, &doc));
    }

    assert_eq!(
        hashes_run1, hashes_run2,
        "hashes should be deterministic across runs"
    );
}
