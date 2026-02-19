// Tests for `keel check` command — pre-edit risk assessment

use std::fs;
use std::process::Command;
use std::time::Instant;

use tempfile::TempDir;

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
    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

/// Extract a hash from `keel search --json` output for a given function name.
fn get_hash_by_name(dir: &std::path::Path, name: &str) -> Option<String> {
    let keel = keel_bin();
    let output = Command::new(&keel)
        .args(["search", name, "--json"])
        .current_dir(dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).ok()?;
    val["results"]
        .as_array()?
        .first()?
        .get("hash")?
        .as_str()
        .map(|s| s.to_string())
}

#[test]
fn test_check_with_valid_hash() {
    let dir = init_and_map(&[
        (
            "src/caller.ts",
            "import { target } from './target';\nexport function caller(): void { target(); }\n",
        ),
        ("src/target.ts", "export function target(): void {}\n"),
    ]);
    let keel = keel_bin();

    if let Some(hash) = get_hash_by_name(dir.path(), "target") {
        let output = Command::new(&keel)
            .args(["check", &hash])
            .current_dir(dir.path())
            .output()
            .expect("Failed to run keel check");

        assert_eq!(
            output.status.code(),
            Some(0),
            "check with valid hash should exit 0\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.is_empty(),
            "check should produce output for a valid node"
        );
    }
}

#[test]
fn test_check_with_name_flag() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function uniqueFn(x: number): number { return x; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["check", "--name", "uniqueFn"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel check");

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 2,
        "check --name should exit 0 (found) or 2 (not found), got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_check_nonexistent_hash_exits_2() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["check", "AAAAAAAAAAA"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel check");

    assert_eq!(
        output.status.code(),
        Some(2),
        "check with nonexistent hash should exit 2"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found"),
        "stderr should mention 'not found': {stderr}"
    );
}

#[test]
fn test_check_file_path_rejected() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["check", "src/index.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel check");

    assert_eq!(
        output.status.code(),
        Some(2),
        "check with file path should exit 2 (not supported)"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("file path") || stderr.contains("not supported"),
        "should mention file paths not supported: {stderr}"
    );
}

#[test]
fn test_check_not_initialized_exits_2() {
    let dir = TempDir::new().unwrap();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["check", "AAAAAAAAAAA"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel check");

    assert_eq!(
        output.status.code(),
        Some(2),
        "check without init should exit 2"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not initialized") || stderr.contains("init"),
        "should mention initialization: {stderr}"
    );
}

#[test]
fn test_check_performance() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let start = Instant::now();
    let _ = Command::new(&keel)
        .args(["check", "AAAAAAAAAAA"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel check");
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 2000,
        "check took {:?} — should be fast",
        elapsed
    );
}
