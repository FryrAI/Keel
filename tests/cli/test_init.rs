// Tests for `keel init` command (Spec 007 - CLI Commands)

use std::fs;
use std::process::Command;

use tempfile::TempDir;

/// Path to the keel binary built by cargo.
fn keel_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // remove test binary name
    path.pop(); // remove 'deps'
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

/// Create a temp project with a single TypeScript file.
fn setup_ts_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "function hello(name: string): string { return name; }\n",
    )
    .unwrap();
    dir
}

#[test]
#[ignore = "Not yet implemented"]
/// `keel init` in a fresh directory should create .keel/ directory structure.
fn test_init_creates_keel_directory() {
    // GIVEN a directory with source files but no .keel/
    // WHEN `keel init` is run
    // THEN .keel/ directory is created with database and config files
}

#[test]
#[ignore = "Not yet implemented"]
/// `keel init` should perform initial full map of the codebase.
fn test_init_performs_initial_map() {
    // GIVEN a directory with 50 source files
    // WHEN `keel init` is run
    // THEN all 50 files are parsed and nodes/edges are stored
}

#[test]
#[ignore = "Not yet implemented"]
/// `keel init` should complete in under 10 seconds for 50k LOC.
fn test_init_performance() {
    // GIVEN a project with ~50k lines of code
    // WHEN `keel init` is run
    // THEN it completes in under 10 seconds
}

#[test]
#[ignore = "Not yet implemented"]
/// `keel init` in a directory that already has .keel/ should return an error.
fn test_init_already_initialized() {
    // GIVEN a directory with existing .keel/ directory
    // WHEN `keel init` is run
    // THEN an error is returned indicating the project is already initialized
}

#[test]
#[ignore = "Not yet implemented"]
/// `keel init` should create a default keel.toml configuration file.
fn test_init_creates_config() {
    // GIVEN a fresh directory
    // WHEN `keel init` is run
    // THEN keel.toml is created with sensible defaults
}

#[test]
#[ignore = "Not yet implemented"]
/// `keel init` should detect the languages used in the project.
fn test_init_detects_languages() {
    // GIVEN a project with .ts, .py, and .go files
    // WHEN `keel init` is run
    // THEN the config records TypeScript, Python, and Go as detected languages
}

// ---------------------------------------------------------------------------
// Hardening tests (implemented)
// ---------------------------------------------------------------------------

#[test]
/// `keel init` creates a .keelignore file with expected default patterns.
fn test_init_creates_keelignore_with_correct_patterns() {
    let dir = setup_ts_project();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");

    assert!(
        output.status.success(),
        "keel init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // .keelignore should exist at the project root
    let ignore_path = dir.path().join(".keelignore");
    assert!(
        ignore_path.exists(),
        ".keelignore was not created by keel init"
    );

    let contents = fs::read_to_string(&ignore_path)
        .expect("Failed to read .keelignore");

    // Verify expected default patterns are present
    let expected_patterns = [
        "node_modules",
        "__pycache__",
        "target",
        "dist",
        "build",
        "vendor",
        ".venv",
    ];

    for pattern in &expected_patterns {
        assert!(
            contents.contains(pattern),
            ".keelignore missing expected pattern '{}'. Contents:\n{}",
            pattern,
            contents
        );
    }
}

#[test]
/// `keel init` installs a git pre-commit hook when .git exists.
fn test_init_installs_git_precommit_hook() {
    let dir = setup_ts_project();
    let keel = keel_bin();

    // Initialize a git repo in the temp directory so .git/hooks exists
    let git_output = Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run git init");
    assert!(
        git_output.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&git_output.stderr)
    );

    // Verify .git/hooks was created by git init
    assert!(
        dir.path().join(".git/hooks").exists(),
        ".git/hooks not created by git init"
    );

    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");

    assert!(
        output.status.success(),
        "keel init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Pre-commit hook should exist
    let hook_path = dir.path().join(".git/hooks/pre-commit");
    assert!(
        hook_path.exists(),
        ".git/hooks/pre-commit was not created by keel init"
    );

    let contents = fs::read_to_string(&hook_path)
        .expect("Failed to read pre-commit hook");

    // Hook should invoke keel compile
    assert!(
        contents.contains("keel compile"),
        "pre-commit hook should contain 'keel compile'. Contents:\n{}",
        contents
    );

    // Hook should have a shebang line
    assert!(
        contents.starts_with("#!/"),
        "pre-commit hook should start with a shebang. Contents:\n{}",
        contents
    );

    // On Unix, hook should be executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(&hook_path).unwrap();
        let mode = metadata.permissions().mode();
        assert!(
            mode & 0o111 != 0,
            "pre-commit hook should be executable, mode: {:o}",
            mode
        );
    }
}
