// Tests for `keel init` command (Spec 007 - CLI Commands)

use std::fs;
use std::process::Command;

use tempfile::TempDir;

/// Path to the keel binary built by cargo.
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
/// `keel init` in a fresh directory should create .keel/ directory structure.
fn test_init_creates_keel_directory() {
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

    assert!(
        dir.path().join(".keel").exists(),
        ".keel/ directory not created"
    );
    assert!(
        dir.path().join(".keel/graph.db").exists(),
        ".keel/graph.db not created"
    );
    assert!(
        dir.path().join(".keel/cache").exists(),
        ".keel/cache/ not created"
    );
}

#[test]
/// `keel init` should perform initial full map of the codebase.
fn test_init_performs_initial_map() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    // Create multiple source files
    for i in 0..10 {
        fs::write(
            src.join(format!("mod_{i}.ts")),
            format!("export function func_{i}(x: number): number {{ return x + {i}; }}\n"),
        )
        .unwrap();
    }

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

    // Database should have been populated with nodes
    assert!(
        dir.path().join(".keel/graph.db").exists(),
        "graph.db should exist after init with mapping"
    );
    let db_size = fs::metadata(dir.path().join(".keel/graph.db"))
        .unwrap()
        .len();
    // A mapped database with 10 files should be larger than an empty schema
    assert!(
        db_size > 4096,
        "graph.db too small ({db_size} bytes) — likely not mapped"
    );
}

#[test]
/// `keel init` should complete in under 10 seconds for 50k LOC.
fn test_init_performance() {
    use std::fmt::Write;
    use std::time::Instant;

    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    // Generate ~50k LOC across 500 files (100 LOC each)
    for i in 0..500 {
        let mut content = String::new();
        for j in 0..10 {
            writeln!(
                content,
                "export function func_{i}_{j}(x: number): number {{\n  \
                 const a = x + 1;\n  const b = x + 2;\n  const c = x + 3;\n  \
                 const d = x + 4;\n  const e = x + 5;\n  return a + b + c + d + e;\n}}\n"
            )
            .unwrap();
        }
        fs::write(src.join(format!("mod_{i}.ts")), &content).unwrap();
    }

    let keel = keel_bin();
    let start = Instant::now();
    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");
    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "keel init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed.as_secs() < 10,
        "keel init took {:?} — exceeds 10s target for 50k LOC",
        elapsed
    );
}

#[test]
/// `keel init` in a directory that already has .keel/ should return an error.
fn test_init_already_initialized() {
    let dir = setup_ts_project();
    let keel = keel_bin();

    // First init should succeed
    let first = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");
    assert!(first.status.success(), "first keel init should succeed");

    // Second init should fail (already initialized)
    let second = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");

    assert!(
        !second.status.success(),
        "second keel init should fail when .keel/ already exists"
    );
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        stderr.to_lowercase().contains("already")
            || stderr.to_lowercase().contains("initialized")
            || stderr.to_lowercase().contains("exists"),
        "error message should indicate already initialized, got: {stderr}"
    );
}

#[test]
/// `keel init` should create a default keel.json configuration file.
fn test_init_creates_config() {
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

    // keel.json config should exist inside .keel/
    let config_path = dir.path().join(".keel/keel.json");
    assert!(config_path.exists(), "keel.json config not created");

    let contents = fs::read_to_string(&config_path).expect("Failed to read keel.json");
    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&contents).expect("keel.json is not valid JSON");
    // Should contain a languages array
    assert!(
        parsed.get("languages").is_some(),
        "keel.json should contain 'languages' key"
    );
}

#[test]
/// `keel init` should detect the languages used in the project.
fn test_init_detects_languages() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    // Create files in multiple languages
    fs::write(
        src.join("app.ts"),
        "function greet(name: string): string { return name; }\n",
    )
    .unwrap();
    fs::write(
        src.join("main.py"),
        "def greet(name: str) -> str:\n    return name\n",
    )
    .unwrap();
    fs::write(
        src.join("main.go"),
        "package main\n\nfunc greet(name string) string {\n\treturn name\n}\n",
    )
    .unwrap();

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

    let config_path = dir.path().join(".keel/keel.json");
    let contents = fs::read_to_string(&config_path).expect("Failed to read keel.json");
    let parsed: serde_json::Value =
        serde_json::from_str(&contents).expect("keel.json is not valid JSON");

    let languages = parsed["languages"]
        .as_array()
        .expect("languages should be an array");
    let lang_strs: Vec<&str> = languages.iter().filter_map(|v| v.as_str()).collect();

    // Should detect at least TypeScript and Python (Go detection depends on implementation)
    assert!(
        lang_strs
            .iter()
            .any(|l| l.to_lowercase().contains("typescript") || *l == "ts"),
        "should detect TypeScript, found: {lang_strs:?}"
    );
    assert!(
        lang_strs
            .iter()
            .any(|l| l.to_lowercase().contains("python") || *l == "py"),
        "should detect Python, found: {lang_strs:?}"
    );
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

    let contents = fs::read_to_string(&ignore_path).expect("Failed to read .keelignore");

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

    let contents = fs::read_to_string(&hook_path).expect("Failed to read pre-commit hook");

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
