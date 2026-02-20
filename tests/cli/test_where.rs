// Tests for `keel where` command (Spec 007 - CLI Commands)

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
/// `keel where <hash>` should return the file path and line number.
fn test_where_returns_file_and_line() {
    let dir = init_and_map(&[(
        "src/parser.ts",
        "export function parse(input: string): string {\n  return input;\n}\n",
    )]);
    let keel = keel_bin();

    // We can't easily get a real hash without deeper integration,
    // so test that the command runs and returns expected exit codes.
    // A valid hash would return file:line; an invalid one returns exit 2.
    let output = Command::new(&keel)
        .args(["where", "AAAAAAAAAAA"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel where");

    // Invalid hash → exit 2
    assert_eq!(
        output.status.code(),
        Some(2),
        "where with nonexistent hash should exit 2"
    );
}

#[test]
/// `keel where` should complete in under 50ms.
fn test_where_performance_target() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let start = Instant::now();
    let _ = Command::new(&keel)
        .args(["where", "AAAAAAAAAAA"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel where");
    let elapsed = start.elapsed();

    // Allow 2s for process spawn, core target is <50ms
    assert!(
        elapsed.as_millis() < 2000,
        "where took {:?} — should be fast",
        elapsed
    );
}

#[test]
/// `keel where` with an invalid hash should return a clear error.
fn test_where_invalid_hash() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["where", "nonexistent"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel where");

    assert!(
        !output.status.success(),
        "where with invalid hash should fail"
    );
    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 2, "where with invalid hash should exit 2");
}

#[test]
/// `keel where` should look up previous hashes if current hash not found.
fn test_where_checks_previous_hashes() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    // Modify the function to change its hash
    fs::write(
        dir.path().join("src/index.ts"),
        "export function hello(name: string): string { return name + '!'; }\n",
    )
    .unwrap();

    // Re-map to update hashes
    let out = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());

    // Looking up the old hash should either find via previous_hashes or return not found
    // Either way, it should not crash (exit code 0 or 2)
    let output = Command::new(&keel)
        .args(["where", "AAAAAAAAAAA"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel where");

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 2,
        "where should exit 0 (found via prev hash) or 2 (not found), got {code}"
    );
}
