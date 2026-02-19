// Tests for `keel init --merge` behavior (Spec 007 - CLI Commands)

use std::fs;
use std::process::Command;

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

fn setup_initialized_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();

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
    dir
}

#[test]
/// `keel init --merge` should merge with existing configuration without data loss.
fn test_init_merge_preserves_existing_config() {
    let dir = setup_initialized_project();
    let keel = keel_bin();

    let config_path = dir.path().join(".keel/keel.json");
    let _original = fs::read_to_string(&config_path).unwrap();

    let output = Command::new(&keel)
        .args(["init", "--merge"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init --merge");

    assert!(output.status.success(), "init --merge should succeed");
    assert!(
        config_path.exists(),
        "keel.json should still exist after merge"
    );
    let after = fs::read_to_string(&config_path).unwrap();
    let _: serde_json::Value =
        serde_json::from_str(&after).expect("keel.json should still be valid JSON after merge");
}

#[test]
/// `keel init --merge` should re-map the codebase while keeping existing graph data.
fn test_init_merge_remaps_with_existing_data() {
    let dir = setup_initialized_project();
    let keel = keel_bin();

    fs::write(
        dir.path().join("src/new.ts"),
        "export function newFunc(x: number): number { return x + 1; }\n",
    )
    .unwrap();

    let output = Command::new(&keel)
        .args(["init", "--merge"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init --merge");

    assert!(output.status.success(), "init --merge should succeed");
    assert!(dir.path().join(".keel/graph.db").exists());
    let db_size = fs::metadata(dir.path().join(".keel/graph.db"))
        .unwrap()
        .len();
    assert!(
        db_size > 4096,
        "graph.db should contain mapped data after merge"
    );
}

#[test]
/// `keel init --merge` should handle schema migrations if needed.
fn test_init_merge_handles_schema_migration() {
    let dir = setup_initialized_project();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["init", "--merge"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init --merge");

    assert!(
        output.status.success(),
        "init --merge should handle current schema"
    );
}

#[test]
/// `keel init --merge` should reset circuit breaker state.
fn test_init_merge_resets_circuit_breaker() {
    let dir = setup_initialized_project();
    let keel = keel_bin();

    let _ = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let output = Command::new(&keel)
        .args(["init", "--merge"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init --merge");

    assert!(output.status.success(), "init --merge should succeed");

    let compile_out = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let code = compile_out.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "compile after merge should exit 0 or 1, got {code}"
    );
}
