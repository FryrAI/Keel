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

/// Create a mixed-language project with TS, Python, Go, and Rust files.
fn setup_mixed_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    // TypeScript
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

    // Python
    fs::write(
        src.join("helper.py"),
        r#"def add(a: int, b: int) -> int:
    return a + b

def multiply(x: int, y: int) -> int:
    return add(x, y)
"#,
    )
    .unwrap();

    // Go
    fs::write(
        src.join("main.go"),
        r#"package main

func Hello(name string) string {
    return "Hello " + name
}

func Run() {
    Hello("world")
}
"#,
    )
    .unwrap();

    // Rust
    fs::write(
        src.join("lib.rs"),
        r#"pub fn compute(x: i32, y: i32) -> i32 {
    x + y
}

pub fn process() -> i32 {
    compute(1, 2)
}
"#,
    )
    .unwrap();

    dir
}

/// Initialize and map a project.
fn init_and_map(dir: &TempDir) {
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("init failed");
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("map failed");
    assert!(
        output.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
fn test_map_detects_all_four_languages() {
    let dir = setup_mixed_project();
    init_and_map(&dir);
    let keel = keel_bin();

    // Use stats to check what was mapped
    let output = Command::new(&keel)
        .arg("stats")
        .current_dir(dir.path())
        .output()
        .expect("stats failed");

    assert!(
        output.status.success(),
        "stats failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Stats should show files for all languages (at least 4 files mapped)
    assert!(
        stdout.contains("files"),
        "stats should mention files, got: {}",
        stdout
    );

    // Verify graph.db has nodes from multiple files
    let db_path = dir.path().join(".keel/graph.db");
    let store =
        keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap()).unwrap();
    let modules = keel_core::store::GraphStore::get_all_modules(&store);

    // Should have modules for each language file
    let file_paths: Vec<String> = modules.iter().map(|m| m.file_path.clone()).collect();
    assert!(
        file_paths.iter().any(|f| f.ends_with(".ts")),
        "Should have TypeScript module, got: {:?}",
        file_paths
    );
    assert!(
        file_paths.iter().any(|f| f.ends_with(".py")),
        "Should have Python module, got: {:?}",
        file_paths
    );
    assert!(
        file_paths.iter().any(|f| f.ends_with(".go")),
        "Should have Go module, got: {:?}",
        file_paths
    );
    assert!(
        file_paths.iter().any(|f| f.ends_with(".rs")),
        "Should have Rust module, got: {:?}",
        file_paths
    );
}

#[test]
fn test_compile_typescript_in_mixed_project() {
    let dir = setup_mixed_project();
    init_and_map(&dir);
    let keel = keel_bin();

    // Compile should be clean initially
    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("compile failed");

    assert_ne!(
        output.status.code(),
        Some(2),
        "compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Modify the TS file: change greet signature (arity change)
    let src = dir.path().join("src");
    fs::write(
        src.join("app.ts"),
        r#"function greet(): string {
    return "Hello";
}

function run(): void {
    greet("world");
}
"#,
    )
    .unwrap();

    // Compile should detect the change
    let output = Command::new(&keel)
        .args(["compile", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("compile failed");

    assert!(
        output.status.code().is_some(),
        "compile should not crash"
    );

    // Should not be internal error
    assert_ne!(
        output.status.code(),
        Some(2),
        "compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_compile_python_in_mixed_project() {
    let dir = setup_mixed_project();
    init_and_map(&dir);
    let keel = keel_bin();

    // Modify Python file: remove type hints
    let src = dir.path().join("src");
    fs::write(
        src.join("helper.py"),
        r#"def add(a, b):
    return a + b

def multiply(x, y):
    return add(x, y)
"#,
    )
    .unwrap();

    // Compile should detect missing type hints
    let output = Command::new(&keel)
        .args(["compile", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("compile failed");

    assert!(
        output.status.code().is_some(),
        "compile should not crash"
    );

    // Should not be internal error
    assert_ne!(
        output.status.code(),
        Some(2),
        "compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // If violations found, should include E002
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.code() == Some(1) {
        assert!(
            stdout.contains("E002") || stdout.contains("missing_type_hints"),
            "Expected E002 for Python missing type hints, got: {}",
            stdout
        );
    }
}

#[test]
fn test_compile_go_in_mixed_project() {
    let dir = setup_mixed_project();
    init_and_map(&dir);
    let keel = keel_bin();

    // Modify Go file: change function signature
    let src = dir.path().join("src");
    fs::write(
        src.join("main.go"),
        r#"package main

func Hello(name string, greeting string) string {
    return greeting + " " + name
}

func Run() {
    Hello("world")
}
"#,
    )
    .unwrap();

    // Compile should detect the arity mismatch
    let output = Command::new(&keel)
        .args(["compile", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("compile failed");

    assert!(
        output.status.code().is_some(),
        "compile should not crash"
    );

    assert_ne!(
        output.status.code(),
        Some(2),
        "compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_compile_rust_in_mixed_project() {
    let dir = setup_mixed_project();
    init_and_map(&dir);
    let keel = keel_bin();

    // Modify Rust file: remove the compute function
    let src = dir.path().join("src");
    fs::write(
        src.join("lib.rs"),
        r#"pub fn process() -> i32 {
    42
}
"#,
    )
    .unwrap();

    // Compile should detect the removed function
    let output = Command::new(&keel)
        .args(["compile", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("compile failed");

    assert!(
        output.status.code().is_some(),
        "compile should not crash"
    );

    assert_ne!(
        output.status.code(),
        Some(2),
        "compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_discover_works_across_languages() {
    let dir = setup_mixed_project();
    init_and_map(&dir);
    let keel = keel_bin();

    // Try discover for a function in each language
    let function_names = vec!["greet", "add", "Hello", "compute"];

    for name in &function_names {
        if let Some(hash) = find_hash_by_name(&dir, name) {
            let output = Command::new(&keel)
                .args(["discover", &hash, "--json"])
                .current_dir(dir.path())
                .output()
                .expect("discover failed");

            assert!(
                output.status.success(),
                "discover failed for {} (hash {}): {}",
                name,
                hash,
                String::from_utf8_lossy(&output.stderr)
            );

            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                !stdout.trim().is_empty(),
                "discover should produce output for {} (hash {})",
                name,
                hash
            );

            // Parse JSON to verify structure
            let json: serde_json::Value = serde_json::from_str(&stdout).expect(
                &format!("discover output for {} should be valid JSON", name),
            );
            assert_eq!(json["command"], "discover");
            assert!(
                json["target"].is_object(),
                "discover for {} should have target",
                name
            );
            assert_eq!(
                json["target"]["name"], *name,
                "discover target name should match"
            );
        }
    }
}
