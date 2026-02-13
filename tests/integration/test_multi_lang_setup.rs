/// Shared setup helpers for multi-language integration tests.
///
/// Provides the keel binary path, a mixed-language project fixture,
/// init+map orchestration, and hash lookup by function name.

use std::fs;
use std::process::Command;

use tempfile::TempDir;

/// Path to the keel binary built by cargo.
pub fn keel_bin() -> std::path::PathBuf {
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
pub fn setup_mixed_project() -> TempDir {
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
pub fn init_and_map(dir: &TempDir) {
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
pub fn find_hash_by_name(dir: &TempDir, name: &str) -> Option<String> {
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
