/// Shared test helpers for all keel integration tests.
///
/// Import from any integration test file with:
///   `#[path = "common/mod.rs"] mod common;`
pub mod generators;

use std::fs;
use std::path::{Path, PathBuf};
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
/// Searches relative to the test executable first (standard `cargo test` layout),
/// then falls back to `target/debug/keel` in the workspace root (handles
/// `cargo llvm-cov` which uses a different `--target-dir`). Builds the binary
/// as a last resort.
#[allow(dead_code)]
pub fn keel_bin() -> PathBuf {
    // Try relative to test executable (works for normal cargo test)
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // remove test binary name
    path.pop(); // remove 'deps'
    path.push("keel");
    if path.exists() {
        return path;
    }

    // Fallback: workspace target/debug/keel (handles cargo-llvm-cov
    // which builds tests into target/llvm-cov-target/ while the pre-built
    // binary lives in target/debug/)
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fallback = workspace.join("target/debug/keel");
    if fallback.exists() {
        return fallback;
    }

    // Last resort: build the binary
    let status = Command::new("cargo")
        .args(["build", "-p", "keel-cli"])
        .current_dir(&workspace)
        .status()
        .expect("Failed to build keel");
    assert!(status.success(), "Failed to build keel binary");
    assert!(
        fallback.exists(),
        "keel binary not found at {}",
        fallback.display()
    );
    fallback
}

/// Run a git command in `dir` and assert it succeeded.
///
/// Identity and signing are forced on the command line so the suite works on a
/// machine with no git identity configured and never blocks on a GPG prompt.
/// Hooks are disabled (`core.hooksPath` pointed at a non-directory) because
/// fixtures that run `keel init` get a pre-commit hook that shells out to
/// `keel` from PATH — present on dev machines, absent on CI runners.
#[allow(dead_code)]
pub fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "user.email=test@keel.dev",
            "-c",
            "user.name=keel test",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git command failed to run");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Run a keel subcommand in `dir` and assert it did not hit an internal error.
#[allow(dead_code)]
pub fn keel(dir: &Path, args: &[&str]) -> std::process::Output {
    let out = Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to run `keel {}`: {e}", args.join(" ")));
    assert_ne!(
        out.status.code(),
        Some(2),
        "`keel {}` hit an internal error:\nstdout: {}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    out
}

/// Create a project dir with the given files and run `keel init`.
#[allow(dead_code)]
pub fn init_project(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (rel, content) in files {
        let full = dir.path().join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, content).unwrap();
    }
    let out = keel(dir.path(), &["init"]);
    assert!(
        out.status.success(),
        "keel init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

/// Create a project dir holding `files`, run `keel init` + `keel map`, and
/// return the live TempDir.
#[allow(dead_code)]
pub fn mapped_project(files: &[(&str, &str)]) -> TempDir {
    let dir = init_project(files);
    keel(dir.path(), &["map"]);
    dir
}

/// `keel compile <file> --json` -> parsed CompileResult value. A file with
/// zero errors AND zero warnings prints nothing (the documented clean-compile
/// contract), so empty stdout maps to an empty errors/warnings result rather
/// than a parse panic.
#[allow(dead_code)]
pub fn compile_json(dir: &Path, file: &str) -> serde_json::Value {
    let out = keel(dir, &["compile", file, "--json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return serde_json::json!({ "errors": [], "warnings": [] });
    }
    serde_json::from_str(trimmed).unwrap_or_else(|e| {
        panic!(
            "compile --json did not emit JSON ({e}):\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// All violations (errors + warnings) in `result` whose `code` matches.
#[allow(dead_code)]
pub fn violations_with_code(result: &serde_json::Value, code: &str) -> Vec<serde_json::Value> {
    result["errors"]
        .as_array()
        .into_iter()
        .chain(result["warnings"].as_array())
        .flatten()
        .filter(|v| v["code"] == code)
        .cloned()
        .collect()
}

/// Find the first violation with `code` across the errors + warnings arrays.
#[allow(dead_code)]
pub fn find_violation(result: &serde_json::Value, code: &str) -> serde_json::Value {
    violations_with_code(result, code)
        .into_iter()
        .next()
        .unwrap_or_else(|| {
            panic!(
                "expected a {code} violation, got:\nerrors: {}\nwarnings: {}",
                result["errors"], result["warnings"]
            )
        })
}

/// Assert no violation with `code` appears in the errors + warnings arrays.
#[allow(dead_code)]
pub fn assert_no_violation(result: &serde_json::Value, code: &str) {
    let hits = violations_with_code(result, code);
    assert!(
        hits.is_empty(),
        "expected no {code} violation, got:\nerrors: {}\nwarnings: {}",
        result["errors"],
        result["warnings"]
    );
}

/// Create an in-memory SqliteGraphStore for testing.
#[allow(dead_code)]
pub fn in_memory_store() -> keel_core::sqlite::SqliteGraphStore {
    keel_core::sqlite::SqliteGraphStore::in_memory()
        .expect("Failed to create in-memory SqliteGraphStore")
}
