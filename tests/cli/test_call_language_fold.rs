//! Round-1 regressions for fresh locals, boundary calls, and Tier-3 admission.

use std::fs;
use std::path::Path;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};
use protobuf::Message;
use scip::types::{Document, Index, Occurrence};

use super::test_bare_call_binding::{fixture, incoming_calls};
use crate::common::{assert_no_violation, keel, violations_with_code};

const PYTHON_SYM: &str =
    "def sym(name: str) -> str:\n    \"\"\"Local symbol.\"\"\"\n    return name\n";

fn compile(root: &Path, file: &str, expected_exit: i32) -> serde_json::Value {
    let out = keel(root, &["compile", file, "--json"]);
    assert_eq!(
        out.status.code(),
        Some(expected_exit),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if out.stdout.is_empty() {
        serde_json::json!({"errors": [], "warnings": []})
    } else {
        serde_json::from_slice(&out.stdout).unwrap()
    }
}

fn boundary_caller(language: &str, wrong_arity: bool) {
    let dir = fixture(&[
        (
            "src/base.py",
            "def baseline() -> None:\n    \"\"\"Baseline.\"\"\"\n    pass\n",
        ),
        (
            "baml_src/main.baml",
            "function ExtractResume(text: string) -> string {\n  client GPT4\n}\n",
        ),
    ]);
    let args = if wrong_arity {
        "\"text\", \"extra\""
    } else {
        "\"text\""
    };
    let (file, source) = if language == "python" {
        ("src/new.py", format!("from baml_client import ExtractResume\ndef wire() -> str:\n    \"\"\"Fresh boundary call.\"\"\"\n    return ExtractResume({args})\n"))
    } else {
        ("src/new.ts", format!("import {{ ExtractResume }} from '../baml_client';\n/** Fresh boundary call. */\nexport function wire(): string {{ return ExtractResume({args}); }}\n"))
    };
    fs::write(dir.path().join(file), source).unwrap();
    let result = compile(dir.path(), file, i32::from(wrong_arity));
    assert_eq!(
        violations_with_code(&result, "E005").len(),
        usize::from(wrong_arity),
        "{result}"
    );
    assert_eq!(
        incoming_calls(dir.path(), "baml_src/main.baml", "ExtractResume", false),
        1
    );
    let store =
        SqliteGraphStore::open(dir.path().join(".keel/graph.db").to_str().unwrap()).unwrap();
    let wire = store
        .get_nodes_in_file(file)
        .into_iter()
        .find(|n| n.name == "wire")
        .unwrap();
    let edges: Vec<_> = store
        .get_edges(wire.id, EdgeDirection::Outgoing)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect();
    assert_eq!(edges.len(), 1);
    assert_eq!(
        edges[0].confidence,
        if language == "python" {
            keel_core::confidence::CROSS_FILE_HEURISTIC
        } else {
            0.85
        }
    );
}

#[test]
fn fresh_python_named_baml_import_keeps_target_and_confidence() {
    boundary_caller("python", false);
}

#[test]
fn fresh_typescript_named_baml_import_keeps_target_and_confidence() {
    boundary_caller("typescript", false);
}

#[test]
fn fresh_python_named_baml_import_still_reports_wrong_arity() {
    boundary_caller("python", true);
}

#[test]
fn fresh_typescript_named_baml_import_still_reports_wrong_arity() {
    boundary_caller("typescript", true);
}

fn fresh_local(source: &str, associated: bool, expected_calls: usize) {
    let dir = fixture(&[("tools/other.py", "def sym(a: str, b: str, c: str) -> str:\n    \"\"\"Other symbol.\"\"\"\n    return a + b + c\n"), ("src/base.py", "")]);
    fs::write(dir.path().join("tools/new.py"), source).unwrap();
    let result = compile(dir.path(), "tools/new.py", 0);
    assert_no_violation(&result, "E005");
    assert_eq!(
        incoming_calls(dir.path(), "tools/other.py", "sym", false),
        0
    );
    assert_eq!(
        incoming_calls(dir.path(), "tools/new.py", "sym", associated),
        expected_calls
    );
    assert!(keel(dir.path(), &["map"]).status.success());
    assert_eq!(
        incoming_calls(dir.path(), "tools/new.py", "sym", associated),
        expected_calls
    );
    assert_eq!(
        incoming_calls(dir.path(), "tools/other.py", "sym", false),
        0
    );
}

#[test]
fn fresh_function_body_uses_parsed_local_before_stored_foreign_name() {
    fresh_local(&format!("{PYTHON_SYM}\ndef main() -> str:\n    \"\"\"Call locally.\"\"\"\n    return sym(\"run\")\n"), false, 1);
}

#[test]
fn fresh_class_body_uses_parsed_class_local_before_stored_foreign_name() {
    fresh_local("class Guard:\n    def sym(name: str) -> str:\n        \"\"\"Class local.\"\"\"\n        return name\n    X = sym(\"run\")\n", true, 1);
}

#[test]
fn fresh_module_call_to_local_method_uses_other_file_for_arity_only() {
    let dir = fixture(&[("tools/other.py", "def sym(a: str, b: str, c: str) -> str:\n    \"\"\"Other symbol.\"\"\"\n    return a + b + c\n"), ("src/base.py", "")]);
    fs::write(dir.path().join("tools/new.py"), "class Guard:\n    def sym(self, name: str) -> str:\n        \"\"\"Method.\"\"\"\n        return name\nX = sym(\"run\")\n").unwrap();
    let result = compile(dir.path(), "tools/new.py", 1);
    assert_eq!(violations_with_code(&result, "E005").len(), 1, "{result}");
    assert_eq!(incoming_calls(dir.path(), "tools/new.py", "sym", true), 0);
    assert_eq!(
        incoming_calls(dir.path(), "tools/other.py", "sym", false),
        0
    );
    assert!(keel(dir.path(), &["map"]).status.success());
    assert_eq!(incoming_calls(dir.path(), "tools/new.py", "sym", true), 0);
    assert_eq!(
        incoming_calls(dir.path(), "tools/other.py", "sym", false),
        0
    );
}

#[test]
fn mapped_module_call_edge_survives_recompile() {
    let source = format!("{PYTHON_SYM}\nX = sym(\"run\")\n");
    let dir = fixture(&[("src/local.py", &source)]);
    for _ in 0..2 {
        compile(dir.path(), "src/local.py", 0);
        let store =
            SqliteGraphStore::open(dir.path().join(".keel/graph.db").to_str().unwrap()).unwrap();
        let module = store
            .get_nodes_in_file("src/local.py")
            .into_iter()
            .find(|n| n.kind == NodeKind::Module)
            .unwrap();
        let calls: Vec<_> = store
            .get_edges(module.id, EdgeDirection::Outgoing)
            .into_iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .collect();
        assert_eq!(calls.len(), 1);
        let target = store.get_node_by_id(calls[0].target_id).unwrap();
        assert_eq!(
            (target.file_path.as_str(), target.name.as_str()),
            ("src/local.py", "sym")
        );
        assert_eq!(calls[0].line, 5);
    }
}

#[test]
fn tier3_selected_foreign_target_is_rejected_with_live_positive_control() {
    let dir = fixture(&[
        ("src/x/a.rs", "/// First run.\npub fn run(x: i32) {}\n"),
        ("src/y/b.rs", "/// Second run.\npub fn run(x: i32) {}\n"),
        (
            "src/caller.py",
            "def wire() -> None:\n    \"\"\"Foreign miss.\"\"\"\n    run(1); helper(1)\n",
        ),
        (
            "src/p/a.py",
            "def helper(x: int) -> None:\n    \"\"\"First helper.\"\"\"\n    pass\n",
        ),
        (
            "src/q/b.py",
            "def helper(x: int) -> None:\n    \"\"\"Second helper.\"\"\"\n    pass\n",
        ),
        (
            "src/control.py",
            "def control() -> None:\n    \"\"\"Ordinary miss.\"\"\"\n    helper(1)\n",
        ),
    ]);
    let mut index = Index::new();
    for (target_file, caller_file, name, definition_line) in [
        ("src/x/a.rs", "src/caller.py", "run", 1),
        ("src/p/a.py", "src/control.py", "helper", 0),
    ] {
        let symbol = format!("scip-python pip fixture 0.1.0 {name}().");
        let mut target = Document::new();
        target.relative_path = target_file.into();
        let mut definition = Occurrence::new();
        definition.range = vec![definition_line, 0, name.len() as i32];
        definition.symbol = symbol.clone();
        definition.symbol_roles = 1;
        target.occurrences.push(definition);
        index.documents.push(target);
        let mut caller = Document::new();
        caller.relative_path = caller_file.into();
        let mut reference = Occurrence::new();
        reference.range = vec![2, 4, 4 + name.len() as i32];
        reference.symbol = symbol;
        caller.occurrences.push(reference);
        index.documents.push(caller);
        if name == "helper" {
            let mut same_line = Document::new();
            same_line.relative_path = "src/caller.py".into();
            let mut reference = Occurrence::new();
            reference.range = vec![2, 12, 18];
            reference.symbol = format!("scip-python pip fixture 0.1.0 {name}().");
            same_line.occurrences.push(reference);
            index.documents.push(same_line);
        }
    }
    fs::write(
        dir.path().join("index.scip"),
        index.write_to_bytes().unwrap(),
    )
    .unwrap();
    let config_path = dir.path().join(".keel/keel.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["tier3"]["enabled"] = true.into();
    config["tier3"]["scip_paths"]["python"] = "index.scip".into();
    fs::write(config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let out = keel(dir.path(), &["map", "--verbose"]);
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("tier3 resolved 1 additional references"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(incoming_calls(dir.path(), "src/x/a.rs", "run", false), 0);
    assert_eq!(incoming_calls(dir.path(), "src/y/b.rs", "run", false), 0);
    assert_eq!(incoming_calls(dir.path(), "src/p/a.py", "helper", false), 1);
    assert_eq!(incoming_calls(dir.path(), "src/q/b.py", "helper", false), 0);
}
