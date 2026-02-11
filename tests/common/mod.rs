/// Shared test helpers for all keel integration tests.
///
/// Import from any integration test file with:
///   `#[path = "common/mod.rs"] mod common;`
pub mod generators;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Set up a test project directory with a sample source file.
///
/// Returns (TempDir, project_root). Hold the TempDir to keep the directory alive.
#[allow(dead_code)]
pub fn setup_test_project(lang: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    match lang {
        "typescript" | "ts" => {
            fs::write(
                src.join("index.ts"),
                "function hello(name: string): string { return name; }\n",
            )
            .unwrap();
        }
        "python" | "py" => {
            fs::write(
                src.join("main.py"),
                "def hello(name: str) -> str:\n    return name\n",
            )
            .unwrap();
        }
        "go" => {
            fs::write(
                src.join("main.go"),
                "package main\n\nfunc hello(name string) string {\n\treturn name\n}\n",
            )
            .unwrap();
        }
        "rust" | "rs" => {
            fs::write(
                src.join("lib.rs"),
                "pub fn hello(name: &str) -> String {\n    name.to_string()\n}\n",
            )
            .unwrap();
        }
        _ => panic!("Unsupported language: {}", lang),
    }

    let project_root = dir.path().to_path_buf();
    (dir, project_root)
}

/// Get path to compiled keel binary.
///
/// Builds the binary if it doesn't exist yet.
#[allow(dead_code)]
pub fn keel_bin() -> PathBuf {
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

/// Create a mapped project from a set of source files.
///
/// Each entry in `files` is `(relative_path, content)`.
/// Returns (TempDir, project_root). Hold the TempDir to keep the directory alive.
#[allow(dead_code)]
pub fn create_mapped_project(files: &[(&str, &str)]) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    for (path, content) in files {
        let full_path = root.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full_path, content).unwrap();
    }

    let project_root = root.to_path_buf();
    (dir, project_root)
}

/// Create an in-memory SqliteGraphStore for testing.
#[allow(dead_code)]
pub fn in_memory_store() -> keel_core::sqlite::SqliteGraphStore {
    keel_core::sqlite::SqliteGraphStore::in_memory()
        .expect("Failed to create in-memory SqliteGraphStore")
}
