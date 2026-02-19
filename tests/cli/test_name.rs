// Tests for `keel name` command — name and location suggestions

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
    let out = Command::new(&keel).arg("init").current_dir(dir.path()).output().unwrap();
    assert!(out.status.success(), "init failed: {}", String::from_utf8_lossy(&out.stderr));
    let out = Command::new(&keel).arg("map").current_dir(dir.path()).output().unwrap();
    assert!(out.status.success(), "map failed: {}", String::from_utf8_lossy(&out.stderr));
    dir
}

#[test]
fn test_name_basic_suggestion() {
    let dir = init_and_map(&[
        ("src/auth.ts", "export function login(user: string): boolean { return true; }\nexport function logout(): void {}\n"),
        ("src/utils.ts", "export function hash(input: string): string { return input; }\n"),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["name", "validate user credentials"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel name");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(
        code, 0,
        "name should exit 0\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "name should produce suggestions");
}

#[test]
fn test_name_with_module_constraint() {
    let dir = init_and_map(&[
        ("src/auth/login.ts", "export function login(user: string): boolean { return true; }\n"),
        ("src/auth/register.ts", "export function register(user: string): boolean { return true; }\n"),
        ("src/utils.ts", "export function hash(input: string): string { return input; }\n"),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["name", "verify email address", "--module", "src/auth"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel name --module");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(
        code, 0,
        "name --module should exit 0\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_name_with_kind_constraint() {
    let dir = init_and_map(&[
        ("src/models.ts", "export class User {}\nexport function createUser(name: string): void {}\n"),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["name", "a data model for products", "--kind", "class"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel name --kind");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(
        code, 0,
        "name --kind should exit 0\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_name_not_initialized_exits_2() {
    let dir = TempDir::new().unwrap();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["name", "some description"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel name");

    assert_eq!(
        output.status.code(),
        Some(2),
        "name without init should exit 2"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("init") || stderr.contains("graph"),
        "should mention init: {stderr}"
    );
}

#[test]
fn test_name_performance() {
    let dir = init_and_map(&[
        ("src/index.ts", "export function hello(name: string): string { return name; }\n"),
    ]);
    let keel = keel_bin();

    let start = Instant::now();
    let _ = Command::new(&keel)
        .args(["name", "a helper function"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel name");
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 2000,
        "name took {:?} — should be fast",
        elapsed
    );
}
