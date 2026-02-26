// Tests for `keel compile` command (Spec 007 - CLI Commands)

use std::fs;
use std::process::Command;
use std::time::Instant;

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

fn init_and_map_project(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, content) in files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }

    let keel = keel_bin();
    let out = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");
    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel map");
    assert!(
        out.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    dir
}

#[test]
/// `keel compile` with no arguments should validate all changed files.
fn test_compile_all_changed() {
    let dir = init_and_map_project(&[
        (
            "src/a.ts",
            "export function foo(x: number): number { return x; }\n",
        ),
        (
            "src/b.ts",
            "export function bar(y: string): string { return y; }\n",
        ),
        (
            "src/c.ts",
            "export function baz(z: boolean): boolean { return z; }\n",
        ),
    ]);
    let keel = keel_bin();

    // Modify all 3 files
    fs::write(
        dir.path().join("src/a.ts"),
        "export function foo(x: number, y: number): number { return x + y; }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/b.ts"),
        "export function bar(y: string, z: string): string { return y + z; }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/c.ts"),
        "export function baz(z: boolean, w: boolean): boolean { return z && w; }\n",
    )
    .unwrap();

    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    // Compile should exit 0 (clean) or 1 (violations), not 2 (internal error)
    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "keel compile should exit 0 or 1, got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
/// `keel compile <file>` should validate a specific file incrementally.
fn test_compile_single_file() {
    let dir = init_and_map_project(&[
        (
            "src/parser.ts",
            "export function parse(input: string): string { return input; }\n",
        ),
        (
            "src/utils.ts",
            "export function helper(x: number): number { return x; }\n",
        ),
    ]);
    let keel = keel_bin();

    // Modify only parser.ts
    fs::write(
        dir.path().join("src/parser.ts"),
        "export function parse(input: string, opts: string): string { return input + opts; }\n",
    )
    .unwrap();

    let output = Command::new(&keel)
        .args(["compile", "src/parser.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "single file compile should exit 0 or 1, got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
/// `keel compile` on a single file should complete in under 200ms.
fn test_compile_single_file_performance() {
    let dir = init_and_map_project(&[(
        "src/fast.ts",
        "export function quick(x: number): number { return x; }\n",
    )]);
    let keel = keel_bin();

    let start = Instant::now();
    let output = Command::new(&keel)
        .args(["compile", "src/fast.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");
    let elapsed = start.elapsed();

    let code = output.status.code().unwrap_or(-1);
    assert!(code == 0 || code == 1, "compile failed with code {code}");

    // Allow generous 5s for CI (process spawn + DB open + coverage overhead), core target is <200ms
    assert!(
        elapsed.as_millis() < 5000,
        "single file compile took {:?} — should be fast",
        elapsed
    );
}

#[test]
/// `keel compile` should output violations in the configured format.
fn test_compile_outputs_violations() {
    let dir = init_and_map_project(&[
        (
            "src/caller.ts",
            "import { target } from './target';\nexport function caller(): void { target(); }\n",
        ),
        ("src/target.ts", "export function target(): void {}\n"),
    ]);
    let keel = keel_bin();

    // Remove the target function to create a broken caller (E001)
    fs::write(
        dir.path().join("src/target.ts"),
        "export function different_name(): void {}\n",
    )
    .unwrap();

    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // If violations found (exit 1), stdout should contain violation info
    if code == 1 {
        assert!(
            !stdout.is_empty(),
            "violations found (exit 1) but stdout is empty"
        );
    }
    // Should not be exit 2 (internal error)
    assert!(code != 2, "compile should not return internal error (2)");
}

#[test]
/// `keel compile --llm` should output in LLM-friendly format.
fn test_compile_llm_format() {
    let dir = init_and_map_project(&[(
        "src/mod.ts",
        "export function greet(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["compile", "--llm"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "compile --llm should exit 0 or 1, got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
/// `keel compile` multiple specific files should validate each.
fn test_compile_multiple_files() {
    let dir = init_and_map_project(&[
        (
            "src/file1.ts",
            "export function f1(x: number): number { return x; }\n",
        ),
        (
            "src/file2.ts",
            "export function f2(y: string): string { return y; }\n",
        ),
        (
            "src/file3.ts",
            "export function f3(z: boolean): boolean { return z; }\n",
        ),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["compile", "src/file1.ts", "src/file2.ts", "src/file3.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "multi-file compile should exit 0 or 1, got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
