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

/// ITEM 4: an explicitly-named target that does not exist must be a hard error
/// (exit 2 + a clear stderr message), not a silent exit 0 that looks clean.
#[test]
fn test_compile_missing_explicit_file_errors() {
    let dir = init_and_map_project(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);

    let out = Command::new(keel_bin())
        .args(["compile", "src/does_not_exist.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        code, 2,
        "missing explicit file must exit 2, got {code}; stderr: {stderr}"
    );
    assert!(
        stderr.contains("file not found"),
        "stderr must name the missing file: {stderr}"
    );
}

/// ITEM 4 (counterpart): a git-deleted path under --changed must still be
/// skipped silently — the exit-2 rule is only for the explicit-user-list branch.
#[test]
fn test_compile_changed_missing_file_does_not_error() {
    let dir = init_and_map_project(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    // Delete a tracked file so `--changed` (if it were to see it) references a
    // path that no longer exists; keel must not exit 2 for that.
    fs::remove_file(dir.path().join("src/index.ts")).ok();

    let out = Command::new(keel_bin())
        .args(["compile", "--changed"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile --changed");

    let code = out.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "--changed with a deleted path must not exit 2, got {code}; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// ITEM 5: `--timeout` must never mask violations as exit 0. An unmissable-tight
/// budget (1ms) is exceeded, but the already-computed violations must still be
/// reported and the real exit code (1) returned.
#[test]
fn test_compile_timeout_still_reports_violations() {
    let dir = init_and_map_project(&[(
        "src/clean.py",
        "def clean(x: int) -> int:\n    \"\"\"Doc.\"\"\"\n    return x\n",
    )]);
    // A new file with a missing-type-hint public function → E002 ERROR.
    fs::write(
        dir.path().join("src/bad.py"),
        "def compute(value):\n    return value\n",
    )
    .unwrap();

    let out = Command::new(keel_bin())
        .args(["compile", "src/bad.py", "--timeout", "1"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile --timeout");

    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        code, 1,
        "--timeout must not mask the E002 violation as exit 0, got {code}; stdout: {stdout}"
    );
    assert!(
        stdout.contains("E002"),
        "the violation must still be reported despite the exceeded budget: {stdout}"
    );
}

// --- T1.1: bare `keel compile` (no files, no --changed, no --since) scopes
// to git-changed files instead of an unscoped full-repo walk. ---

/// `git init` + an initial commit inside `dir`, so a subsequent uncommitted
/// edit shows up in `git diff --name-only HEAD`.
fn git_init_and_commit(dir: &TempDir) {
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .expect("failed to run git");
        assert!(status.success(), "git {:?} failed", args);
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "test"]);
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "initial"]);
}

#[test]
/// A bare compile in a git repo must scope to the working-tree diff — the
/// untouched file's own (pre-existing) violation must NOT appear, only the
/// edited file's.
fn test_bare_compile_in_git_repo_scopes_to_changed_files() {
    let dir = init_and_map_project(&[
        (
            "src/a.py",
            "def a(x: int) -> int:\n    \"\"\"Doc.\"\"\"\n    return x\n",
        ),
        (
            "src/b.py",
            "def b(x: int) -> int:\n    \"\"\"Doc.\"\"\"\n    return x\n",
        ),
    ]);
    git_init_and_commit(&dir);

    // Edit only a.py — drop its docstring, a fresh (not pre-existing) E003.
    fs::write(
        dir.path().join("src/a.py"),
        "def a(x: int) -> int:\n    return x\n",
    )
    .unwrap();

    let output = Command::new(keel_bin())
        .args(["compile", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_ne!(
        output.status.code(),
        Some(2),
        "compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("src/a.py"),
        "the edited file must be analyzed: {stdout}"
    );
    assert!(
        !stdout.contains("src/b.py"),
        "the untouched file must NOT be scanned by a git-scoped bare compile: {stdout}"
    );
}

#[test]
/// A bare compile with no `.git` directory keeps the old full-repo-scan
/// default, since there is no git history to scope against — both files must
/// be analyzed even though only one was edited.
fn test_bare_compile_without_git_falls_back_to_full_scan() {
    let dir = init_and_map_project(&[
        (
            "src/a.py",
            "def a(x: int) -> int:\n    \"\"\"Doc.\"\"\"\n    return x\n",
        ),
        // b.py has a pre-existing violation (no type hints) that only a full
        // scan will surface, since it's never touched below.
        ("src/b.py", "def b(x):\n    return x\n"),
    ]);

    // Edit only a.py — drop its docstring, a fresh E003.
    fs::write(
        dir.path().join("src/a.py"),
        "def a(x: int) -> int:\n    return x\n",
    )
    .unwrap();

    let output = Command::new(keel_bin())
        .args(["compile", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_ne!(
        output.status.code(),
        Some(2),
        "compile internal error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("src/a.py"),
        "the edited file must be analyzed: {stdout}"
    );
    assert!(
        stdout.contains("src/b.py"),
        "a non-git bare compile must still scan every file (old default preserved): {stdout}"
    );
}

#[test]
fn test_compile_parses_sql_without_an_untracked_notice() {
    let dir = init_and_map_project(&[(
        "src/db.ts",
        "export function query(sql: string): string { return sql; }\n",
    )]);
    fs::create_dir_all(dir.path().join("migrations")).unwrap();
    fs::write(
        dir.path().join("migrations/001_init.sql"),
        "CREATE TABLE users (id INT);\n",
    )
    .unwrap();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["compile", "migrations/001_init.sql"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "SQL compile should pass; stderr: {stderr}"
    );
    assert!(stderr.trim().is_empty(), "unexpected notice: {stderr}");

    let discovered = Command::new(&keel)
        .args(["discover", "migrations/001_init.sql", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to discover compiled SQL");
    let json: serde_json::Value = serde_json::from_slice(&discovered.stdout).unwrap();
    assert!(json["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .any(|symbol| symbol["name"] == "users"));
}

#[test]
/// T1.5: the notice must NOT leak to documentation or config edits — those are
/// the edits where extra hook output is pure noise.
fn test_compile_no_notice_for_docs_and_config() {
    let dir = init_and_map_project(&[(
        "src/db.ts",
        "export function query(sql: string): string { return sql; }\n",
    )]);
    fs::write(dir.path().join("README.md"), "# Title\n").unwrap();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["compile", "README.md"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(
        stderr.trim().is_empty() && stdout.trim().is_empty(),
        "compiling README.md must print nothing; stdout: {stdout} stderr: {stderr}"
    );
}

#[test]
/// A `.sql` file reached via `--changed` is parsed without an unsupported-file notice.
fn test_compile_changed_parses_sql() {
    let dir = init_and_map_project(&[(
        "src/a.py",
        "def a(x: int) -> int:\n    \"\"\"Doc.\"\"\"\n    return x\n",
    )]);
    fs::create_dir_all(dir.path().join("migrations")).unwrap();
    fs::write(dir.path().join("migrations/001_init.sql"), "SELECT 1;\n").unwrap();
    git_init_and_commit(&dir);

    fs::write(
        dir.path().join("migrations/001_init.sql"),
        "ALTER TABLE users ADD COLUMN email TEXT;\n",
    )
    .unwrap();

    let output = Command::new(keel_bin())
        .args(["compile", "--changed"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(stderr.trim().is_empty(), "unexpected notice: {stderr}");
}
