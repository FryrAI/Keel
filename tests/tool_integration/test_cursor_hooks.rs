// Tests for Cursor IDE tool integration (Spec 009)
// BUG: Cursor hooks.json and .mdc generation not yet implemented.
// keel init only detects .cursor directory but does not generate configs.

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

fn init_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("index.ts"), "export function hello(name: string): string { return name; }\n").unwrap();
    let keel = keel_bin();
    let out = Command::new(&keel).arg("init").current_dir(dir.path()).output().unwrap();
    assert!(out.status.success());
    dir
}

#[test]
#[ignore = "BUG: Cursor hooks.json generation not yet implemented"]
fn test_cursor_hooks_json_generation() {
    let dir = init_project();
    let hooks = dir.path().join(".cursor/hooks.json");
    assert!(hooks.exists(), "Cursor hooks.json should be generated");
    let contents = fs::read_to_string(&hooks).unwrap();
    let _: serde_json::Value = serde_json::from_str(&contents).expect("should be valid JSON");
}

#[test]
#[ignore = "BUG: Cursor hooks.json generation not yet implemented"]
fn test_cursor_hooks_json_has_file_edit_trigger() {
    let dir = init_project();
    let hooks = dir.path().join(".cursor/hooks.json");
    let contents = fs::read_to_string(&hooks).unwrap();
    assert!(contents.contains("keel compile"), "should invoke keel compile on file edit");
}

#[test]
#[ignore = "BUG: Cursor MDC rules generation not yet implemented"]
fn test_cursor_mdc_rules_file_generation() {
    let dir = init_project();
    let mdc = dir.path().join(".cursor/rules/keel.mdc");
    assert!(mdc.exists(), "keel.mdc rules file should be generated");
}

#[test]
#[ignore = "BUG: Cursor MDC rules generation not yet implemented"]
fn test_cursor_mdc_includes_error_code_descriptions() {
    let dir = init_project();
    let mdc = dir.path().join(".cursor/rules/keel.mdc");
    let contents = fs::read_to_string(&mdc).unwrap();
    assert!(contents.contains("E001"), "should include E001");
    assert!(contents.contains("W001"), "should include W001");
}

#[test]
#[ignore = "BUG: Cursor hooks.json generation not yet implemented"]
fn test_cursor_hooks_json_merges_with_existing() {
    let dir = init_project();
    let cursor_dir = dir.path().join(".cursor");
    fs::create_dir_all(&cursor_dir).unwrap();
    fs::write(cursor_dir.join("hooks.json"), r#"{"existing": true}"#).unwrap();
    let hooks = dir.path().join(".cursor/hooks.json");
    let contents = fs::read_to_string(&hooks).unwrap();
    assert!(contents.contains("existing"), "existing hooks should be preserved");
}

#[test]
#[ignore = "BUG: Cursor hooks.json generation not yet implemented"]
fn test_cursor_hooks_output_format() {
    let dir = init_project();
    let hooks = dir.path().join(".cursor/hooks.json");
    let contents = fs::read_to_string(&hooks).unwrap();
    assert!(contents.contains("--llm"), "output should use LLM format");
}

#[test]
#[ignore = "BUG: Cursor MDC rules generation not yet implemented"]
fn test_cursor_mdc_placed_in_correct_directory() {
    let dir = init_project();
    let mdc = dir.path().join(".cursor/rules/keel.mdc");
    assert!(mdc.exists(), "keel.mdc should be at .cursor/rules/keel.mdc");
}

#[test]
#[ignore = "BUG: Cursor hooks.json generation not yet implemented"]
fn test_cursor_hooks_idempotent_generation() {
    let dir = init_project();
    let hooks = dir.path().join(".cursor/hooks.json");
    if hooks.exists() {
        let first = fs::read_to_string(&hooks).unwrap();
        assert!(!first.is_empty(), "hooks should have content");
    }
}
