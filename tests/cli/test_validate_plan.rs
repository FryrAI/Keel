// Tests for `keel validate-plan` — plan validation against the dependency graph.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

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

/// Mapped cross-file fixture: lib.ts::foo called by main.ts::run.
fn mapped_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("lib.ts"),
        "export function foo(x: number): number {\n  return x + 1;\n}\n",
    )
    .unwrap();
    fs::write(
        src.join("main.ts"),
        "import { foo } from './lib';\nexport function run(): number {\n  return foo(1);\n}\n",
    )
    .unwrap();

    let keel = keel_bin();
    assert!(Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap()
        .status
        .success());
    let map = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        map.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&map.stderr)
    );
    dir
}

fn keel(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run keel")
}

#[test]
fn test_validate_plan_removal_is_high_risk() {
    let dir = mapped_fixture();
    let plan = dir.path().join("plan.md");
    fs::write(&plan, "## Plan\n\n1. Remove foo since it is unused.\n").unwrap();

    let out = keel(dir.path(), &["validate-plan", "plan.md", "--llm"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "validate-plan should exit 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("VALIDATE-PLAN"), "missing header: {stdout}");
    assert!(
        stdout.contains("ACTION remove foo"),
        "missing action: {stdout}"
    );
    assert!(
        stdout.contains("risk=HIGH"),
        "removal with callers is HIGH: {stdout}"
    );
    assert!(
        stdout.contains("src/main.ts"),
        "caller in main.ts should be listed: {stdout}"
    );
    assert!(
        stdout.contains("order:"),
        "should suggest a callers-first order: {stdout}"
    );
}

#[test]
fn test_validate_plan_json_shape() {
    let dir = mapped_fixture();
    let plan = dir.path().join("plan.md");
    fs::write(&plan, "Delete foo entirely.\n").unwrap();

    let out = keel(dir.path(), &["validate-plan", "plan.md", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(json["command"], "validate-plan");
    assert_eq!(json["unrecognized"], false);
    let actions = json["actions"].as_array().unwrap();
    assert_eq!(actions[0]["symbol"], "foo");
    assert_eq!(actions[0]["action"], "remove");
    assert_eq!(actions[0]["risk"], "HIGH");
    assert!(actions[0]["caller_count"].as_u64().unwrap() >= 1);
    assert!(actions[0]["callers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["file"].as_str().unwrap_or("").contains("main.ts")));
}

#[test]
fn test_validate_plan_nonsense_is_graceful() {
    let dir = mapped_fixture();
    let plan = dir.path().join("plan.md");
    fs::write(&plan, "Buy groceries and water the plants.\n").unwrap();

    let out = keel(dir.path(), &["validate-plan", "plan.md"]);
    assert_eq!(out.status.code(), Some(0), "nonsense plan must not error");
    let stdout = String::from_utf8_lossy(&out.stdout).to_lowercase();
    assert!(
        stdout.contains("no graph-relevant actions detected"),
        "should report graceful no-detection: {stdout}"
    );
}

#[test]
fn test_validate_plan_stdin() {
    let dir = mapped_fixture();
    let mut child = Command::new(keel_bin())
        .args(["validate-plan", "-", "--llm"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn keel");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"Rename foo to compute.\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ACTION rename foo"),
        "stdin plan not read: {stdout}"
    );
}
