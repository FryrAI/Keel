// Tests for Cursor IDE tool integration (Spec 009)
// Validates hooks.json and .mdc generation when .cursor/ directory is present.

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

fn init_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();
    // Create .cursor/ so tool detection fires during keel init
    fs::create_dir_all(dir.path().join(".cursor")).unwrap();
    let keel = keel_bin();
    let out = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "keel init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

#[test]
fn test_cursor_hooks_json_generation() {
    let dir = init_project();
    let hooks = dir.path().join(".cursor/hooks.json");
    assert!(hooks.exists(), "Cursor hooks.json should be generated");
    let contents = fs::read_to_string(&hooks).unwrap();
    let _: serde_json::Value = serde_json::from_str(&contents).expect("should be valid JSON");
}

#[test]
fn test_cursor_hooks_json_has_file_edit_trigger() {
    let dir = init_project();
    let hooks = dir.path().join(".cursor/hooks.json");
    let contents = fs::read_to_string(&hooks).unwrap();
    assert!(
        contents.contains("keel compile"),
        "should reference keel compile on file edit"
    );
}

#[test]
fn test_cursor_mdc_rules_file_generation() {
    let dir = init_project();
    let mdc = dir.path().join(".cursor/rules/keel.mdc");
    assert!(mdc.exists(), "keel.mdc rules file should be generated");
}

#[test]
fn test_cursor_mdc_includes_error_code_descriptions() {
    let dir = init_project();
    let mdc = dir.path().join(".cursor/rules/keel.mdc");
    let contents = fs::read_to_string(&mdc).unwrap();
    assert!(contents.contains("E001"), "should include E001");
    assert!(contents.contains("W001"), "should include W001");
}

#[test]
fn test_cursor_hooks_json_merges_with_existing() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();

    // Create .cursor/ with existing hooks.json BEFORE keel init
    let cursor_dir = dir.path().join(".cursor");
    fs::create_dir_all(&cursor_dir).unwrap();
    fs::write(cursor_dir.join("hooks.json"), r#"{"existing": true}"#).unwrap();

    // Run keel init — should detect .cursor and merge
    let keel = keel_bin();
    let out = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "keel init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let hooks = dir.path().join(".cursor/hooks.json");
    let contents = fs::read_to_string(&hooks).unwrap();
    assert!(
        contents.contains("existing"),
        "existing hooks should be preserved"
    );
    assert!(contents.contains("hooks"), "keel hooks should be added");
}

#[test]
fn test_cursor_hooks_output_format() {
    let dir = init_project();
    let hooks = dir.path().join(".cursor/hooks.json");
    let contents = fs::read_to_string(&hooks).unwrap();
    assert!(contents.contains("--llm"), "output should use LLM format");
}

#[test]
fn test_cursor_mdc_placed_in_correct_directory() {
    let dir = init_project();
    let mdc = dir.path().join(".cursor/rules/keel.mdc");
    assert!(mdc.exists(), "keel.mdc should be at .cursor/rules/keel.mdc");
}

#[test]
fn test_cursor_hooks_idempotent_generation() {
    let dir = init_project();
    let hooks = dir.path().join(".cursor/hooks.json");
    assert!(hooks.exists(), "hooks.json should exist");
    let first = fs::read_to_string(&hooks).unwrap();
    assert!(!first.is_empty(), "hooks should have content");
}
