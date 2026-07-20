//! Fixture-driven integration tests for the v0.5 "economy" warnings, driven
//! through the real `keel` binary (init -> map -> compile --json).
//!
//! Unlike the in-process, hand-seeded engine tests in
//! `crates/keel-enforce/src/violations_economy_tests.rs`, these exercise the
//! whole pipeline — the parser assigns `is_public`/body text, `keel map`
//! populates the graph and body index, and `keel compile` surfaces the
//! violations in its JSON output. They are the end-to-end proof that each code
//! genuinely fires (with the documented severity + confidence) against
//! on-disk source, past the cfg(test)/entrypoint/single-module exemptions.
//!
//! - W005 dead_code — private function with no callers (confidence 0.7)
//! - W006 duplicate_implementation — two files, one shared body (confidence 0.85)
//! - W007 oversized_file — a file that grew past the line budget (confidence 0.8)

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

/// Locate the `keel` binary next to the test executable, falling back to the
/// workspace `target/debug/keel` (built by CI / a prior `cargo build`).
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

/// Run a keel subcommand in `dir` and assert it did not hit an internal error.
fn keel(dir: &Path, args: &[&str]) -> std::process::Output {
    let out = Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to run `keel {}`: {e}", args.join(" ")));
    assert_ne!(
        out.status.code(),
        Some(2),
        "`keel {}` hit an internal error:\nstdout: {}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    out
}

/// `keel compile <file> --json` -> parsed CompileResult value.
fn compile_json(dir: &Path, file: &str) -> serde_json::Value {
    let out = keel(dir, &["compile", file, "--json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "compile --json did not emit JSON ({e}):\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// Find the first violation with `code` across the errors + warnings arrays.
fn find_violation(result: &serde_json::Value, code: &str) -> serde_json::Value {
    result["errors"]
        .as_array()
        .into_iter()
        .chain(result["warnings"].as_array())
        .flatten()
        .find(|v| v["code"] == code)
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "expected a {code} violation, got:\nerrors: {}\nwarnings: {}",
                result["errors"], result["warnings"]
            )
        })
}

/// `keel compile <file> --json`, tolerating a clean compile. A file with zero
/// errors AND zero warnings prints nothing (the documented clean-compile
/// contract), so empty stdout maps to an empty errors/warnings result rather
/// than a parse panic. Used by the absence assertions below.
fn compile_json_lenient(dir: &Path, file: &str) -> serde_json::Value {
    let out = keel(dir, &["compile", file, "--json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return serde_json::json!({ "errors": [], "warnings": [] });
    }
    serde_json::from_str(trimmed).unwrap_or_else(|e| {
        panic!(
            "compile --json did not emit JSON ({e}):\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// Assert no violation with `code` appears in the errors + warnings arrays.
fn assert_no_violation(result: &serde_json::Value, code: &str) {
    let hit = result["errors"]
        .as_array()
        .into_iter()
        .chain(result["warnings"].as_array())
        .flatten()
        .any(|v| v["code"] == code);
    assert!(
        !hit,
        "expected no {code} violation, got:\nerrors: {}\nwarnings: {}",
        result["errors"], result["warnings"]
    );
}

/// Lower the W007 line budget so the oversized-file fixture stays small.
fn set_max_file_lines(dir: &Path, n: u64) {
    let cfg_path = dir.join(".keel/keel.json");
    let raw = fs::read_to_string(&cfg_path).expect("keel.json exists after init");
    let mut cfg: serde_json::Value = serde_json::from_str(&raw).expect("keel.json is valid JSON");
    cfg["enforce"]["max_file_lines"] = serde_json::json!(n);
    fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
}

/// Create a project dir with the given files and run `keel init`.
fn init_project(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (rel, content) in files {
        let full = dir.path().join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, content).unwrap();
    }
    let out = keel(dir.path(), &["init"]);
    assert!(
        out.status.success(),
        "keel init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

/// W005: a private (non-exported) function nothing calls, present in the graph
/// at map time, must surface as a WARNING with confidence 0.7.
#[test]
fn test_w005_dead_code_surfaces() {
    let dir = init_project(&[(
        "src/util.ts",
        // `deadHelper` is module-private (not exported) and called nowhere, so
        // it has zero incoming call edges after `keel map` -> W005.
        "function deadHelper(x: number): number {\n  return x + 1;\n}\n",
    )]);
    keel(dir.path(), &["map"]);

    let result = compile_json(dir.path(), "src/util.ts");
    let v = find_violation(&result, "W005");
    assert_eq!(v["severity"], "WARNING");
    assert_eq!(v["category"], "dead_code");
    assert_eq!(v["confidence"].as_f64().unwrap(), 0.7);
}

/// W006: two files whose functions share an identical body (longer than the
/// indexed-body minimum) must surface as a WARNING with confidence 0.85.
#[test]
fn test_w006_duplicate_implementation_surfaces() {
    // Identical body, comfortably over MIN_DUPLICATE_BODY_LEN (60 normalized
    // chars) so it is indexed at map time and matched cross-file at compile.
    let body = "  const a = x + 1;\n  const b = a * 2;\n  const c = b - 3;\n  \
                const d = c + a;\n  return a + b + c + d;\n";
    let dir = init_project(&[
        (
            "src/alpha.ts",
            &format!("function computeAlpha(x: number): number {{\n{body}}}\n"),
        ),
        (
            "src/beta.ts",
            &format!("function computeBeta(x: number): number {{\n{body}}}\n"),
        ),
    ]);
    keel(dir.path(), &["map"]);

    let result = compile_json(dir.path(), "src/beta.ts");
    let v = find_violation(&result, "W006");
    assert_eq!(v["severity"], "WARNING");
    assert_eq!(v["category"], "duplicate_implementation");
    assert_eq!(v["confidence"].as_f64().unwrap(), 0.85);
}

/// W007: a file mapped small then grown past the (lowered) line budget must
/// surface as a WARNING with confidence 0.8.
#[test]
fn test_w007_oversized_file_surfaces() {
    // Start small (under the budget we set below), map, then grow past it.
    let small = gen_ts_functions(0, 3);
    let dir = init_project(&[("src/grow.ts", &small)]);
    set_max_file_lines(dir.path(), 30);
    keel(dir.path(), &["map"]);

    // Grow well past both the 30-line budget and the stored extent.
    let grown = gen_ts_functions(0, 25);
    fs::write(dir.path().join("src/grow.ts"), &grown).unwrap();

    let result = compile_json(dir.path(), "src/grow.ts");
    let v = find_violation(&result, "W007");
    assert_eq!(v["severity"], "WARNING");
    assert_eq!(v["category"], "oversized_file");
    assert_eq!(v["confidence"].as_f64().unwrap(), 0.8);
}

/// Issue #45 (Part 1): a private helper in an `engine_tests_*.rs` split-file
/// (the `#[path = "..._tests.rs"]`-included convention) must be recognized as
/// living in a test file, so it never fires W005/W002. When each split file is
/// parsed standalone the parent's `#[cfg(test)] mod` is invisible, so the
/// helper looks like plain production code — only the file-name rule saves it.
#[test]
fn test_w005_skips_engine_tests_split_files() {
    // Bare private helper, genuinely uncalled — the `engine_tests_*.rs`
    // basename is the only thing that must suppress W005 here.
    let dir = init_project(&[(
        "src/engine_tests_helper.rs",
        "fn build_row(x: i32) -> i32 {\n    x + 1\n}\n",
    )]);
    keel(dir.path(), &["map"]);

    let result = compile_json_lenient(dir.path(), "src/engine_tests_helper.rs");
    assert_no_violation(&result, "W005");
    assert_no_violation(&result, "W002");
}

/// Issue #45 (Part 2): a private helper called ONLY inside a macro token tree
/// (`vec![...]`) is real usage, but tree-sitter leaves macro bodies unparsed,
/// so pre-fix the reference is invisible and the helper fires a false W005.
/// The macro-body query pattern captures it as a Value reference, suppressing
/// the warning at compile time.
#[test]
fn test_w005_skips_helper_used_only_in_macro_body() {
    let dir = init_project(&[(
        "src/rows.rs",
        "fn make_item(x: i32) -> i32 {\n    x + 1\n}\n\n\
         /// Build the rows.\n\
         pub fn build() -> Vec<i32> {\n    vec![make_item(1), make_item(2)]\n}\n",
    )]);
    keel(dir.path(), &["map"]);

    let result = compile_json_lenient(dir.path(), "src/rows.rs");
    assert_no_violation(&result, "W005");
}

/// Generate `count` distinct, spaced-out TS functions so line numbers grow
/// predictably (4 lines each: signature, body, close brace, blank).
fn gen_ts_functions(start: usize, count: usize) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    for i in start..start + count {
        write!(
            s,
            "function fn_{i}(x: number): number {{\n  return x + {i};\n}}\n\n"
        )
        .unwrap();
    }
    s
}
