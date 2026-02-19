// Tests for Gemini CLI tool integration (Spec 009)
// Validates settings.json and GEMINI.md generation when .gemini/ directory is present.

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
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();
    // Create .gemini/ so tool detection fires during keel init
    fs::create_dir_all(dir.path().join(".gemini")).unwrap();
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
fn test_gemini_settings_json_generation() {
    let dir = init_project();
    let settings = dir.path().join(".gemini/settings.json");
    assert!(
        settings.exists(),
        "Gemini settings.json should be generated"
    );
    let contents = fs::read_to_string(&settings).unwrap();
    let _: serde_json::Value = serde_json::from_str(&contents).expect("should be valid JSON");
}

#[test]
fn test_gemini_md_instruction_file_generation() {
    let dir = init_project();
    let md = dir.path().join("GEMINI.md");
    assert!(md.exists(), "GEMINI.md should be generated");
}

#[test]
fn test_gemini_md_includes_keel_commands() {
    let dir = init_project();
    let md = dir.path().join("GEMINI.md");
    let contents = fs::read_to_string(&md).unwrap();
    assert!(
        contents.contains("compile"),
        "should include compile command"
    );
    assert!(
        contents.contains("discover"),
        "should include discover command"
    );
}

#[test]
fn test_gemini_md_includes_error_handling() {
    let dir = init_project();
    let md = dir.path().join("GEMINI.md");
    let contents = fs::read_to_string(&md).unwrap();
    assert!(
        contents.contains("E001") || contents.contains("error"),
        "should include error handling"
    );
}

#[test]
fn test_gemini_settings_has_post_edit_hook() {
    let dir = init_project();
    let settings = dir.path().join(".gemini/settings.json");
    let contents = fs::read_to_string(&settings).unwrap();
    assert!(
        contents.contains("keel compile"),
        "should reference keel compile on edit"
    );
}

#[test]
fn test_gemini_settings_merges_with_existing() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();

    // Create .gemini/ with existing settings.json BEFORE keel init
    let gemini_dir = dir.path().join(".gemini");
    fs::create_dir_all(&gemini_dir).unwrap();
    fs::write(gemini_dir.join("settings.json"), r#"{"existing": true}"#).unwrap();

    // Run keel init — should detect .gemini and merge
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

    let settings = dir.path().join(".gemini/settings.json");
    let contents = fs::read_to_string(&settings).unwrap();
    assert!(
        contents.contains("existing"),
        "existing settings should be preserved"
    );
    assert!(contents.contains("hooks"), "keel hooks should be added");
}

#[test]
fn test_gemini_hooks_output_format_is_llm() {
    let dir = init_project();
    let settings = dir.path().join(".gemini/settings.json");
    let contents = fs::read_to_string(&settings).unwrap();
    assert!(contents.contains("--llm"), "should use LLM output format");
}

#[test]
fn test_gemini_md_placed_in_project_root() {
    let dir = init_project();
    let md = dir.path().join("GEMINI.md");
    assert!(md.exists(), "GEMINI.md should be at project root");
}
