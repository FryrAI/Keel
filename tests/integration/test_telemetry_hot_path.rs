//! T1.1: the compile hot path must never block on a network round trip.
//!
//! Regression guard for `crates/keel-cli/src/telemetry_recorder.rs`'s
//! `hot_path_commands()` gate and the (now conditional) `handle.join()` in
//! `main.rs`. A re-added blocking join, or a re-added remote send for a
//! hot-path command, would push wall time toward the telemetry client's
//! internal 2s timeout; correct behavior finishes in well under 200ms.

use std::fs;
use std::net::TcpListener;
use std::process::Command;
use std::time::{Duration, Instant};

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

fn init_and_map_project(files: &[(&str, &str)]) -> TempDir {
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
        .expect("Failed to run keel init");
    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");
    assert!(
        out.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

/// Point `.keel/keel.json`'s telemetry block at `endpoint` with `remote:
/// true` — the point of the test is that hot-path commands ignore this
/// setting entirely (no config override exists for them), so remote is
/// turned deliberately ON here to prove that.
fn set_remote_endpoint(dir: &TempDir, endpoint: &str) {
    let config_path = dir.path().join(".keel/keel.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap())
            .expect("keel.json should be valid JSON");
    config["telemetry"]["remote"] = serde_json::Value::Bool(true);
    config["telemetry"]["endpoint"] = serde_json::Value::String(endpoint.to_string());
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
        .expect("failed to write keel.json");
}

/// A local listener that completes the TCP handshake but never accepts or
/// responds — a reliable, network-sandbox-independent stand-in for a
/// blackhole/unroutable endpoint (loopback is always reachable, so unlike a
/// real unroutable IP this doesn't depend on the CI runner's outbound
/// egress policy to actually hang).
fn blackhole_endpoint() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind blackhole listener");
    let port = listener.local_addr().unwrap().port();
    let endpoint = format!("http://127.0.0.1:{port}/telemetry");
    (listener, endpoint)
}

/// Assert wall time for a set of hot-path commands, run against a blackhole
/// telemetry endpoint with remote reporting force-enabled. A generous but
/// still meaningful bound (2s) is used to avoid CI flakiness while still
/// catching a regression: ureq's internal client timeout is 2s, so any
/// re-introduced blocking network call pushes wall time to roughly that.
fn assert_hot_path_command_is_fast(dir: &TempDir, args: &[&str]) {
    let keel = keel_bin();
    let start = Instant::now();
    let output = Command::new(&keel)
        .args(args)
        .current_dir(dir.path())
        .output()
        .unwrap_or_else(|e| panic!("failed to run keel {}: {}", args.join(" "), e));
    let elapsed = start.elapsed();

    assert_ne!(
        output.status.code(),
        Some(2),
        "keel {} should not internal-error: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "keel {} took {:?} against a blackhole telemetry endpoint — \
         a network round trip is blocking the hot path",
        args.join(" "),
        elapsed
    );
}

#[test]
fn test_compile_hot_path_ignores_blackhole_telemetry_endpoint() {
    let dir = init_and_map_project(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let (listener, endpoint) = blackhole_endpoint();
    set_remote_endpoint(&dir, &endpoint);

    assert_hot_path_command_is_fast(&dir, &["compile", "src/index.ts"]);

    drop(listener);
}

#[test]
fn test_compile_llm_hot_path_ignores_blackhole_telemetry_endpoint() {
    let dir = init_and_map_project(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let (listener, endpoint) = blackhole_endpoint();
    set_remote_endpoint(&dir, &endpoint);

    assert_hot_path_command_is_fast(&dir, &["compile", "--llm", "src/index.ts"]);

    drop(listener);
}

#[test]
fn test_search_hot_path_ignores_blackhole_telemetry_endpoint() {
    let dir = init_and_map_project(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let (listener, endpoint) = blackhole_endpoint();
    set_remote_endpoint(&dir, &endpoint);

    assert_hot_path_command_is_fast(&dir, &["search", "hello"]);

    drop(listener);
}

/// `keel map` is NOT on the hot-path list — it is expected to still join its
/// remote-send thread (drained opportunistically, per T1.1's design). This
/// is a sanity check that the blackhole fixture itself actually hangs a
/// non-hot-path command, so the hot-path tests above are proving something.
#[test]
fn test_map_is_not_hot_path_and_does_block_on_remote_send() {
    let dir = init_and_map_project(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let (listener, endpoint) = blackhole_endpoint();
    set_remote_endpoint(&dir, &endpoint);

    let keel = keel_bin();
    let start = Instant::now();
    let output = Command::new(&keel)
        .args(["map"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");
    let elapsed = start.elapsed();

    drop(listener);

    assert_ne!(
        output.status.code(),
        Some(2),
        "map should not internal-error"
    );
    assert!(
        elapsed >= Duration::from_millis(1900),
        "keel map should join its remote telemetry send against a blackhole \
         endpoint (fixture sanity check), took {:?}",
        elapsed
    );
}
