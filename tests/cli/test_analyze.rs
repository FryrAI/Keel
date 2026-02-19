// Tests for `keel analyze` command — file structure and smell analysis

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

#[test]
fn test_analyze_single_file() {
    let dir = init_and_map(&[
        ("src/utils.ts", "export function add(a: number, b: number): number { return a + b; }\nexport function sub(a: number, b: number): number { return a - b; }\n"),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["analyze", "src/utils.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel analyze");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(
        code,
        0,
        "analyze should exit 0 for a mapped file\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.is_empty(),
        "analyze should produce output for a mapped file"
    );
}

#[test]
fn test_analyze_file_with_class() {
    let dir = init_and_map(&[(
        "src/service.ts",
        concat!(
            "export class UserService {\n",
            "  getUser(id: string): string { return id; }\n",
            "  createUser(name: string): string { return name; }\n",
            "  deleteUser(id: string): void {}\n",
            "}\n",
        ),
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["analyze", "src/service.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel analyze");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(
        code,
        0,
        "analyze should exit 0\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_analyze_unknown_file_exits_2() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["analyze", "src/nonexistent.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel analyze");

    assert_eq!(
        output.status.code(),
        Some(2),
        "analyze for unknown file should exit 2"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no data") || stderr.contains("not found") || stderr.contains("map"),
        "should hint about missing data: {stderr}"
    );
}

#[test]
fn test_analyze_not_initialized_exits_2() {
    let dir = TempDir::new().unwrap();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["analyze", "src/index.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel analyze");

    assert_eq!(
        output.status.code(),
        Some(2),
        "analyze without init should exit 2"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not initialized") || stderr.contains("init"),
        "should mention initialization: {stderr}"
    );
}

#[test]
fn test_analyze_verbose_shows_counts() {
    let dir = init_and_map(&[(
        "src/mod.ts",
        "export function a(): void {}\nexport function b(): void {}\nexport class C {}\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["analyze", "src/mod.ts", "--verbose"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel analyze --verbose");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 0, "analyze --verbose should exit 0");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("function") || stderr.contains("class"),
        "verbose output should mention counts: {stderr}"
    );
}

#[test]
fn test_analyze_performance() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let start = Instant::now();
    let _ = Command::new(&keel)
        .args(["analyze", "src/index.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel analyze");
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 2000,
        "analyze took {:?} — should be fast",
        elapsed
    );
}
