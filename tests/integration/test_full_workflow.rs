// Integration tests: full command workflow (E2E)
//
// Validates the complete lifecycle of keel commands from init through deinit,
// exercising every major command in sequence.

use std::fs;
use std::process::Command;

use tempfile::TempDir;

/// Path to the keel binary built by cargo.
fn keel_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // remove test binary name
    path.pop(); // remove 'deps'
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

/// Create a project with caller/callee relationships for testing.
fn setup_project_with_calls() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    // File with two functions where run() calls greet()
    fs::write(
        src.join("app.ts"),
        r#"function greet(name: string): string {
    return "Hello " + name;
}

function run(): void {
    greet("world");
}
"#,
    )
    .unwrap();

    dir
}

/// Initialize and map a project, returning the TempDir.
fn init_and_map(dir: &TempDir) {
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("keel init failed");
    assert!(output.status.success(), "init failed: {:?}", output.stderr);

    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("keel map failed");
    assert!(output.status.success(), "map failed: {:?}", output.stderr);
}

/// Query the graph database to find a function node's hash by name.
fn find_hash_by_name(dir: &TempDir, name: &str) -> Option<String> {
    let db_path = dir.path().join(".keel/graph.db");
    let store =
        keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap()).ok()?;
    let modules = keel_core::store::GraphStore::get_all_modules(&store);
    for module in &modules {
        let nodes =
            keel_core::store::GraphStore::get_nodes_in_file(&store, &module.file_path);
        for node in &nodes {
            if node.name == name && node.kind == keel_core::types::NodeKind::Function {
                return Some(node.hash.clone());
            }
        }
    }
    None
}

#[test]
fn test_full_lifecycle_init_map_compile_deinit() {
    let dir = setup_project_with_calls();
    let keel = keel_bin();

    // Init
    let out = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "init failed");
    assert!(dir.path().join(".keel").exists());

    // Map
    let out = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "map failed");

    // Compile
    let out = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "compile should be clean");

    // Deinit
    let out = Command::new(&keel)
        .arg("deinit")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "deinit failed");
    assert!(
        !dir.path().join(".keel").exists(),
        ".keel/ should be removed after deinit"
    );
}

#[test]
fn test_discover_returns_valid_adjacency_after_map() {
    let dir = setup_project_with_calls();
    init_and_map(&dir);
    let keel = keel_bin();

    // Find the hash for the "greet" function (which is called by "run")
    let hash = find_hash_by_name(&dir, "greet")
        .expect("greet function should be in graph after map");

    // Discover
    let output = Command::new(&keel)
        .args(["discover", &hash, "--json"])
        .current_dir(dir.path())
        .output()
        .expect("keel discover failed");

    assert!(
        output.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.trim().is_empty(),
        "discover should produce output for known hash"
    );

    // Parse JSON and check structure
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("discover output should be valid JSON");
    assert_eq!(json["command"], "discover");
    assert!(json["target"].is_object(), "should have target node info");
    assert_eq!(json["target"]["name"], "greet");
}

#[test]
fn test_where_resolves_hash_to_file_and_line() {
    let dir = setup_project_with_calls();
    init_and_map(&dir);
    let keel = keel_bin();

    // Find the hash for "greet"
    let hash = find_hash_by_name(&dir, "greet")
        .expect("greet function should be in graph after map");

    // Where
    let output = Command::new(&keel)
        .args(["where", &hash])
        .current_dir(dir.path())
        .output()
        .expect("keel where failed");

    assert!(
        output.status.success(),
        "where failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Output should contain file path and line number (format: "path:line")
    assert!(
        stdout.contains("app.ts"),
        "where output should contain file path, got: {}",
        stdout
    );
    assert!(
        stdout.contains(':'),
        "where output should be in file:line format, got: {}",
        stdout
    );
}

#[test]
fn test_explain_shows_resolution_chain() {
    let dir = setup_project_with_calls();
    init_and_map(&dir);
    let keel = keel_bin();

    // Find the hash for "greet"
    let hash = find_hash_by_name(&dir, "greet")
        .expect("greet function should be in graph after map");

    // Explain
    let output = Command::new(&keel)
        .args(["explain", "E001", &hash, "--json"])
        .current_dir(dir.path())
        .output()
        .expect("keel explain failed");

    assert!(
        output.status.success(),
        "explain failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("explain output should be valid JSON");
    assert_eq!(json["command"], "explain");
    assert_eq!(json["error_code"], "E001");
    assert!(
        json["confidence"].is_number(),
        "should have confidence score"
    );
    assert!(
        json["resolution_tier"].is_string(),
        "should have resolution_tier"
    );
}

#[test]
fn test_stats_shows_graph_summary() {
    let dir = setup_project_with_calls();
    init_and_map(&dir);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("stats")
        .current_dir(dir.path())
        .output()
        .expect("keel stats failed");

    assert!(
        output.status.success(),
        "stats failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Stats should mention modules, functions, and files
    assert!(
        stdout.contains("modules"),
        "stats should show module count, got: {}",
        stdout
    );
    assert!(
        stdout.contains("functions"),
        "stats should show function count, got: {}",
        stdout
    );
    assert!(
        stdout.contains("files"),
        "stats should show file count, got: {}",
        stdout
    );
}

#[test]
fn test_deinit_removes_all_keel_artifacts() {
    let dir = setup_project_with_calls();
    init_and_map(&dir);
    let keel = keel_bin();

    // Verify .keel/ exists before deinit
    assert!(dir.path().join(".keel").exists());
    assert!(dir.path().join(".keel/graph.db").exists());
    assert!(dir.path().join(".keel/keel.json").exists());

    // Deinit
    let output = Command::new(&keel)
        .arg("deinit")
        .current_dir(dir.path())
        .output()
        .expect("keel deinit failed");

    assert!(
        output.status.success(),
        "deinit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify all artifacts removed
    assert!(
        !dir.path().join(".keel").exists(),
        ".keel/ directory should be removed"
    );
    assert!(
        !dir.path().join(".keel/graph.db").exists(),
        "graph.db should be removed"
    );
    assert!(
        !dir.path().join(".keel/keel.json").exists(),
        "keel.json should be removed"
    );

    // Source files should still exist
    assert!(
        dir.path().join("src/app.ts").exists(),
        "Source files should not be removed by deinit"
    );
}
