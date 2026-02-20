// Tests for instruction file generation (Spec 009)

use std::fs;
use std::process::Command;

use tempfile::TempDir;

fn keel_bin() -> std::path::PathBuf {
    // Try relative to test executable (standard cargo test layout)
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("keel");
    if path.exists() {
        return path;
    }

    // Fallback: workspace target/debug/keel (handles cargo-llvm-cov)
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fallback = workspace.join("target/debug/keel");
    if fallback.exists() {
        return fallback;
    }

    // Last resort: build the binary
    let status = Command::new("cargo")
        .args(["build", "-p", "keel-cli"])
        .current_dir(&workspace)
        .status()
        .expect("Failed to build keel");
    assert!(status.success(), "Failed to build keel binary");
    fallback
}

/// Initialize a project with .claude/ directory so Claude Code is detected.
fn init_project_with_claude() -> TempDir {
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

/// Initialize a project with .windsurf/ directory so Windsurf is detected.
fn init_project_with_windsurf() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join(".windsurf")).unwrap();
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

/// Initialize a project with .github/ directory so Copilot is detected.
fn init_project_with_copilot() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join(".github")).unwrap();
    fs::write(dir.path().join(".github/copilot-instructions.md"), "").unwrap();
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

/// Initialize a project with existing CLAUDE.md (has content before init).
fn init_project_with_existing_claude_md() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();
    // Create .claude/ for detection
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    // Write existing CLAUDE.md BEFORE init
    fs::write(
        dir.path().join("CLAUDE.md"),
        "# Existing Instructions\n\nUser content here.\n",
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

/// Initialize a bare project (no tool directories) — only AGENTS.md should be generated.
fn init_project_bare() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
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
fn test_claude_md_generation() {
    let dir = init_project_with_claude();
    let md = dir.path().join("CLAUDE.md");
    assert!(md.exists(), "CLAUDE.md should be generated");
    let contents = fs::read_to_string(&md).unwrap();
    assert!(
        contents.contains("keel"),
        "should contain keel instructions"
    );
}

#[test]
fn test_claude_md_includes_compile_workflow() {
    let dir = init_project_with_claude();
    let md = dir.path().join("CLAUDE.md");
    let contents = fs::read_to_string(&md).unwrap();
    assert!(
        contents.contains("keel compile"),
        "should include compile workflow"
    );
}

#[test]
fn test_agents_md_generation() {
    let dir = init_project_bare();
    let md = dir.path().join("AGENTS.md");
    assert!(md.exists(), "AGENTS.md should be generated");
}

#[test]
fn test_windsurfrules_generation() {
    let dir = init_project_with_windsurf();
    let rules = dir.path().join(".windsurfrules");
    assert!(rules.exists(), ".windsurfrules should be generated");
}

#[test]
fn test_copilot_instructions_generation() {
    let dir = init_project_with_copilot();
    let md = dir.path().join(".github/copilot-instructions.md");
    assert!(md.exists(), "copilot-instructions.md should be generated");
}

#[test]
fn test_instruction_files_include_error_codes() {
    let dir = init_project_with_claude();
    // AGENTS.md (universal) includes error codes
    let md = dir.path().join("AGENTS.md");
    let contents = fs::read_to_string(&md).unwrap();
    assert!(contents.contains("E001"), "should include E001");
    assert!(contents.contains("E005"), "should include E005");
    assert!(contents.contains("W001"), "should include W001");
}

#[test]
fn test_instruction_files_merge_with_existing() {
    let dir = init_project_with_existing_claude_md();
    let md = dir.path().join("CLAUDE.md");
    let contents = fs::read_to_string(&md).unwrap();
    assert!(
        contents.contains("Existing"),
        "existing content should be preserved"
    );
    assert!(
        contents.contains("keel"),
        "keel instructions should be appended"
    );
}

#[test]
fn test_instruction_files_idempotent() {
    let dir = init_project_with_claude();
    let md = dir.path().join("CLAUDE.md");
    assert!(md.exists(), "CLAUDE.md should exist");
    let first = fs::read_to_string(&md).unwrap();
    assert!(!first.is_empty(), "file should have content");
}

#[test]
fn test_init_yes_flag_skips_prompt() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();
    // Create .claude/ so Claude Code is detected
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    let keel = keel_bin();
    let out = Command::new(&keel)
        .args(["init", "--yes"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "keel init --yes failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        dir.path().join(".claude/settings.json").exists(),
        "Claude Code settings.json should be generated with --yes"
    );
}

#[test]
fn test_init_yes_short_flag() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();
    let keel = keel_bin();
    let out = Command::new(&keel)
        .args(["init", "-y"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "keel init -y failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Bare project with -y should still succeed (no agents detected, only AGENTS.md)
    assert!(
        dir.path().join("AGENTS.md").exists(),
        "AGENTS.md should always be generated"
    );
}
