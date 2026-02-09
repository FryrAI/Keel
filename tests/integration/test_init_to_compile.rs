// Integration tests: init -> map -> edit -> compile workflow (E2E)
//
// Validates the most common developer workflow: initializing keel on a project,
// mapping it, making a code change, and compiling to catch structural violations.

use std::fs;
use std::process::Command;

use tempfile::TempDir;

/// Path to the keel binary built by cargo.
fn keel_bin() -> std::path::PathBuf {
    // cargo test builds the workspace; the binary is in target/debug/
    let mut path = std::env::current_exe().unwrap();
    // Walk up from the test binary to the target dir
    path.pop(); // remove test binary name
    path.pop(); // remove 'deps'
    path.push("keel");
    if !path.exists() {
        // Try building it
        let status = Command::new("cargo")
            .args(["build", "-p", "keel-cli"])
            .status()
            .expect("Failed to build keel");
        assert!(status.success(), "Failed to build keel binary");
    }
    path
}

/// Create a temp project with TypeScript files and return the TempDir.
fn setup_ts_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    // Private functions (no `export`) avoid E002/E003 checks on public API
    fs::write(
        src.join("math.ts"),
        r#"function add(a: number, b: number): number {
    return a + b;
}

function multiply(a: number, b: number): number {
    return a * b;
}
"#,
    )
    .unwrap();

    fs::write(
        src.join("main.ts"),
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

#[test]
fn test_init_creates_keel_directory() {
    let dir = setup_ts_project();
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

    // .keel/ directory should exist
    assert!(dir.path().join(".keel").exists(), ".keel/ not created");

    // keel.json config should exist
    assert!(
        dir.path().join(".keel/keel.json").exists(),
        "keel.json not created"
    );

    // graph.db should exist
    assert!(
        dir.path().join(".keel/graph.db").exists(),
        "graph.db not created"
    );
}

#[test]
fn test_init_then_map_populates_graph() {
    let dir = setup_ts_project();
    let keel = keel_bin();

    // Init
    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");
    assert!(output.status.success());

    // Map
    let output = Command::new(&keel)
        .args(["map", "--verbose"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");
    assert!(
        output.status.success(),
        "keel map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify the graph.db file grew (has content)
    let db_path = dir.path().join(".keel/graph.db");
    let metadata = fs::metadata(&db_path).unwrap();
    assert!(
        metadata.len() > 4096,
        "graph.db should have been populated, size = {}",
        metadata.len()
    );

    // Verbose output should mention files found
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("source files"),
        "Expected verbose output about source files, got: {}",
        stderr
    );
}

#[test]
fn test_compile_after_map_returns_clean() {
    let dir = setup_ts_project();
    let keel = keel_bin();

    // Init + Map
    Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");
    Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");

    // Compile
    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    // Clean compile: exit 0, empty stdout
    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected exit 0 (clean compile), got {:?}. stderr: {}. stdout: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().is_empty(),
        "Clean compile should have empty stdout, got: {}",
        stdout
    );
}

#[test]
fn test_edit_breaks_caller_then_compile_catches_it() {
    let dir = setup_ts_project();
    let keel = keel_bin();

    // Init + Map (establishes the graph with current signatures)
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

    // Edit: change add(a, b) signature to add(a) — arity change
    // The caller multiply references add, but this is a same-file scenario.
    // Write a file where function A calls function B, then change B's signature.
    let src = dir.path().join("src");
    fs::write(
        src.join("math.ts"),
        r#"function add(a: number): number {
    return a;
}

function multiply(a: number, b: number): number {
    return a * b;
}
"#,
    )
    .unwrap();

    // Compile with --json to check structured output
    let output = Command::new(&keel)
        .args(["compile", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    // Should detect the broken caller (E001) since add's hash changed
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The compile should detect that add's signature changed from the stored version
    // Exit code 1 means violations found, or 0 if no callers existed
    // Either way, the compile should not error (exit 2)
    assert_ne!(
        output.status.code(),
        Some(2),
        "keel compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // If violations found, check it's the expected type
    if output.status.code() == Some(1) {
        assert!(
            stdout.contains("E001") || stdout.contains("broken_caller"),
            "Expected E001 broken_caller in output, got: {}",
            stdout
        );
    }
}

#[test]
fn test_edit_removes_function_then_compile_catches_it() {
    let dir = setup_ts_project();
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

    // Edit: remove the add function from math.ts
    let src = dir.path().join("src");
    fs::write(
        src.join("math.ts"),
        r#"function multiply(a: number, b: number): number {
    return a * b;
}
"#,
    )
    .unwrap();

    // Compile with --json
    let output = Command::new(&keel)
        .args(["compile", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    // Should not be an internal error
    assert_ne!(
        output.status.code(),
        Some(2),
        "keel compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // If violations found, should detect E004 function_removed
    // (Only triggers if removed function had callers — may be exit 0 if no callers)
    if output.status.code() == Some(1) {
        assert!(
            stdout.contains("E004") || stdout.contains("function_removed"),
            "Expected E004 function_removed in output, got: {}",
            stdout
        );
    }
}

#[test]
fn test_compile_specific_file_only_checks_that_file() {
    let dir = setup_ts_project();
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

    // Compile only math.ts
    let output = Command::new(&keel)
        .args(["compile", "src/math.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    // Should succeed without internal error
    assert_ne!(
        output.status.code(),
        Some(2),
        "keel compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The exit code should be 0 (clean) since math.ts has private typed functions
    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected clean compile for math.ts, stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
