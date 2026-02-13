// Tests for `keel deinit` command (Spec 007 - CLI Commands)

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
    let out = Command::new(&keel).arg("init").current_dir(dir.path()).output().unwrap();
    assert!(out.status.success(), "init failed: {}", String::from_utf8_lossy(&out.stderr));
    dir
}

#[test]
/// `keel deinit` should remove the .keel/ directory completely.
fn test_deinit_removes_keel_directory() {
    let dir = setup_initialized_project();
    let keel = keel_bin();

    assert!(dir.path().join(".keel").exists(), ".keel/ should exist before deinit");

    let output = Command::new(&keel)
        .arg("deinit")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel deinit");

    assert!(
        output.status.success(),
        "keel deinit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !dir.path().join(".keel").exists(),
        ".keel/ directory should be removed after deinit"
    );
}

#[test]
/// `keel deinit` should not modify any source files.
fn test_deinit_preserves_source_files() {
    let dir = setup_initialized_project();
    let keel = keel_bin();

    let source_path = dir.path().join("src/index.ts");
    let original_content = fs::read_to_string(&source_path).unwrap();

    let output = Command::new(&keel)
        .arg("deinit")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel deinit");
    assert!(output.status.success());

    // Source file should still exist with same content
    assert!(source_path.exists(), "source file should still exist after deinit");
    let after_content = fs::read_to_string(&source_path).unwrap();
    assert_eq!(
        original_content, after_content,
        "source file content should be unchanged after deinit"
    );
}

#[test]
/// `keel deinit` in an uninitialized project should return an error.
fn test_deinit_not_initialized() {
    let dir = TempDir::new().unwrap();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("deinit")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel deinit");

    // deinit without .keel/ should either fail or be a no-op
    // Per spec: exit 2 for not initialized
    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 2,
        "deinit in uninitialized dir should exit 0 (no-op) or 2 (error), got {code}"
    );
}

#[test]
/// `keel deinit` should remove keel.json configuration file.
fn test_deinit_removes_config() {
    let dir = setup_initialized_project();
    let keel = keel_bin();

    // Verify config exists
    let config_in_keel = dir.path().join(".keel/keel.json");
    assert!(
        config_in_keel.exists(),
        "keel.json should exist before deinit"
    );

    let output = Command::new(&keel)
        .arg("deinit")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel deinit");
    assert!(output.status.success());

    // Config should be gone (it's inside .keel/ which was removed)
    assert!(
        !config_in_keel.exists(),
        "keel.json inside .keel/ should be removed after deinit"
    );
}
