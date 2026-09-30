//! Module identity reuse must not turn module definitions into call targets.

use std::fs;
use std::path::Path;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeDirection, EdgeKind, GraphEdge, NodeKind};

use super::test_bare_call_binding::fixture;
use crate::common::keel;

fn compile(root: &Path) {
    let out = keel(root, &["compile", "src/lib.rs", "--json", "--verbose"]);
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("graph sync"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn assert_function_target(root: &Path) {
    let store = SqliteGraphStore::open(root.join(".keel/graph.db").to_str().unwrap()).unwrap();
    let caller = store
        .get_nodes_in_file("src/lib.rs")
        .into_iter()
        .find(|n| n.name == "wire")
        .unwrap();
    let targets: Vec<_> = store
        .get_edges(caller.id, EdgeDirection::Outgoing)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .map(|e| {
            let target = store.get_node_by_id(e.target_id).unwrap();
            (target.name, target.kind, target.file_path)
        })
        .collect();
    assert_eq!(
        targets,
        vec![("a".into(), NodeKind::Function, "src/b.rs".into())]
    );
}

fn assert_removed_module_does_not_capture_call(replacement: &str) {
    let dir = fixture(&[
        ("src/lib.rs", "mod a;\n/// Caller.\nfn wire() {}\n"),
        ("src/a.rs", ""),
        ("src/b.rs", "/// Imported function.\npub fn a(x: i32) {}\n"),
    ]);
    fs::write(
        dir.path().join("src/lib.rs"),
        format!("{replacement}use crate::b::a;\n/// Caller.\nfn wire() {{ a(1); }}\n"),
    )
    .unwrap();
    for _ in 0..2 {
        compile(dir.path());
        assert_function_target(dir.path());
    }
    assert!(keel(dir.path(), &["map"]).status.success());
    assert_function_target(dir.path());
}

#[test]
fn renamed_rust_module_never_captures_imported_function_call() {
    assert_removed_module_does_not_capture_call("mod b;\n");
}

#[test]
fn removed_rust_module_never_captures_imported_function_call() {
    assert_removed_module_does_not_capture_call("");
}

#[test]
fn vanished_inline_module_is_pruned_unless_it_has_a_live_external_caller() {
    let dir = fixture(&[
        (
            "src/lib.rs",
            "mod m {}\nmod kept {}\n/// Anchor.\nfn anchor() {}\n",
        ),
        ("src/caller.rs", "/// External caller.\nfn wire() {}\n"),
    ]);
    let mut store =
        SqliteGraphStore::open(dir.path().join(".keel/graph.db").to_str().unwrap()).unwrap();
    let nodes = store.get_nodes_in_file("src/lib.rs");
    let id = |name| {
        nodes
            .iter()
            .find(|n| n.name == name && n.kind == NodeKind::Module)
            .unwrap()
            .id
    };
    let (vanished, kept, file_module) = (id("m"), id("kept"), id("src/lib.rs"));
    let caller = store
        .get_nodes_in_file("src/caller.rs")
        .into_iter()
        .find(|n| n.name == "wire")
        .unwrap();
    // Seed a historical caller: current resolution must never call a module,
    // but sync still preserves the existing live-external-caller contract.
    let edge_id = store.max_id() + 1;
    store
        .update_edges(vec![EdgeChange::Add(GraphEdge {
            id: edge_id,
            source_id: caller.id,
            target_id: kept,
            kind: EdgeKind::Calls,
            file_path: "src/caller.rs".into(),
            line: 2,
            confidence: 1.0,
        })])
        .unwrap();
    drop(store);
    fs::write(
        dir.path().join("src/lib.rs"),
        "/// Anchor.\nfn anchor() {}\n",
    )
    .unwrap();
    for _ in 0..2 {
        compile(dir.path());
        let store =
            SqliteGraphStore::open(dir.path().join(".keel/graph.db").to_str().unwrap()).unwrap();
        assert!(store.get_node_by_id(vanished).is_none());
        assert_eq!(store.get_node_by_id(kept).unwrap().kind, NodeKind::Module);
        assert_eq!(
            store.get_node_by_id(file_module).unwrap().name,
            "src/lib.rs"
        );
        let incoming = store.get_edges(kept, EdgeDirection::Incoming);
        assert!(incoming.iter().any(|e| e.id == edge_id));
    }
}
