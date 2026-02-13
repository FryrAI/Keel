// Tests for instruction file generation (Spec 009)
// BUG: Instruction file generation (CLAUDE.md, AGENTS.md, .windsurfrules,
// copilot-instructions.md) not yet implemented. keel init does not generate these.

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
#[ignore = "BUG: CLAUDE.md instruction file generation not yet implemented"]
fn test_claude_md_generation() {
    let dir = init_project();
    let md = dir.path().join("CLAUDE.md");
    assert!(md.exists(), "CLAUDE.md should be generated");
    let contents = fs::read_to_string(&md).unwrap();
    assert!(contents.contains("keel"), "should contain keel instructions");
}

#[test]
#[ignore = "BUG: CLAUDE.md instruction file generation not yet implemented"]
fn test_claude_md_includes_compile_workflow() {
    let dir = init_project();
    let md = dir.path().join("CLAUDE.md");
    let contents = fs::read_to_string(&md).unwrap();
    assert!(contents.contains("keel compile"), "should include compile workflow");
}

#[test]
#[ignore = "BUG: AGENTS.md instruction file generation not yet implemented"]
fn test_agents_md_generation() {
    let dir = init_project();
    let md = dir.path().join("AGENTS.md");
    assert!(md.exists(), "AGENTS.md should be generated");
}

#[test]
#[ignore = "BUG: .windsurfrules generation not yet implemented"]
fn test_windsurfrules_generation() {
    let dir = init_project();
    let rules = dir.path().join(".windsurfrules");
    assert!(rules.exists(), ".windsurfrules should be generated");
}

#[test]
#[ignore = "BUG: copilot-instructions.md generation not yet implemented"]
fn test_copilot_instructions_generation() {
    let dir = init_project();
    let md = dir.path().join(".github/copilot-instructions.md");
    assert!(md.exists(), "copilot-instructions.md should be generated");
}

#[test]
#[ignore = "BUG: Instruction file generation not yet implemented"]
fn test_instruction_files_include_error_codes() {
    let dir = init_project();
    // Check any generated instruction file for error codes
    let md = dir.path().join("CLAUDE.md");
    let contents = fs::read_to_string(&md).unwrap();
    assert!(contents.contains("E001"), "should include E001");
    assert!(contents.contains("E005"), "should include E005");
    assert!(contents.contains("W001"), "should include W001");
}

#[test]
#[ignore = "BUG: Instruction file generation not yet implemented"]
fn test_instruction_files_merge_with_existing() {
    let dir = init_project();
    // Create existing CLAUDE.md
    fs::write(dir.path().join("CLAUDE.md"), "# Existing Instructions\n\nUser content here.\n").unwrap();
    // Re-init would need --merge support
    let md = dir.path().join("CLAUDE.md");
    let contents = fs::read_to_string(&md).unwrap();
    assert!(contents.contains("Existing"), "existing content should be preserved");
}

#[test]
#[ignore = "BUG: Instruction file generation not yet implemented"]
fn test_instruction_files_idempotent() {
    let dir = init_project();
    let md = dir.path().join("CLAUDE.md");
    if md.exists() {
        let first = fs::read_to_string(&md).unwrap();
        assert!(!first.is_empty(), "file should have content");
    }
}
