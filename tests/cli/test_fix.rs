// Tests for `keel fix` command — violation fix plan generation

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

fn init_and_map(files: &[(&str, &str)]) -> TempDir {
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
        .unwrap();
    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

#[test]
fn test_fix_clean_project_exits_0() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["fix"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel fix");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(
        code,
        0,
        "fix on clean project should exit 0\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fix_generates_plan_for_violations() {
    let dir = init_and_map(&[
        (
            "src/caller.ts",
            "import { target } from './target';\nexport function caller(): void { target(); }\n",
        ),
        ("src/target.ts", "export function target(): void {}\n"),
    ]);
    let keel = keel_bin();

    // Break the callee to create a violation
    fs::write(
        dir.path().join("src/target.ts"),
        "export function renamed(): void {}\n",
    )
    .unwrap();

    let output = Command::new(&keel)
        .args(["fix"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel fix");

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "fix should exit 0 (plan generated) or 1, got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fix_with_file_flag() {
    let dir = init_and_map(&[
        (
            "src/a.ts",
            "export function fnA(x: number): number { return x; }\n",
        ),
        (
            "src/b.ts",
            "export function fnB(y: string): string { return y; }\n",
        ),
    ]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["fix", "--file", "src/a.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel fix --file");

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "fix --file should exit 0 or 1, got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fix_not_initialized_exits_2() {
    let dir = TempDir::new().unwrap();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["fix"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel fix");

    assert_eq!(
        output.status.code(),
        Some(2),
        "fix without init should exit 2"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("init") || stderr.contains("graph"),
        "should mention init or graph store: {stderr}"
    );
}

#[test]
fn test_fix_with_specific_hash() {
    let dir = init_and_map(&[
        (
            "src/caller.ts",
            "import { target } from './target';\nexport function caller(): void { target(); }\n",
        ),
        ("src/target.ts", "export function target(): void {}\n"),
    ]);
    let keel = keel_bin();

    // Pass a fake hash — fix should still run (just won't match any violations)
    let output = Command::new(&keel)
        .args(["fix", "AAAAAAAAAAA"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel fix with hash");

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "fix with hash filter should exit 0 or 1, got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_fix_apply_on_clean_project() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["fix", "--apply"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel fix --apply");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(
        code,
        0,
        "fix --apply on clean project should exit 0\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
