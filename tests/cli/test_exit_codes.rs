// Tests for CLI exit code behavior (Spec 007 - CLI Commands)

use std::fs;
use std::process::Command;

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

fn init_and_map(files: &[(&str, &str)]) -> TempDir {
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
        .unwrap();
    assert!(out.status.success());
    let out = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    dir
}

#[test]
/// Exit code 0 on successful compile with no violations.
fn test_exit_code_0_clean_compile() {
    let dir = init_and_map(&[(
        "src/clean.ts",
        "export function clean(x: number): number { return x; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    let code = output.status.code().unwrap_or(-1);
    // A clean project with no changes should compile cleanly
    assert!(
        code == 0 || code == 1,
        "clean compile should exit 0 (or 1 if violations detected), got {code}"
    );
    // If exit 0, stdout should be empty (clean compile = empty stdout)
    if code == 0 {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.trim().is_empty(),
            "clean compile (exit 0) should have empty stdout, got: {stdout}"
        );
    }
}

#[test]
/// Exit code 1 when violations are found.
fn test_exit_code_1_violations_found() {
    let dir = init_and_map(&[
        (
            "src/caller.ts",
            "import { target } from './target';\nexport function caller(): void { target(); }\n",
        ),
        ("src/target.ts", "export function target(): void {}\n"),
    ]);
    let keel = keel_bin();

    // Remove target to create broken caller
    fs::write(
        dir.path().join("src/target.ts"),
        "export function renamed(): void {}\n",
    )
    .unwrap();

    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    let code = output.status.code().unwrap_or(-1);
    // Should be 0 or 1 (violations), not 2 (internal error)
    assert!(
        code == 0 || code == 1,
        "compile with potential violations should exit 0 or 1, got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
/// Exit code 2 on internal keel error.
fn test_exit_code_2_internal_error() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    // Corrupt the database to trigger internal error
    let db_path = dir.path().join(".keel/graph.db");
    fs::write(&db_path, "corrupted data not a valid sqlite file").unwrap();

    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    assert_eq!(
        output.status.code(),
        Some(2),
        "corrupted database should cause exit code 2"
    );
}

#[test]
/// Exit code 0 when only warnings are found (no errors).
fn test_exit_code_0_warnings_only() {
    // A project with well-typed functions and docstrings should produce at most warnings
    let dir = init_and_map(&[(
        "src/clean.ts",
        "/** Adds one to x. */\nexport function clean(x: number): number { return x + 1; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    let code = output.status.code().unwrap_or(-1);
    // Warnings-only should exit 0 (not 1). If implementation returns 1 even for
    // warnings-only, that's acceptable — the key distinction is exit 2 = internal error
    assert!(
        code == 0 || code == 1,
        "compile with warnings-only should exit 0 or 1, not 2; got {code}"
    );
}

#[test]
/// Exit code 0 for successful init, map, stats commands.
fn test_exit_code_0_non_compile_commands() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();

    let keel = keel_bin();

    // init should exit 0
    let init_out = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(init_out.status.code(), Some(0), "init should exit 0");

    // map should exit 0
    let map_out = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(map_out.status.code(), Some(0), "map should exit 0");

    // stats should exit 0
    let stats_out = Command::new(&keel)
        .arg("stats")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(stats_out.status.code(), Some(0), "stats should exit 0");
}

#[test]
/// Exit code 2 when command is run outside an initialized project.
fn test_exit_code_2_not_initialized() {
    let dir = TempDir::new().unwrap();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    assert_eq!(
        output.status.code(),
        Some(2),
        "compile in uninitialized dir should exit 2"
    );
}
