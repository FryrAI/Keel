// Integration tests: large codebase scaling (E2E)
//
// Validates that keel handles large generated codebases within documented
// performance targets. Uses code generators to create realistic test repos.
//
// These are gated behind the `perf-tests` feature because they generate
// 50-100k LOC and are slow in debug builds. Run with:
//   cargo test --features perf-tests --release

#[cfg(feature = "perf-tests")]
use std::fs;
#[cfg(feature = "perf-tests")]
use std::process::Command;
#[cfg(feature = "perf-tests")]
use std::time::Instant;

#[cfg(feature = "perf-tests")]
use tempfile::TempDir;

/// Path to the keel binary built by cargo.
#[cfg(feature = "perf-tests")]
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

/// Generate a project with approximately `target_loc` lines of TypeScript.
/// Creates files with ~100 LOC each containing typed functions.
#[cfg(feature = "perf-tests")]
fn generate_ts_project(target_loc: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    let lines_per_file = 100;
    let num_files = target_loc / lines_per_file;

    for i in 0..num_files {
        let subdir = src.join(format!("mod_{}", i / 50));
        fs::create_dir_all(&subdir).unwrap();

        let mut content = String::new();
        // ~10 functions per file, each ~10 lines
        for j in 0..10 {
            content.push_str(&format!(
                r#"function func_{i}_{j}(a: number, b: number): number {{
    const x = a + b;
    const y = x * 2;
    const z = y - 1;
    if (z > 0) {{
        return z;
    }}
    return x + y + z;
}}

"#,
                i = i,
                j = j
            ));
        }

        fs::write(subdir.join(format!("file_{}.ts", i)), &content).unwrap();
    }

    dir
}

#[test]
#[cfg(feature = "perf-tests")]
fn test_init_50k_loc_under_10s() {
    let dir = generate_ts_project(50_000);
    let keel = keel_bin();

    let start = Instant::now();
    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");

    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "keel init failed on 50k LOC project: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        elapsed.as_secs() < 10,
        "keel init on 50k LOC took {:.1}s (target: <10s)",
        elapsed.as_secs_f64()
    );
}

#[test]
#[cfg(feature = "perf-tests")]
fn test_map_100k_loc_under_5s() {
    let dir = generate_ts_project(100_000);
    let keel = keel_bin();

    // Init first
    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());

    // Time the map
    let start = Instant::now();
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");

    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "keel map failed on 100k LOC: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Debug builds are ~20x slower; target <5s for release, <120s for debug
    let limit_secs = if cfg!(debug_assertions) { 120 } else { 5 };
    assert!(
        elapsed.as_secs() < limit_secs,
        "keel map on 100k LOC took {:.1}s (target: <{}s)",
        elapsed.as_secs_f64(),
        limit_secs
    );
}

#[test]
#[cfg(feature = "perf-tests")]
fn test_compile_single_file_in_large_project_under_200ms() {
    let dir = generate_ts_project(100_000);
    let keel = keel_bin();

    // Init + Map
    Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Find a file to compile
    let src = dir.path().join("src/mod_0/file_0.ts");
    let rel_path = "src/mod_0/file_0.ts";
    assert!(src.exists(), "Test file should exist");

    // Time single-file compile
    let start = Instant::now();
    let output = Command::new(&keel)
        .args(["compile", rel_path])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    let elapsed = start.elapsed();

    assert_ne!(
        output.status.code(),
        Some(2),
        "compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Debug builds are ~50x slower
    let limit_ms: u128 = if cfg!(debug_assertions) { 15000 } else { 200 };
    assert!(
        elapsed.as_millis() < limit_ms,
        "Single-file compile took {}ms (target: <{}ms)",
        elapsed.as_millis(),
        limit_ms
    );
}

#[test]
#[cfg(feature = "perf-tests")]
fn test_discover_in_large_graph_under_50ms() {
    let dir = generate_ts_project(100_000);
    let keel = keel_bin();

    // Init + Map
    Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Find a hash from the graph
    let db_path = dir.path().join(".keel/graph.db");
    let store = keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap()).unwrap();
    let modules = keel_core::store::GraphStore::get_all_modules(&store);
    assert!(!modules.is_empty(), "Should have modules in graph");

    // Get a function node hash
    let mut hash = None;
    for module in &modules {
        let nodes = keel_core::store::GraphStore::get_nodes_in_file(&store, &module.file_path);
        for node in &nodes {
            if node.kind == keel_core::types::NodeKind::Function {
                hash = Some(node.hash.clone());
                break;
            }
        }
        if hash.is_some() {
            break;
        }
    }

    let hash = hash.expect("Should find at least one function in large graph");

    // Time discover
    let start = Instant::now();
    let output = Command::new(&keel)
        .args(["discover", &hash, "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel discover");

    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Debug builds are ~10x slower
    let limit_ms = if cfg!(debug_assertions) { 500 } else { 50 };
    assert!(
        elapsed.as_millis() < limit_ms,
        "Discover in large graph took {}ms (target: <{}ms)",
        elapsed.as_millis(),
        limit_ms
    );
}

#[test]
#[cfg(feature = "perf-tests")]
fn test_graph_db_size_reasonable_for_100k_loc() {
    let dir = generate_ts_project(100_000);
    let keel = keel_bin();

    // Init + Map
    Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let db_path = dir.path().join(".keel/graph.db");
    let db_size = fs::metadata(&db_path).unwrap().len();

    // 50MB limit
    let limit_bytes: u64 = 50 * 1024 * 1024;
    assert!(
        db_size < limit_bytes,
        "graph.db for 100k LOC is {}MB (target: <50MB)",
        db_size / (1024 * 1024)
    );

    // Should have substantial content
    assert!(
        db_size > 4096,
        "graph.db should have content, size = {}",
        db_size
    );
}
