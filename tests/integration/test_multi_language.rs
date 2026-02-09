// Integration tests: multi-language project support (E2E)
//
// Validates that keel correctly handles projects containing a mix of
// TypeScript, Python, Go, and Rust source files simultaneously.

use std::fs;
use std::process::Command;

use tempfile::TempDir;

/// Path to the keel binary built by cargo.
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

/// Create a mixed-language project with .ts, .py, .go, and .rs files.
/// All functions are private/non-exported to avoid E002/E003 checks.
fn setup_mixed_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    // TypeScript file
    fs::write(
        src.join("math.ts"),
        r#"function add(a: number, b: number): number {
    return a + b;
}
"#,
    )
    .unwrap();

    // Python file
    fs::write(
        src.join("utils.py"),
        r#"def greet(name: str) -> str:
    return f"Hello {name}"
"#,
    )
    .unwrap();

    // Go file (needs package declaration)
    fs::write(
        src.join("helper.go"),
        r#"package src

func multiply(a int, b int) int {
	return a * b
}
"#,
    )
    .unwrap();

    // Rust file
    fs::write(
        src.join("lib.rs"),
        r#"fn divide(a: f64, b: f64) -> f64 {
    a / b
}
"#,
    )
    .unwrap();

    dir
}

/// Init and map the project, asserting success.
fn init_and_map(dir: &TempDir) {
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("keel init failed");
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("keel map failed");
    assert!(
        output.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Find a function hash by name in the graph DB.
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
fn test_map_detects_all_four_languages() {
    let dir = setup_mixed_project();
    init_and_map(&dir);

    // Open the graph DB and check for nodes from each language
    let db_path = dir.path().join(".keel/graph.db");
    let store =
        keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap())
            .expect("should open graph.db");

    let modules = keel_core::store::GraphStore::get_all_modules(&store);

    // Collect all nodes across all modules
    let mut all_nodes = Vec::new();
    for module in &modules {
        let nodes =
            keel_core::store::GraphStore::get_nodes_in_file(&store, &module.file_path);
        all_nodes.extend(nodes);
    }

    // Check for nodes from each language by file extension
    let file_paths: Vec<&str> = all_nodes.iter().map(|n| n.file_path.as_str()).collect();
    let has_ts = file_paths.iter().any(|p| p.ends_with(".ts"));
    let has_py = file_paths.iter().any(|p| p.ends_with(".py"));
    let has_go = file_paths.iter().any(|p| p.ends_with(".go"));
    let has_rs = file_paths.iter().any(|p| p.ends_with(".rs"));

    assert!(
        has_ts,
        "graph should contain TypeScript nodes, found paths: {:?}",
        file_paths
    );
    assert!(
        has_py,
        "graph should contain Python nodes, found paths: {:?}",
        file_paths
    );
    assert!(
        has_go,
        "graph should contain Go nodes, found paths: {:?}",
        file_paths
    );
    assert!(
        has_rs,
        "graph should contain Rust nodes, found paths: {:?}",
        file_paths
    );

    // Verify specific function names from each language
    let names: Vec<&str> = all_nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"add"), "should find TS add function");
    assert!(names.contains(&"greet"), "should find Python greet function");
    assert!(
        names.contains(&"multiply"),
        "should find Go multiply function"
    );
    assert!(names.contains(&"divide"), "should find Rust divide function");
}

#[test]
fn test_discover_works_across_languages() {
    let dir = setup_mixed_project();
    init_and_map(&dir);
    let keel = keel_bin();

    // Try discovering each language's function by hash
    let functions = ["add", "greet", "multiply", "divide"];
    let langs = ["TypeScript", "Python", "Go", "Rust"];

    for (func, lang) in functions.iter().zip(langs.iter()) {
        let hash = find_hash_by_name(&dir, func);
        assert!(
            hash.is_some(),
            "{lang} function '{func}' should be in graph"
        );
        let hash = hash.unwrap();

        let output = Command::new(&keel)
            .args(["discover", &hash, "--json"])
            .current_dir(dir.path())
            .output()
            .unwrap_or_else(|_| panic!("keel discover failed for {lang} {func}"));

        assert!(
            output.status.success(),
            "discover failed for {lang} {func}: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.trim().is_empty(),
            "discover should produce output for {lang} {func}"
        );

        let json: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|_| panic!("discover output for {lang} {func} should be valid JSON"));
        assert_eq!(
            json["command"], "discover",
            "discover output should have command field"
        );
        assert!(
            json["target"].is_object(),
            "discover output should have target for {lang} {func}"
        );
        assert_eq!(
            json["target"]["name"], *func,
            "target name should be {func}"
        );
    }
}

#[test]
#[ignore = "Not yet implemented"]
fn test_compile_typescript_in_mixed_project() {
    // GIVEN a mixed-language project that has been mapped
    // WHEN a TypeScript file is modified to break a caller and `keel compile` is run
    // THEN the violation is detected correctly using Oxc resolution (Tier 2)
}

#[test]
#[ignore = "Not yet implemented"]
fn test_compile_python_in_mixed_project() {
    // GIVEN a mixed-language project that has been mapped
    // WHEN a Python file is modified to remove type hints and `keel compile` is run
    // THEN the E002 missing_type_hints violation is detected using ty resolution
}

#[test]
#[ignore = "Not yet implemented"]
fn test_compile_go_in_mixed_project() {
    // GIVEN a mixed-language project that has been mapped
    // WHEN a Go file is modified to break a function signature and `keel compile` is run
    // THEN the E005 arity_mismatch violation is detected using tree-sitter heuristics
}

#[test]
#[ignore = "Not yet implemented"]
fn test_compile_rust_in_mixed_project() {
    // GIVEN a mixed-language project that has been mapped
    // WHEN a Rust file is modified to remove a public function and `keel compile` is run
    // THEN the E004 function_removed violation is detected
}
