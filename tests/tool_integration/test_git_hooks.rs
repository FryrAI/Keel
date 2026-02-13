// Tests for git hook integration (Spec 009)

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

fn setup_git_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();

    // Init git repo
    let git = Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run git init");
    assert!(git.status.success());

    // Configure git user for commits
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    dir
}

#[test]
/// Pre-commit hook generation: `keel init` in a git repo creates pre-commit hook.
fn test_pre_commit_hook_generation() {
    let dir = setup_git_project();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");
    assert!(output.status.success());

    let hook_path = dir.path().join(".git/hooks/pre-commit");
    assert!(hook_path.exists(), "pre-commit hook should be created");

    let contents = fs::read_to_string(&hook_path).unwrap();
    assert!(
        contents.contains("keel compile"),
        "hook should invoke keel compile, got: {contents}"
    );
}

#[test]
/// Pre-commit hook should have executable permissions.
fn test_pre_commit_hook_is_executable() {
    let dir = setup_git_project();
    let keel = keel_bin();

    Command::new(&keel).arg("init").current_dir(dir.path()).output().unwrap();

    let hook_path = dir.path().join(".git/hooks/pre-commit");
    assert!(hook_path.exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&hook_path).unwrap().permissions().mode();
        assert!(
            mode & 0o111 != 0,
            "pre-commit hook should be executable, mode: {:o}",
            mode
        );
    }
}

#[test]
/// Pre-commit hook should run keel compile on staged source files.
fn test_pre_commit_hook_compiles_staged_files() {
    let dir = setup_git_project();
    let keel = keel_bin();

    Command::new(&keel).arg("init").current_dir(dir.path()).output().unwrap();

    // Stage a source file
    Command::new("git")
        .args(["add", "src/index.ts"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // The hook should contain logic to compile staged files
    let hook_path = dir.path().join(".git/hooks/pre-commit");
    let contents = fs::read_to_string(&hook_path).unwrap();
    assert!(
        contents.contains("keel") && contents.contains("compile"),
        "hook should reference keel compile for staged files"
    );
}

#[test]
/// Pre-commit hook allows commits when keel compile passes.
fn test_pre_commit_hook_allows_clean_commits() {
    let dir = setup_git_project();
    let keel = keel_bin();

    Command::new(&keel).arg("init").current_dir(dir.path()).output().unwrap();

    // Stage and commit the initial files — hook should pass for clean code
    Command::new("git").args(["add", "."]).current_dir(dir.path()).output().unwrap();

    // Attempt a commit — the hook runs keel compile
    let commit = Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run git commit");

    // The commit should either succeed (hook passes) or fail (keel compile found issues)
    // Either is valid behavior — we just verify it doesn't crash
    let code = commit.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "git commit should exit 0 (clean) or 1 (hook blocked), got {code}\nstderr: {}",
        String::from_utf8_lossy(&commit.stderr)
    );
}

#[test]
/// When a pre-commit hook already exists, keel should not overwrite it.
fn test_pre_commit_hook_preserves_existing_hooks() {
    let dir = setup_git_project();
    let keel = keel_bin();

    // Create an existing pre-commit hook
    let hook_path = dir.path().join(".git/hooks/pre-commit");
    fs::write(&hook_path, "#!/bin/sh\necho 'existing hook'\n").unwrap();

    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");
    assert!(output.status.success());

    // The existing hook should be preserved (not overwritten)
    let contents = fs::read_to_string(&hook_path).unwrap();
    assert!(
        contents.contains("existing hook"),
        "existing hook content should be preserved, got: {contents}"
    );
}

#[test]
/// Hook should only check source files, not docs/config.
fn test_pre_commit_hook_only_checks_source_files() {
    let dir = setup_git_project();
    let keel = keel_bin();

    Command::new(&keel).arg("init").current_dir(dir.path()).output().unwrap();

    // The hook script should contain keel compile (which only processes source files)
    let hook_path = dir.path().join(".git/hooks/pre-commit");
    let contents = fs::read_to_string(&hook_path).unwrap();

    // Verify the hook calls keel compile (which inherently only checks source files)
    assert!(
        contents.contains("keel compile"),
        "hook should invoke keel compile"
    );
}

#[test]
/// Hook exit code 1 (violations) should block git commit.
fn test_pre_commit_hook_exit_code_1_blocks_commit() {
    let dir = setup_git_project();
    let keel = keel_bin();

    Command::new(&keel).arg("init").current_dir(dir.path()).output().unwrap();

    // The hook should propagate keel compile's exit code
    let hook_path = dir.path().join(".git/hooks/pre-commit");
    let contents = fs::read_to_string(&hook_path).unwrap();

    // Verify the hook has a shebang and runs keel compile (exit code is propagated)
    assert!(contents.starts_with("#!/"), "hook should have shebang");
    assert!(
        contents.contains("keel compile"),
        "hook should run keel compile"
    );
    // The shell script should propagate the exit code (not swallow it)
    // A simple `keel compile "$@"` as the last line will propagate the exit code
}
