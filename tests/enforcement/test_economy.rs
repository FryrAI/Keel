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

use crate::common::{
    assert_no_violation, compile_json, find_violation, init_project, keel, mapped_project,
};

/// Lower the W007 line budget so the oversized-file fixture stays small.
fn set_max_file_lines(dir: &Path, n: u64) {
    let cfg_path = dir.join(".keel/keel.json");
    let raw = fs::read_to_string(&cfg_path).expect("keel.json exists after init");
    let mut cfg: serde_json::Value = serde_json::from_str(&raw).expect("keel.json is valid JSON");
    cfg["enforce"]["max_file_lines"] = serde_json::json!(n);
    fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
}

/// W005: a private (non-exported) function nothing calls, present in the graph
/// at map time, must surface as a WARNING with confidence 0.7.
#[test]
fn test_w005_dead_code_surfaces() {
    let dir = mapped_project(&[(
        "src/util.ts",
        // `deadHelper` is module-private (not exported) and called nowhere, so
        // it has zero incoming call edges after `keel map` -> W005.
        "function deadHelper(x: number): number {\n  return x + 1;\n}\n",
    )]);

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
    let dir = mapped_project(&[
        (
            "src/alpha.ts",
            &format!("function computeAlpha(x: number): number {{\n{body}}}\n"),
        ),
        (
            "src/beta.ts",
            &format!("function computeBeta(x: number): number {{\n{body}}}\n"),
        ),
    ]);

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

/// A Python function decorated with `@register(...)` is handed to the
/// decorator/framework, never called by name — keel must not flag it dead
/// even with zero call edges. `__all__` deliberately excludes it so the
/// exemption under test is `is_decorated`, not the unrelated public-export one.
#[test]
fn test_w005_skips_decorated_python_function() {
    let dir = mapped_project(&[(
        "src/handlers.py",
        "__all__ = [\"public_api\"]\n\n\
         def register(name):\n    def deco(fn):\n        return fn\n    return deco\n\n\
         @register(\"evt\")\ndef handler():\n    pass\n\n\
         def public_api():\n    return 1\n",
    )]);

    let result = compile_json(dir.path(), "src/handlers.py");
    assert_no_violation(&result, "W005");
}

/// A function marked `# keel:keep` is the language-agnostic escape hatch for
/// dynamic dispatch (`globals()[name]()`) the graph cannot see through —
/// keel must not flag it dead. `__all__` again excludes it from the
/// unrelated public-export exemption.
#[test]
fn test_w005_skips_keel_keep_marked_function() {
    let dir = mapped_project(&[(
        "src/dispatch.py",
        "__all__ = [\"public_api\"]\n\n\
         # keel:keep\n\
         def dynamic_handler():\n    pass\n\n\
         def public_api():\n    return 1\n",
    )]);

    let result = compile_json(dir.path(), "src/dispatch.py");
    assert_no_violation(&result, "W005");
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
    let dir = mapped_project(&[(
        "src/engine_tests_helper.rs",
        "fn build_row(x: i32) -> i32 {\n    x + 1\n}\n",
    )]);

    let result = compile_json(dir.path(), "src/engine_tests_helper.rs");
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
    let dir = mapped_project(&[(
        "src/rows.rs",
        "fn make_item(x: i32) -> i32 {\n    x + 1\n}\n\n\
         /// Build the rows.\n\
         pub fn build() -> Vec<i32> {\n    vec![make_item(1), make_item(2)]\n}\n",
    )]);

    let result = compile_json(dir.path(), "src/rows.rs");
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
