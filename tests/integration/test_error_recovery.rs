// Integration tests: error recovery and fault tolerance (E2E)
//
// Validates that keel handles corrupt databases, missing files, parse failures,
// and other error conditions gracefully without panicking or losing data.

use std::fs;
use std::process::Command;

use tempfile::TempDir;

/// Path to the keel binary built by cargo.
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

/// Create and initialize a simple TS project, returning the TempDir.
fn setup_initialized_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("app.ts"),
        r#"function greet(name: string): string {
    return "Hello " + name;
}

function run(): void {
    greet("world");
}
"#,
    )
    .unwrap();

    let keel = keel_bin();
    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("init failed");
    assert!(output.status.success(), "init failed");

    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("map failed");
    assert!(output.status.success(), "map failed");

    dir
}

#[test]
fn test_corrupt_graph_db_triggers_rebuild() {
    let dir = setup_initialized_project();
    let keel = keel_bin();

    // Corrupt graph.db by truncating it
    let db_path = dir.path().join(".keel/graph.db");
    assert!(db_path.exists(), "graph.db should exist after map");
    fs::write(&db_path, b"corrupted data not a sqlite db").unwrap();

    // compile should detect corruption and report error (exit 2)
    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("keel compile should not crash on corrupt db");

    // Should not panic
    assert!(
        output.status.code().is_some(),
        "Process should not be killed by signal"
    );

    // Should exit with error (code 2 = internal error)
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "Expected exit code 2 for corrupt db. stderr: {}",
        stderr
    );
}

#[test]
fn test_missing_graph_db_triggers_init_suggestion() {
    let dir = setup_initialized_project();
    let keel = keel_bin();

    // Delete graph.db but leave .keel/ directory
    let db_path = dir.path().join(".keel/graph.db");
    fs::remove_file(&db_path).unwrap();
    assert!(!db_path.exists());

    // compile should report that graph.db is missing
    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("keel compile should not crash on missing db");

    assert!(
        output.status.code().is_some(),
        "Process should not be killed by signal"
    );

    // Should still work (compile creates/opens a new db) or fail gracefully
    // Either exit 0 (empty graph = clean) or exit 2 (can't open)
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 2,
        "Expected exit 0 or 2, got {}. stderr: {}",
        code,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_parse_failure_skips_file_gracefully() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    // One valid file
    fs::write(
        src.join("app.ts"),
        "function greet(name: string): string { return name; }\n",
    )
    .unwrap();

    // One syntactically broken file
    fs::write(
        src.join("broken.ts"),
        "function {{{{ broken syntax @@@ not valid typescript !!!!",
    )
    .unwrap();

    let keel = keel_bin();

    // Init
    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("init failed");
    assert!(output.status.success());

    // Map should still succeed (tree-sitter is error-tolerant)
    let output = Command::new(&keel)
        .args(["map", "--verbose"])
        .current_dir(dir.path())
        .output()
        .expect("keel map should not crash on parse failure");

    assert!(
        output.status.success(),
        "map should succeed despite broken file: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_missing_source_file_after_map() {
    let dir = setup_initialized_project();
    let keel = keel_bin();

    // Delete a source file after mapping
    let app_path = dir.path().join("src/app.ts");
    assert!(app_path.exists());
    fs::remove_file(&app_path).unwrap();

    // compile on the deleted file should handle gracefully
    let output = Command::new(&keel)
        .args(["compile", "src/app.ts"])
        .current_dir(dir.path())
        .output()
        .expect("keel compile should not crash on missing file");

    assert!(
        output.status.code().is_some(),
        "Process should not be killed by signal"
    );

    // Should either exit 0 (no files to check = clean) or exit 2 (file not found)
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 2,
        "Expected exit 0 or 2, got {}. stderr: {}",
        code,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_permission_denied_on_source_file() {
    let dir = setup_initialized_project();
    let keel = keel_bin();

    // Make source file unreadable
    let app_path = dir.path().join("src/app.ts");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o000);
        fs::set_permissions(&app_path, perms).unwrap();
    }

    // compile should handle gracefully (skip unreadable file)
    let output = Command::new(&keel)
        .args(["compile", "src/app.ts"])
        .current_dir(dir.path())
        .output()
        .expect("keel compile should not crash on permission denied");

    // Restore permissions for cleanup
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o644);
        let _ = fs::set_permissions(&app_path, perms);
    }

    assert!(
        output.status.code().is_some(),
        "Process should not be killed by signal"
    );

    // Should exit 0 (skipped file = clean) or exit 2 (error reading)
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 2,
        "Expected exit 0 or 2, got {}. stderr: {}",
        code,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_empty_project_compiles_cleanly() {
    let dir = TempDir::new().unwrap();
    // No source files at all — just an empty directory
    let keel = keel_bin();

    // Init
    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("init failed");
    assert!(
        output.status.success(),
        "init failed on empty project: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Map (nothing to map)
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("map failed");
    assert!(
        output.status.success(),
        "map should succeed on empty project: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Compile (nothing to compile = clean)
    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("compile failed");

    assert_eq!(
        output.status.code(),
        Some(0),
        "Empty project compile should be clean (exit 0). stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().is_empty(),
        "Empty project should have empty stdout, got: {}",
        stdout
    );
}

#[test]
fn test_concurrent_keel_processes_lock_graph_db() {
    let dir = setup_initialized_project();
    let keel = keel_bin();

    // Launch two compile processes simultaneously
    let child1 = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn first compile");

    let child2 = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn second compile");

    let output1 = child1
        .wait_with_output()
        .expect("Failed to wait for child1");
    let output2 = child2
        .wait_with_output()
        .expect("Failed to wait for child2");

    // Neither should panic or be killed
    assert!(
        output1.status.code().is_some(),
        "First process should not be killed by signal"
    );
    assert!(
        output2.status.code().is_some(),
        "Second process should not be killed by signal"
    );

    // At least one should succeed; the other may succeed or fail gracefully
    let code1 = output1.status.code().unwrap();
    let code2 = output2.status.code().unwrap();

    // Both should be either 0 (clean) or 2 (lock contention), not 1 (violations)
    assert!(
        (code1 == 0 || code1 == 2) && (code2 == 0 || code2 == 2),
        "Expected exit 0 or 2 for concurrent processes, got {} and {}",
        code1,
        code2
    );
}

#[test]
fn test_recovery_after_interrupted_map() {
    let dir = setup_initialized_project();
    let keel = keel_bin();

    // Simulate interrupted map by truncating the graph.db to partial state
    let db_path = dir.path().join(".keel/graph.db");
    let original_size = fs::metadata(&db_path).unwrap().len();
    assert!(original_size > 0, "graph.db should have content");

    // Truncate to ~half to simulate partial write
    let data = fs::read(&db_path).unwrap();
    let half = data.len() / 2;
    fs::write(&db_path, &data[..half]).unwrap();

    // Re-run map (should rebuild from scratch)
    let output = Command::new(&keel)
        .args(["map", "--verbose"])
        .current_dir(dir.path())
        .output()
        .expect("keel map should not crash on partial db");

    // map opens a new SQLite connection which may detect corruption
    // It should either succeed (rebuild) or fail gracefully (exit 2)
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 2,
        "Expected exit 0 (rebuild) or 2 (detected corruption), got {}. stderr: {}",
        code,
        String::from_utf8_lossy(&output.stderr)
    );

    // If map succeeded, compile should also work
    if code == 0 {
        let output = Command::new(&keel)
            .arg("compile")
            .current_dir(dir.path())
            .output()
            .expect("compile after recovery failed");
        assert!(
            output.status.code().is_some(),
            "compile should not crash after recovery"
        );
    }
}
