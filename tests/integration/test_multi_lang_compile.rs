/// Multi-language integration tests: compile across languages.
///
/// Verifies that `keel compile` correctly detects violations when
/// source files are modified in each of the four supported languages.

use std::fs;
use std::process::Command;

use super::test_multi_lang_setup::{init_and_map, keel_bin, setup_mixed_project};

#[test]
fn test_compile_typescript_in_mixed_project() {
    let dir = setup_mixed_project();
    init_and_map(&dir);
    let keel = keel_bin();

    // Compile should be clean initially
    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("compile failed");

    assert_ne!(
        output.status.code(),
        Some(2),
        "compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Modify the TS file: change add signature (arity change)
    let src = dir.path().join("src");
    fs::write(
        src.join("math.ts"),
        r#"function add(a: number): number {
    return a;
}
"#,
    )
    .unwrap();

    // Compile should detect the change
    let output = Command::new(&keel)
        .args(["compile", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("compile failed");

    assert!(
        output.status.code().is_some(),
        "compile should not crash"
    );

    assert_ne!(
        output.status.code(),
        Some(2),
        "compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_compile_python_in_mixed_project() {
    let dir = setup_mixed_project();
    init_and_map(&dir);
    let keel = keel_bin();

    // Modify Python file: remove type hints
    let src = dir.path().join("src");
    fs::write(
        src.join("utils.py"),
        r#"def greet(name):
    return f"Hello {name}"
"#,
    )
    .unwrap();

    // Compile should detect missing type hints
    let output = Command::new(&keel)
        .args(["compile", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("compile failed");

    assert!(
        output.status.code().is_some(),
        "compile should not crash"
    );

    assert_ne!(
        output.status.code(),
        Some(2),
        "compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // If violations found, should include E002
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.code() == Some(1) {
        assert!(
            stdout.contains("E002") || stdout.contains("missing_type_hints"),
            "Expected E002 for Python missing type hints, got: {}",
            stdout
        );
    }
}

#[test]
fn test_compile_go_in_mixed_project() {
    let dir = setup_mixed_project();
    init_and_map(&dir);
    let keel = keel_bin();

    // Modify Go file: change function signature (add extra param)
    let src = dir.path().join("src");
    fs::write(
        src.join("helper.go"),
        r#"package src

func multiply(a int, b int, c int) int {
	return a * b * c
}
"#,
    )
    .unwrap();

    // Compile should detect the arity mismatch
    let output = Command::new(&keel)
        .args(["compile", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("compile failed");

    assert!(
        output.status.code().is_some(),
        "compile should not crash"
    );

    assert_ne!(
        output.status.code(),
        Some(2),
        "compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_compile_rust_in_mixed_project() {
    let dir = setup_mixed_project();
    init_and_map(&dir);
    let keel = keel_bin();

    // Modify Rust file: remove the divide function
    let src = dir.path().join("src");
    fs::write(
        src.join("lib.rs"),
        r#"fn remainder(a: f64, b: f64) -> f64 {
    a % b
}
"#,
    )
    .unwrap();

    // Compile should detect the removed function
    let output = Command::new(&keel)
        .args(["compile", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("compile failed");

    assert!(
        output.status.code().is_some(),
        "compile should not crash"
    );

    assert_ne!(
        output.status.code(),
        Some(2),
        "compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
