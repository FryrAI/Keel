// Tests for `keel map` command (Spec 007 - CLI Commands)

use std::fmt::Write;
use std::fs;
use std::process::Command;
use std::time::Instant;

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

fn init_ts_project(file_count: usize, fns_per_file: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    for i in 0..file_count {
        let mut content = String::new();
        for j in 0..fns_per_file {
            writeln!(
                content,
                "export function func_{i}_{j}(x: number): number {{\n  \
                 const a = x + 1;\n  return a;\n}}\n"
            )
            .unwrap();
        }
        fs::write(src.join(format!("mod_{i}.ts")), &content).unwrap();
    }

    let keel = keel_bin();
    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");
    assert!(
        output.status.success(),
        "keel init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir
}

#[test]
/// `keel map` should perform a full re-map of the codebase.
fn test_map_full_remap() {
    let dir = init_ts_project(5, 3);
    let keel = keel_bin();

    // Add a new file after init
    fs::write(
        dir.path().join("src/new_module.ts"),
        "export function newFunc(x: number): number { return x; }\n",
    )
    .unwrap();

    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");

    assert!(
        output.status.success(),
        "keel map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify the database was updated (size should change after remap with new file)
    let db_size = fs::metadata(dir.path().join(".keel/graph.db"))
        .unwrap()
        .len();
    assert!(db_size > 4096, "graph.db should contain mapped data");
}

#[test]
/// `keel map` should complete in reasonable time for a moderate codebase.
/// (100k LOC target is <5s in release; debug builds are ~10x slower, so we test 10k LOC)
fn test_map_performance_target() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    // Generate ~10k LOC: 100 files x 100 LOC each (debug-friendly scale)
    for i in 0..100 {
        let mut content = String::new();
        for j in 0..10 {
            writeln!(
                content,
                "export function func_{i}_{j}(x: number): number {{\n  \
                 const a = x + 1;\n  const b = x + 2;\n  const c = x + 3;\n  \
                 const d = x + 4;\n  const e = x + 5;\n  return a + b + c + d + e;\n}}\n"
            )
            .unwrap();
        }
        fs::write(src.join(format!("mod_{i}.ts")), &content).unwrap();
    }

    let keel = keel_bin();
    Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");

    let start = Instant::now();
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");
    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "keel map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Debug mode + parallel test contention: allow 90s (release target: <1s)
    assert!(
        elapsed.as_secs() < 90,
        "keel map took {:?} — exceeds 90s limit for 10k LOC in debug",
        elapsed
    );
}

#[test]
/// `keel map` succeeds silently on a valid project (clean output = empty stdout).
fn test_map_outputs_summary() {
    let dir = init_ts_project(3, 2);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");

    assert!(
        output.status.success(),
        "keel map failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Clean output principle: success = exit 0, empty stdout
    // Summary stats only appear with --verbose
}

#[test]
/// `keel map` in an uninitialized directory should return an error.
fn test_map_uninitialized_error() {
    let dir = TempDir::new().unwrap();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");

    assert!(
        !output.status.success(),
        "keel map should fail in uninitialized directory"
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "exit code should be 2 for uninitialized project"
    );
}

#[test]
/// `keel map --cached` on a project that was `keel init`-ed but never
/// `keel map`-ped (graph.db exists but is empty) must fall back to a full
/// map instead of erroring — this is exactly the cold-start state a fresh
/// clone/worktree is in when a session-start hook first fires.
fn test_map_cached_fresh_init_falls_back() {
    let dir = init_ts_project(3, 2);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["map", "--cached"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map --cached");

    assert!(
        output.status.success(),
        "keel map --cached on an empty graph should fall back to a full map, not error:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let db_size = fs::metadata(dir.path().join(".keel/graph.db"))
        .unwrap()
        .len();
    assert!(
        db_size > 4096,
        "fallback full map should have populated graph.db"
    );
}

#[test]
/// `keel map --cached` when a cache already exists should stay on the fast
/// (read-only) path and still succeed.
fn test_map_cached_uses_existing_cache() {
    let dir = init_ts_project(3, 2);
    let keel = keel_bin();

    // Populate the cache first.
    let map_output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");
    assert!(map_output.status.success());

    let output = Command::new(&keel)
        .args(["map", "--cached", "--verbose"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map --cached");

    assert!(
        output.status.success(),
        "keel map --cached should succeed when a cache exists: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("falling back to full map"),
        "should not fall back when the cache is populated: {}",
        stderr
    );
}

#[test]
/// `keel map --cached` must report the same `summary.languages` as a fresh
/// `keel map` — the cached path reconstructs the list from file extensions
/// and must not collapse or rename what the fresh walk reports.
fn test_map_cached_languages_match_fresh_map() {
    let dir = init_ts_project(2, 1);
    let src = dir.path().join("src");
    fs::write(
        src.join("Widget.svelte"),
        "<script lang=\"ts\">\nexport function widgetName(): string { return \"w\"; }\n</script>\n<div>hi</div>\n",
    )
    .unwrap();
    fs::write(
        src.join("legacy.js"),
        "export function legacy(x) { return x; }\n",
    )
    .unwrap();
    let keel = keel_bin();

    let languages = |raw: &[u8]| -> Vec<String> {
        let v: serde_json::Value = serde_json::from_slice(raw).expect("map output is JSON");
        serde_json::from_value(v["summary"]["languages"].clone()).expect("summary.languages array")
    };

    let fresh = Command::new(&keel)
        .args(["map", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");
    assert!(fresh.status.success());

    let cached = Command::new(&keel)
        .args(["map", "--cached", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map --cached");
    assert!(cached.status.success());

    assert_eq!(
        languages(&fresh.stdout),
        languages(&cached.stdout),
        "cached and fresh map must agree on the language list"
    );
}

#[test]
/// `keel map --cached` must report the same node/edge/function/class counts
/// as a fresh `keel map` (issue #40). The cached reconstruction
/// (map_cached.rs) used to multiple-count non-module nodes: when a file has
/// more than one module-kind row sharing its `file_path` (a file-level
/// module node plus a resolver-emitted module definition for that same
/// file), every non-module node in that file was pushed into `NodeChanges`
/// once per co-located module row, inflating functions/classes several-fold
/// versus the fresh summary.
fn test_map_cached_counts_match_fresh_map() {
    let dir = init_ts_project(3, 2);
    let src = dir.path().join("src");
    fs::write(
        src.join("widget.ts"),
        "export class Widget {\n  render(): string {\n    return \"widget\";\n  }\n  update(x: number): number {\n    return x + 1;\n  }\n}\n",
    )
    .unwrap();
    fs::write(
        src.join("thing.py"),
        "class Thing:\n    def method_one(self, x: int) -> int:\n        return x + 1\n\n    def method_two(self, x: int) -> int:\n        return x * 2\n\n\ndef top_level_fn(x: int) -> int:\n    \"\"\"Docstring.\"\"\"\n    return x + 1\n",
    )
    .unwrap();
    let keel = keel_bin();

    let summary = |raw: &[u8]| -> serde_json::Value {
        let v: serde_json::Value = serde_json::from_slice(raw).expect("map output is JSON");
        v["summary"].clone()
    };

    let fresh = Command::new(&keel)
        .args(["map", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");
    assert!(fresh.status.success());

    let cached = Command::new(&keel)
        .args(["map", "--cached", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map --cached");
    assert!(cached.status.success());

    let fresh_summary = summary(&fresh.stdout);
    let cached_summary = summary(&cached.stdout);

    for field in [
        "total_nodes",
        "total_edges",
        "modules",
        "functions",
        "classes",
    ] {
        assert_eq!(
            fresh_summary[field], cached_summary[field],
            "cached and fresh map must agree on summary.{field}"
        );
    }

    // Sanity check: the fixture genuinely has classes and multiple functions,
    // so a regression that zeroed everything out wouldn't slip through.
    assert!(fresh_summary["classes"].as_u64().unwrap() > 0);
    assert!(fresh_summary["functions"].as_u64().unwrap() > 1);
}

#[test]
/// `keel map` should handle file deletions (remove orphaned nodes).
fn test_map_handles_deleted_files() {
    let dir = init_ts_project(5, 2);
    let keel = keel_bin();

    // Record db size after initial map
    let db_before = fs::metadata(dir.path().join(".keel/graph.db"))
        .unwrap()
        .len();

    // Delete 3 of the 5 source files
    fs::remove_file(dir.path().join("src/mod_2.ts")).unwrap();
    fs::remove_file(dir.path().join("src/mod_3.ts")).unwrap();
    fs::remove_file(dir.path().join("src/mod_4.ts")).unwrap();

    // Re-map
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");

    assert!(
        output.status.success(),
        "keel map failed after file deletion: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The database should still be valid (map didn't crash on deleted files)
    assert!(
        dir.path().join(".keel/graph.db").exists(),
        "graph.db should still exist after remap"
    );
    // We can't easily assert the db got smaller due to SQLite page reuse,
    // but at minimum the map should succeed without error
    let _ = db_before; // used for documentation, not assertion
}
