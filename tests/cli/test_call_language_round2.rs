//! Final-fold regressions for call admission and incremental graph parity.

use std::fs;
use std::path::Path;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeKind, NodeKind};
use protobuf::Message;
use scip::types::{Document, Index, Occurrence};

use super::test_bare_call_binding::{fixture, incoming_calls};
use crate::common::{keel, violations_with_code};

fn verbose_compile(root: &Path, file: &str) -> std::process::Output {
    let out = keel(root, &["compile", file, "--json", "--verbose"]);
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("graph sync"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn calls(root: &Path, file: &str) -> Vec<(String, String, String, u32)> {
    let conn = rusqlite::Connection::open(root.join(".keel/graph.db")).unwrap();
    let mut query = conn.prepare("SELECT s.name,t.name,t.file_path,e.line FROM edges e JOIN nodes s ON s.id=e.source_id JOIN nodes t ON t.id=e.target_id WHERE e.kind='calls' AND e.file_path=?1 ORDER BY 1,2,3,4").unwrap();
    query
        .query_map([file], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn tier3_member_refusal_keeps_same_line_helper_resolvable() {
    let member = "/// Guard.\npub struct G;\nimpl G {\n /// Member.\n pub fn run(&self) {}\n}\n";
    let dir = fixture(&[
        ("src/x/a.rs", member),
        ("src/y/b.rs", member),
        (
            "src/p/h.rs",
            "/// First helper.\npub fn helper(x: i32) {}\n",
        ),
        (
            "src/q/h.rs",
            "/// Second helper.\npub fn helper(x: i32) {}\n",
        ),
        (
            "src/caller.rs",
            "/// Caller.\npub fn wire() {\n run(1); helper(2);\n}\n",
        ),
        (
            "src/control.rs",
            "/// Control.\npub fn control() {\n helper(2);\n}\n",
        ),
    ]);
    let mut index = Index::new();
    for (file, name, line, callers) in [
        ("src/x/a.rs", "run", 4, vec![("src/caller.rs", 1)]),
        (
            "src/p/h.rs",
            "helper",
            1,
            vec![("src/caller.rs", 9), ("src/control.rs", 1)],
        ),
    ] {
        let symbol = format!("scip-rust cargo fixture 0.1.0 {name}().");
        let mut target = Document::new();
        target.relative_path = file.into();
        let mut definition = Occurrence::new();
        definition.range = vec![line, 0, name.len() as i32];
        definition.symbol = symbol.clone();
        definition.symbol_roles = 1;
        target.occurrences.push(definition);
        index.documents.push(target);
        for (file, column) in callers {
            let mut caller = Document::new();
            caller.relative_path = file.into();
            let mut reference = Occurrence::new();
            reference.range = vec![2, column, column + name.len() as i32];
            reference.symbol = symbol.clone();
            caller.occurrences.push(reference);
            index.documents.push(caller);
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
    config["tier3"]["scip_paths"]["rust"] = "index.scip".into();
    fs::write(config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let out = keel(dir.path(), &["map", "--verbose"]);
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("tier3 resolved 2 additional references"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(incoming_calls(dir.path(), "src/x/a.rs", "run", true), 0);
    assert_eq!(incoming_calls(dir.path(), "src/y/b.rs", "run", true), 0);
    assert_eq!(incoming_calls(dir.path(), "src/p/h.rs", "helper", false), 2);
    assert_eq!(incoming_calls(dir.path(), "src/q/h.rs", "helper", false), 0);
    assert_eq!(
        calls(dir.path(), "src/caller.rs"),
        vec![("wire".into(), "helper".into(), "src/p/h.rs".into(), 3)]
    );
}

#[test]
fn module_barrel_call_recompile_keeps_only_maps_target() {
    let dir = fixture(&[
        ("src/a.ts", "/** Original. */\nexport function original(): void {}\n"),
        ("src/barrel.ts", "export { original as f } from './a';\n/** Wrapper. */\nexport function wrap(): void {\n /** Nested. */\n function f(): void {}\n}\n"),
        ("src/caller.ts", "import { f } from './barrel';\n/** Anchor. */\nexport function anchor(): void {}\nf();\n"),
    ]);
    let expected = vec![(
        "src/caller.ts".into(),
        "original".into(),
        "src/a.ts".into(),
        4,
    )];
    assert_eq!(calls(dir.path(), "src/caller.ts"), expected);
    for _ in 0..2 {
        assert!(verbose_compile(dir.path(), "src/caller.ts")
            .status
            .success());
        assert_eq!(calls(dir.path(), "src/caller.ts"), expected);
    }
}

fn assert_source_parity(file: &str, source: &str, caller: &str) {
    for fresh in [true, false] {
        let dir = fixture(&[("src/base.py", ""), (file, if fresh { "" } else { source })]);
        fs::write(dir.path().join(file), source).unwrap();
        for _ in 0..2 {
            assert!(verbose_compile(dir.path(), file).status.success());
            let compiled = calls(dir.path(), file);
            assert_eq!(compiled.len(), 1, "fresh={fresh}: {compiled:?}");
            assert_eq!(compiled[0].0, caller);
            assert_eq!(compiled[0].1, "sym");
            assert_eq!(compiled[0].2, file);
            assert!(keel(dir.path(), &["map"]).status.success());
            assert_eq!(calls(dir.path(), file), compiled);
        }
    }
}

#[test]
fn method_body_compile_source_matches_map() {
    assert_source_parity("src/local.py", "def sym(x: str) -> str:\n    \"\"\"Symbol.\"\"\"\n    return x\nclass C:\n    \"\"\"Class.\"\"\"\n    def run(self) -> str:\n        \"\"\"Method.\"\"\"\n        return sym(\"run\")\n", "run");
}

#[test]
fn nested_function_compile_source_matches_map() {
    assert_source_parity("src/local.py", "def sym(x: str) -> str:\n    \"\"\"Symbol.\"\"\"\n    return x\ndef outer() -> None:\n    \"\"\"Outer.\"\"\"\n    def inner() -> str:\n        \"\"\"Inner.\"\"\"\n        return sym(\"run\")\n", "inner");
}

#[test]
fn rust_test_module_function_compile_source_matches_map() {
    assert_source_parity("src/lib.rs", "/// Symbol.\npub fn sym(x: i32) {}\n#[cfg(test)]\nmod tests {\n /// Caller.\n fn wire() {\n  sym(1);\n }\n}\n", "wire");
}

fn assert_rust_module_sync(module: &str) {
    let source = format!("{module}\n/// Symbol.\npub fn sym(x: i32) {{}}\n/// Caller.\npub fn wire() {{\n sym(1);\n}}\n");
    let dir = fixture(&[("src/lib.rs", &source), ("src/tests.rs", "")]);
    let store =
        SqliteGraphStore::open(dir.path().join(".keel/graph.db").to_str().unwrap()).unwrap();
    let module_id = store
        .get_nodes_in_file("src/lib.rs")
        .into_iter()
        .find(|n| n.name == "tests" && n.kind == NodeKind::Module)
        .unwrap()
        .id;
    drop(store);
    fs::write(
        dir.path().join("src/lib.rs"),
        source.replace(" sym(1);", " sym(1);\n sym(2);"),
    )
    .unwrap();
    for _ in 0..2 {
        assert!(verbose_compile(dir.path(), "src/lib.rs").status.success());
        let compiled = calls(dir.path(), "src/lib.rs");
        assert_eq!(compiled.len(), 2, "{compiled:?}");
        assert!(compiled.iter().all(|row| row.0 == "wire" && row.1 == "sym"));
        let store =
            SqliteGraphStore::open(dir.path().join(".keel/graph.db").to_str().unwrap()).unwrap();
        let node = store.get_node_by_id(module_id).unwrap();
        assert_eq!(node.name, "tests");
        assert_eq!(node.kind, NodeKind::Module);
        assert!(store
            .reference_edges_from_file("src/lib.rs")
            .iter()
            .all(|(e, _)| e.kind == EdgeKind::Calls));
    }
    let compiled = calls(dir.path(), "src/lib.rs");
    assert!(keel(dir.path(), &["map"]).status.success());
    assert_eq!(calls(dir.path(), "src/lib.rs"), compiled);
}

#[test]
fn mapped_inline_rust_module_recompile_updates_calls_without_sync_failure() {
    assert_rust_module_sync(
        "#[cfg(test)]\nmod tests {\n /// An inline fixture.\n fn example() {}\n}",
    );
}

#[test]
fn mapped_external_rust_module_recompile_updates_calls_without_sync_failure() {
    assert_rust_module_sync("#[cfg(test)]\nmod tests;");
}

fn assert_method_shadow(file: &str, other: &str, target: &str, source: &str) {
    for fresh in [true, false] {
        let dir = fixture(&[
            ("src/base.py", ""),
            (other, target),
            (file, if fresh { "" } else { source }),
        ]);
        fs::write(dir.path().join(file), source).unwrap();
        for _ in 0..2 {
            let out = verbose_compile(dir.path(), file);
            assert_eq!(
                out.status.code(),
                Some(1),
                "fresh={fresh}: {}",
                String::from_utf8_lossy(&out.stdout)
            );
            let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(violations_with_code(&result, "E005").len(), 1, "{result}");
            assert_eq!(incoming_calls(dir.path(), file, "run", true), 0);
            assert_eq!(incoming_calls(dir.path(), other, "run", false), 1);
            let compiled = calls(dir.path(), file);
            assert_eq!(compiled.len(), 1, "{compiled:?}");
            assert_eq!(
                (&compiled[0].0[..], &compiled[0].1[..], &compiled[0].2[..]),
                ("wire", "run", other)
            );
        }
    }
}

#[test]
fn python_method_shadow_preserves_imported_functions_e005() {
    assert_method_shadow("src/caller.py", "src/x.py", "def run(x: int) -> None:\n    \"\"\"Imported.\"\"\"\n    pass\n", "from x import run\nclass C:\n    \"\"\"Class.\"\"\"\n    def run(self) -> None:\n        \"\"\"Method.\"\"\"\n        pass\ndef wire() -> None:\n    \"\"\"Caller.\"\"\"\n    run(1, 2)\n");
}

#[test]
fn typescript_method_shadow_preserves_imported_functions_e005() {
    assert_method_shadow("src/caller.ts", "src/x.ts", "/** Imported. */\nexport function run(x: number): void {}\n", "import { run } from './x';\n/** Class. */\nclass C {\n /** Method. */\n run(): void {}\n}\n/** Caller. */\nexport function wire(): void { run(1, 2); }\n");
}

#[test]
fn fresh_go_method_shadow_preserves_package_functions_e005() {
    let dir = fixture(&[
        (
            "src/x.go",
            "package fixture\n// run is a package function.\nfunc run(x int) {}\n",
        ),
        ("src/caller.go", ""),
    ]);
    fs::write(dir.path().join("src/caller.go"), "package fixture\n// C is a class.\ntype C struct {}\n// run is a method.\nfunc (c C) run() {}\n// wire calls the package function.\nfunc wire() { run(1, 2) }\n").unwrap();
    let out = verbose_compile(dir.path(), "src/caller.go");
    assert_eq!(out.status.code(), Some(1));
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(violations_with_code(&result, "E005").len(), 1, "{result}");
    assert_eq!(incoming_calls(dir.path(), "src/caller.go", "run", true), 0);
    // Base's Go Tier-2 resolver picks the local method once its node exists;
    // #81 then refuses it, so sync writes no edge and later E005 stays silent.
    assert!(calls(dir.path(), "src/caller.go").is_empty());
    let out = verbose_compile(dir.path(), "src/caller.go");
    assert!(out.status.success());
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(violations_with_code(&result, "E005").is_empty());
}

#[test]
fn rust_method_shadow_preserves_imported_functions_e005() {
    assert_method_shadow("src/caller.rs", "src/x.rs", "/// Imported.\npub fn run(x: i32) {}\n", "use crate::x::run;\n/// Class.\npub struct C;\nimpl C {\n /// Method.\n pub fn run(&self) {}\n}\n/// Caller.\npub fn wire() { run(1, 2); }\n");
}
