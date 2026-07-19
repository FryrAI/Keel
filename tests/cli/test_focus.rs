// Tests for `keel focus` command (issue #20).

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

fn init_and_map_project(files: &[(&str, &str)]) -> TempDir {
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
        .expect("Failed to run keel init");
    assert!(out.status.success(), "init failed");

    let out = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");
    assert!(
        out.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

const FILES: &[(&str, &str)] = &[
    (
        "src/caller.ts",
        "import { middle } from './middle';\nexport function caller(): void { middle(); }\n",
    ),
    (
        "src/middle.ts",
        "import { callee } from './callee';\nexport function middle(): void { callee(); }\n",
    ),
    ("src/callee.ts", "export function callee(): void {}\n"),
];

#[test]
/// `keel focus <file>` returns the context set for a mapped file (file mode).
fn test_focus_file_mode_succeeds() {
    let dir = init_and_map_project(FILES);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["focus", "src/middle.ts", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel focus");

    assert!(
        output.status.success(),
        "focus should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\n---\n{stdout}"));
    assert_eq!(parsed["command"], "focus");
    assert!(
        parsed["files"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false),
        "focus should list files to read"
    );
    // read_order should include the target file.
    let order = parsed["read_order"].as_array().unwrap();
    assert!(order.iter().any(|p| p == "src/middle.ts"));
}

#[test]
/// `keel focus` on an unknown target exits 2 with a clear error.
fn test_focus_unknown_target() {
    let dir = init_and_map_project(FILES);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["focus", "AAAAAAAAAAA"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel focus");

    assert_eq!(output.status.code(), Some(2));
}

#[test]
/// `keel focus` without init exits 2 (graph-backed command).
fn test_focus_requires_init() {
    let dir = TempDir::new().unwrap();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["focus", "src/middle.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel focus");

    assert_eq!(output.status.code(), Some(2));
}
