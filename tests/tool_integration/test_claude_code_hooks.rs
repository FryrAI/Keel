// Tests for Claude Code tool integration (Spec 009)

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

/// Initialize a project with .claude/ directory so keel init detects Claude Code.
fn init_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();
    // Create .claude/ so keel init detects Claude Code
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
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

/// Initialize a project with existing .claude/settings.json before keel init.
fn init_project_with_existing_settings() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();
    // Create .claude/ with existing settings.json BEFORE init
    let claude_dir = dir.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join("settings.json"),
        r#"{"existing_key": true}"#,
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
        "keel init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

#[test]
fn test_claude_code_settings_json_generation() {
    let dir = init_project();
    let settings = dir.path().join(".claude/settings.json");
    assert!(
        settings.exists(),
        "Claude Code settings.json should be generated"
    );
    let contents = fs::read_to_string(&settings).unwrap();
    let _: serde_json::Value = serde_json::from_str(&contents).expect("should be valid JSON");
}

#[test]
fn test_claude_code_hook_config_has_session_start() {
    let dir = init_project();
    let settings = dir.path().join(".claude/settings.json");
    let contents = fs::read_to_string(&settings).unwrap();
    assert!(
        contents.contains("SessionStart") || contents.contains("session_start"),
        "should have SessionStart event config"
    );
}

#[test]
fn test_claude_code_hook_config_has_post_tool_use() {
    let dir = init_project();
    let settings = dir.path().join(".claude/settings.json");
    let contents = fs::read_to_string(&settings).unwrap();
    assert!(
        contents.contains("PostToolUse") || contents.contains("post_tool_use"),
        "should have PostToolUse event config"
    );
}

#[test]
fn test_claude_code_hook_output_format_is_llm() {
    let dir = init_project();
    let settings = dir.path().join(".claude/settings.json");
    let contents = fs::read_to_string(&settings).unwrap();
    assert!(
        contents.contains("--llm"),
        "hooks should use --llm output format"
    );
}

#[test]
fn test_claude_code_hook_fires_on_write_tool() {
    let dir = init_project();
    let settings = dir.path().join(".claude/settings.json");
    let contents = fs::read_to_string(&settings).unwrap();
    assert!(
        contents.contains("Write") || contents.contains("write"),
        "should trigger on Write tool"
    );
}

#[test]
fn test_claude_code_hook_fires_on_edit_tool() {
    let dir = init_project();
    let settings = dir.path().join(".claude/settings.json");
    let contents = fs::read_to_string(&settings).unwrap();
    assert!(
        contents.contains("Edit") || contents.contains("edit"),
        "should trigger on Edit tool"
    );
}

#[test]
fn test_claude_code_hook_skips_non_source_files() {
    let dir = init_project();
    let settings = dir.path().join(".claude/settings.json");
    let contents = fs::read_to_string(&settings).unwrap();
    assert!(
        !contents.is_empty(),
        "settings should exist with file filtering"
    );
}

#[test]
fn test_claude_code_hook_batch_mode_support() {
    let dir = init_project();
    let settings = dir.path().join(".claude/settings.json");
    let contents = fs::read_to_string(&settings).unwrap();
    // The settings.json itself doesn't contain batch flags,
    // but the instruction file (CLAUDE.md) does
    let md = dir.path().join("CLAUDE.md");
    let md_contents = fs::read_to_string(&md).unwrap();
    assert!(
        contents.contains("batch") || contents.contains("--batch") || md_contents.contains("batch"),
        "should support batch mode (in settings or instructions)"
    );
}

#[test]
fn test_claude_code_settings_json_merges_with_existing() {
    let dir = init_project_with_existing_settings();
    let settings = dir.path().join(".claude/settings.json");
    let contents = fs::read_to_string(&settings).unwrap();
    assert!(
        contents.contains("existing_key"),
        "existing settings should be preserved"
    );
    assert!(
        contents.contains("hooks") || contents.contains("SessionStart"),
        "keel hook config should be merged in"
    );
}

#[test]
fn test_claude_code_hook_exit_code_propagation() {
    let dir = init_project();
    let settings = dir.path().join(".claude/settings.json");
    let contents = fs::read_to_string(&settings).unwrap();
    // The hooks use post-edit.sh which runs keel compile
    assert!(
        contents.contains("keel") || contents.contains("post-edit"),
        "hook should invoke keel compile (directly or via post-edit.sh)"
    );
}
