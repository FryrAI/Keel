// Tests for `keel search` command — graph search by name

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
fn test_search_exact_match() {
    let dir = init_and_map(&[
        ("src/utils.ts", "export function calculateTotal(x: number): number { return x; }\n"),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["search", "calculateTotal"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel search");

    assert_eq!(output.status.code(), Some(0), "search should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("calculateTotal"),
        "search output should contain the function name: {stdout}"
    );
}

#[test]
fn test_search_json_output() {
    let dir = init_and_map(&[
        ("src/math.ts", "export function add(a: number, b: number): number { return a + b; }\n"),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["search", "add", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel search");

    assert_eq!(output.status.code(), Some(0), "search --json should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("search --json should produce valid JSON: {e}\nstdout: {stdout}"));

    assert_eq!(val["command"], "search");
    assert!(val["results"].is_array(), "results should be an array");

    let results = val["results"].as_array().unwrap();
    assert!(!results.is_empty(), "should find at least one result for 'add'");

    let first = &results[0];
    assert!(first["hash"].is_string(), "result should have a hash");
    assert!(first["file"].is_string(), "result should have a file");
    assert!(first["name"].is_string(), "result should have a name");
}

#[test]
fn test_search_substring_fallback() {
    let dir = init_and_map(&[
        ("src/handlers.ts", "export function handleUserLogin(user: string): void {}\n"),
        ("src/utils.ts", "export function handlePayment(amount: number): void {}\n"),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["search", "handle", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel search");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let results = val["results"].as_array().unwrap();
    assert!(
        results.len() >= 2,
        "substring search for 'handle' should find at least 2 results, got {}",
        results.len()
    );
}

#[test]
fn test_search_with_kind_filter() {
    let dir = init_and_map(&[
        ("src/models.ts", "export class UserModel {}\nexport function createUser(name: string): void {}\n"),
    ]);
    let keel = keel_bin();

    // Search for "User" filtered to functions only
    let output = Command::new(&keel)
        .args(["search", "User", "--kind", "function", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel search");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&stdout) {
        let results = val["results"].as_array().unwrap();
        for r in results {
            assert_eq!(
                r["kind"].as_str().unwrap_or(""),
                "function",
                "kind filter should only return functions"
            );
        }
    }
}

#[test]
fn test_search_no_results() {
    let dir = init_and_map(&[
        ("src/index.ts", "export function hello(name: string): string { return name; }\n"),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["search", "nonexistentXYZ123", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel search");

    assert_eq!(output.status.code(), Some(0), "search with no results should still exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let results = val["results"].as_array().unwrap();
    assert!(results.is_empty(), "should find no results for gibberish term");
}

#[test]
fn test_search_not_initialized_exits_2() {
    let dir = TempDir::new().unwrap();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["search", "anything"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel search");

    assert_eq!(
        output.status.code(),
        Some(2),
        "search without init should exit 2"
    );
}

#[test]
fn test_search_llm_format() {
    let dir = init_and_map(&[
        ("src/index.ts", "export function hello(name: string): string { return name; }\n"),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["search", "hello", "--llm"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel search");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("SEARCH"),
        "LLM format should start with SEARCH prefix: {stdout}"
    );
}

#[test]
fn test_search_performance() {
    let dir = init_and_map(&[
        ("src/index.ts", "export function hello(name: string): string { return name; }\n"),
    ]);
    let keel = keel_bin();

    let start = Instant::now();
    let _ = Command::new(&keel)
        .args(["search", "hello"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel search");
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 2000,
        "search took {:?} — should be fast",
        elapsed
    );
}
