// Tests for `keel review --base <ref>` — the two-sided graph diff cover letter.

use std::fs;
use std::path::Path;
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

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "user.email=test@keel.dev",
            "-c",
            "user.name=keel test",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git command failed to run");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn keel(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run keel")
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// A committed, mapped repo: `lib.ts::execute` called by `main.ts::run`.
fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "src/lib.ts",
        "export function execute(cmd: number): number {\n  return cmd;\n}\n",
    );
    write(
        root,
        "src/main.ts",
        "import { execute } from './lib';\nexport function run(): number {\n  return execute(1);\n}\n",
    );

    git(root, &["init", "-q"]);
    let init = keel(root, &["init"]);
    assert!(init.status.success(), "keel init failed");
    let map = keel(root, &["map"]);
    assert!(map.status.success(), "keel map failed");

    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "base"]);
    dir
}

#[test]
fn signature_change_leads_with_the_caller_left_outside_the_diff() {
    let dir = fixture();
    let root = dir.path();
    // Only lib.ts changes — main.ts still calls the old two-arg-less shape.
    write(
        root,
        "src/lib.ts",
        "export function execute(cmd: number, dryRun: boolean): number {\n  return dryRun ? 0 : cmd;\n}\n",
    );

    let out = keel(root, &["review", "--base", "HEAD", "--llm"]);
    assert_eq!(out.status.code(), Some(0), "review must not gate");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first = stdout.lines().next().unwrap_or_default();
    assert!(
        first.contains("execute()") && first.contains("src/lib.ts"),
        "first line should name the changed contract, got: {first}"
    );
    assert!(
        first.contains("1 caller(s) outside the diff"),
        "first line should count stranded callers, got: {first}"
    );
    assert!(stdout.contains("CONTRACT signature_changed execute"));
    assert!(stdout.contains("run@src/main.ts"));
}

#[test]
fn body_only_pr_prints_nothing_and_exits_zero() {
    let dir = fixture();
    let root = dir.path();
    write(
        root,
        "src/lib.ts",
        "export function execute(cmd: number): number {\n  const out = cmd;\n  return out;\n}\n",
    );

    let out = keel(root, &["review", "--base", "HEAD", "--llm"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "",
        "a body-only PR must print nothing"
    );
}

#[test]
fn verbose_prints_the_counts_even_when_silent() {
    let dir = fixture();
    let root = dir.path();
    write(
        root,
        "src/lib.ts",
        "export function execute(cmd: number): number {\n  const out = cmd;\n  return out;\n}\n",
    );

    let out = keel(root, &["review", "--base", "HEAD", "--llm", "--verbose"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("changed their contract"),
        "verbose should still report the counts, got: {stdout}"
    );
}

#[test]
fn renamed_file_with_no_content_change_reports_moved() {
    let dir = fixture();
    let root = dir.path();
    fs::rename(root.join("src/lib.ts"), root.join("src/exec.ts")).unwrap();
    // Rename detection compares the staged deletion with the staged addition.
    git(root, &["add", "-A"]);

    let out = keel(root, &["review", "--base", "HEAD", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("review --json must emit JSON");
    let changes = parsed["changes"].as_array().unwrap();
    let moved = changes
        .iter()
        .find(|c| c["name"] == "execute")
        .expect("execute should be reported");
    assert_eq!(moved["kind"], "moved");
    assert_eq!(moved["from"], "src/lib.ts");
    assert_eq!(moved["file"], "src/exec.ts");
    assert!(
        !changes.iter().any(|c| c["kind"] == "removed"),
        "a rename must not read as add + remove: {changes:?}"
    );
}

#[test]
fn baml_and_sql_land_under_unanalyzed() {
    let dir = fixture();
    let root = dir.path();
    write(root, "migrations/001_init.sql", "CREATE TABLE t(x INT);\n");
    write(root, "baml_src/main.baml", "function Plan() -> string {}\n");
    write(root, "tests/fixtures/eval_01.json", "{\"a\": 1}\n");
    write(root, "README.md", "# docs\n");
    git(root, &["add", "-A"]);

    let out = keel(root, &["review", "--base", "HEAD", "--llm"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("UNANALYZED baml_src/main.baml [boundary]"));
    assert!(stdout.contains("UNANALYZED migrations/001_init.sql [data]"));
    assert!(stdout.contains("UNANALYZED tests/fixtures/eval_01.json [fixture]"));
    assert!(
        !stdout.contains("README.md"),
        "prose must not pad the unanalyzed list: {stdout}"
    );
}

#[test]
fn unresolvable_base_exits_two() {
    let dir = fixture();
    let out = keel(dir.path(), &["review", "--base", "no-such-ref", "--llm"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no-such-ref"));
}

/// The perf budget from the plan: < 3s on a ~54-file PR. 60 changed files here
/// stands in for the 54-file zenzy PR the budget was sized against.
///
/// The wall-clock ceiling only holds for an optimized binary — tree-sitter is
/// roughly an order of magnitude slower unoptimized, so a `cargo test` in the
/// default debug profile would fail this on parse speed alone and say nothing
/// about `review`. What holds in **both** profiles is the shape of the work:
/// review parses each changed file twice (base blob + working tree) where
/// `keel map` parses the whole repo once and also resolves and persists it, so
/// review must stay within a small constant factor of a full map. That ratio is
/// what catches an accidental O(n²) or a per-file resolver rebuild.
#[test]
fn a_sixty_file_diff_reviews_within_its_budget() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    for i in 0..60 {
        write(
            root,
            &format!("src/mod{i}.ts"),
            &format!("export function f{i}(x: number): number {{\n  return x + {i};\n}}\n"),
        );
    }
    git(root, &["init", "-q"]);
    assert!(keel(root, &["init"]).status.success());

    let map_start = Instant::now();
    assert!(keel(root, &["map"]).status.success());
    let map_elapsed = map_start.elapsed().as_secs_f64();

    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "base"]);

    for i in 0..60 {
        write(
            root,
            &format!("src/mod{i}.ts"),
            &format!("export function f{i}(x: number, y: number): number {{\n  return x + y + {i};\n}}\n"),
        );
    }

    let start = Instant::now();
    let out = keel(root, &["review", "--base", "HEAD", "--llm"]);
    let elapsed = start.elapsed().as_secs_f64();
    assert_eq!(out.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("contracts=60"),
        "all 60 contracts should be reported"
    );

    assert!(
        elapsed < map_elapsed * 3.0 + 1.0,
        "review took {elapsed:.2}s against a {map_elapsed:.2}s full map — \
         that is more than two parses per changed file plus git"
    );
    if !cfg!(debug_assertions) {
        assert!(
            elapsed < 3.0,
            "review of a 60-file diff took {elapsed:.2}s, budget is 3s"
        );
    }
}
