//! Bare-call eligibility across full map, incremental compile, and enforcement.

use std::fs;
use std::path::Path;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind};
use tempfile::TempDir;

use crate::common::{assert_no_violation, compile_json, git, keel, violations_with_code};

const DROP_REPRO: &str = "/// A guard.\npub struct Guard;\n\nimpl Drop for Guard {\n    fn drop(&mut self) {}\n}\n\n/// Takes and releases the guard.\npub fn release() {\n    let g = Guard;\n    drop(g);\n}\n";

fn fixture(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (file, source) in files {
        let path = dir.path().join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    git(dir.path(), &["init", "-q"]);
    assert!(keel(dir.path(), &["init", "--yes"]).status.success());
    assert!(keel(dir.path(), &["map"]).status.success());
    git(dir.path(), &["add", "src"]);
    git(dir.path(), &["commit", "-q", "-m", "baseline"]);
    dir
}

fn incoming_calls(dir: &Path, file: &str, name: &str, associated: bool) -> usize {
    let store = SqliteGraphStore::open(dir.join(".keel/graph.db").to_str().unwrap()).unwrap();
    let node = store
        .get_nodes_in_file(file)
        .into_iter()
        .find(|n| n.name == name && n.is_associated == associated)
        .unwrap();
    store
        .get_edges(node.id, EdgeDirection::Incoming)
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .count()
}

#[test]
fn drop_prelude_call_never_binds_drop_trait_method() {
    let dir = fixture(&[("src/lib.rs", DROP_REPRO)]);
    assert_eq!(incoming_calls(dir.path(), "src/lib.rs", "drop", true), 0);
    let edited = format!("{DROP_REPRO}\n/// Releases another guard.\npub fn release_again() {{\n    let g = Guard;\n    drop(g);\n}}\n");
    fs::write(dir.path().join("src/lib.rs"), edited).unwrap();
    let out = keel(dir.path(), &["compile", "--llm", "src/lib.rs"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "the issue repro is completely clean");
    let result = compile_json(dir.path(), "src/lib.rs");
    assert_no_violation(&result, "E005");
    assert_eq!(incoming_calls(dir.path(), "src/lib.rs", "drop", true), 0);
}

#[test]
fn qualified_constructor_and_free_function_still_report_e005() {
    let base = "/// A guard.\npub struct Guard;\nimpl Guard {\n /// Constructs a guard.\n pub fn new(a: i32) -> Self { let _ = a; Guard }\n}\n/// A free function.\npub fn free(a: i32) { let _ = a; }\n";
    let dir = fixture(&[("src/lib.rs", base)]);
    fs::write(dir.path().join("src/lib.rs"), format!("{base}\n/// Calls both functions incorrectly.\npub fn wire() {{\n Guard::new(1, 2);\n free(1, 2);\n}}\n")).unwrap();
    let result = compile_json(dir.path(), "src/lib.rs");
    let mismatches = violations_with_code(&result, "E005");
    assert_eq!(mismatches.len(), 2, "{result}");
    assert!(mismatches
        .iter()
        .any(|v| v["message"].as_str().unwrap().contains("new")));
    assert!(mismatches
        .iter()
        .any(|v| v["message"].as_str().unwrap().contains("free")));
    assert_eq!(
        keel(dir.path(), &["compile", "src/lib.rs"]).status.code(),
        Some(1)
    );
}

#[test]
fn bare_free_function_wins_over_same_named_method_in_either_order() {
    let free = "/// A free function.\npub fn run(a: i32) { let _ = a; }\n";
    let method = "impl Guard {\n /// An associated function.\n pub fn run() {}\n}\n";
    for (first, second) in [(free, method), (method, free)] {
        let base = format!("/// A guard.\npub struct Guard;\n{first}{second}\n/// Calls the free function.\npub fn wire() {{ run(1); }}\n");
        let dir = fixture(&[("src/lib.rs", &base)]);
        assert_eq!(incoming_calls(dir.path(), "src/lib.rs", "run", true), 0);
        assert_eq!(incoming_calls(dir.path(), "src/lib.rs", "run", false), 1);
        fs::write(
            dir.path().join("src/lib.rs"),
            base.replace("run(1)", "run(1, 2)"),
        )
        .unwrap();
        let result = compile_json(dir.path(), "src/lib.rs");
        assert_eq!(violations_with_code(&result, "E005").len(), 1, "{result}");
        assert_eq!(incoming_calls(dir.path(), "src/lib.rs", "run", true), 0);
        assert_eq!(incoming_calls(dir.path(), "src/lib.rs", "run", false), 1);
    }
}

#[test]
fn bare_member_name_produces_no_e001_or_e004_after_member_edits() {
    let base = "/// A guard.\npub struct Guard;\nimpl Guard {\n /// An associated function.\n pub fn run() {}\n}\n";
    let dir = fixture(&[
        ("src/guard.rs", base),
        (
            "src/caller.rs",
            "/// A caller.\npub fn wire() { run(1); }\n",
        ),
    ]);
    assert_eq!(incoming_calls(dir.path(), "src/guard.rs", "run", true), 0);
    fs::write(
        dir.path().join("src/guard.rs"),
        base.replace("fn run()", "fn run(a: i32)"),
    )
    .unwrap();
    let result = compile_json(dir.path(), "src/guard.rs");
    assert_no_violation(&result, "E001");
    fs::write(
        dir.path().join("src/guard.rs"),
        "/// A guard.\npub struct Guard {}\n",
    )
    .unwrap();
    let result = compile_json(dir.path(), "src/guard.rs");
    assert_no_violation(&result, "E004");
}

#[test]
fn python_class_body_call_survives_map_and_compile() {
    let base = "class Guard:\n    \"\"\"A guard.\"\"\"\n    def helper(x: int) -> int:\n        \"\"\"Return a value.\"\"\"\n        return x\n    value = helper(1)\n";
    let dir = fixture(&[("src/class.py", base)]);
    assert_eq!(
        incoming_calls(dir.path(), "src/class.py", "helper", true),
        1
    );
    let result = compile_json(dir.path(), "src/class.py");
    assert_no_violation(&result, "E005");
    assert_eq!(
        incoming_calls(dir.path(), "src/class.py", "helper", true),
        1
    );
    fs::write(
        dir.path().join("src/class.py"),
        base.replace("helper(1)", "helper(1, 2)"),
    )
    .unwrap();
    let result = compile_json(dir.path(), "src/class.py");
    assert_eq!(violations_with_code(&result, "E005").len(), 1, "{result}");
}
