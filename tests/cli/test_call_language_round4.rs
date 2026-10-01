//! Module and function definitions with the same name keep distinct identities.

use std::fs;
use std::path::Path;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};

use super::test_bare_call_binding::fixture;
use crate::common::keel;

const MODULE: &str = "mod a {}\n/// Caller.\nfn wire() {}\n";
const FUNCTION: &str = "/// Function.\npub fn a(x: i32) {}\n/// Caller.\nfn wire() { a(1); }\n";
const EXTERNAL: &str = "use crate::lib::a;\n/// External caller.\nfn outside() { a(1); }\n";

fn compile(root: &Path) {
    let out = keel(root, &["compile", "src/lib.rs", "--json", "--verbose"]);
    // The historical module caller can produce E001; graph sync must succeed.
    assert!(matches!(out.status.code(), Some(0 | 1)));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("graph sync"), "{stderr}");
    assert!(!stderr.contains("FOREIGN KEY"), "{stderr}");
}

fn assert_local_function_call(store: &SqliteGraphStore) -> u64 {
    let nodes = store.get_nodes_in_file("src/lib.rs");
    let functions: Vec<_> = nodes
        .iter()
        .filter(|n| n.name == "a" && n.kind == NodeKind::Function)
        .collect();
    assert_eq!(functions.len(), 1, "Function a must be stored: {nodes:?}");
    let function = functions[0];
    let wire = nodes.iter().find(|n| n.name == "wire").unwrap();
    let calls: Vec<_> = store
        .get_edges(wire.id, EdgeDirection::Outgoing)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect();
    assert_eq!(calls.len(), 1, "wire must call Function a: {calls:?}");
    assert_eq!(calls[0].target_id, function.id);
    function.id
}

#[test]
fn replaced_module_stores_same_named_function_and_preserves_live_external_caller() {
    for external in [true, false] {
        let mut files = vec![("src/lib.rs", MODULE)];
        if external {
            files.push(("src/caller.rs", EXTERNAL));
        }
        let dir = fixture(&files);
        let store =
            SqliteGraphStore::open(dir.path().join(".keel/graph.db").to_str().unwrap()).unwrap();
        let module = store
            .get_nodes_in_file("src/lib.rs")
            .into_iter()
            .find(|n| n.name == "a" && n.kind == NodeKind::Module)
            .unwrap();
        let historical: Vec<_> = store
            .get_edges(module.id, EdgeDirection::Incoming)
            .into_iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .collect();
        assert_eq!(historical.len(), usize::from(external));
        drop(store);
        fs::write(dir.path().join("src/lib.rs"), FUNCTION).unwrap();
        let mut function_id = None;
        for _ in 0..2 {
            compile(dir.path());
            let store = SqliteGraphStore::open(dir.path().join(".keel/graph.db").to_str().unwrap())
                .unwrap();
            let id = assert_local_function_call(&store);
            assert_eq!(*function_id.get_or_insert(id), id);
            if external {
                assert_ne!(id, module.id);
                assert_eq!(
                    store.get_node_by_id(module.id).unwrap().kind,
                    NodeKind::Module
                );
                let incoming = store.get_edges(module.id, EdgeDirection::Incoming);
                assert!(incoming.iter().any(|e| {
                    e.id == historical[0].id
                        && e.source_id == historical[0].source_id
                        && e.kind == EdgeKind::Calls
                }));
            } else {
                // The engine may already have reused this row before sync (#100).
                assert!(store
                    .get_node_by_id(module.id)
                    .is_none_or(|n| n.kind != NodeKind::Module));
            }
        }
    }
}

#[test]
fn added_function_beside_same_named_module_stores_call_without_foreign_key_error() {
    let dir = fixture(&[("src/lib.rs", MODULE)]);
    let store =
        SqliteGraphStore::open(dir.path().join(".keel/graph.db").to_str().unwrap()).unwrap();
    let module = store
        .get_nodes_in_file("src/lib.rs")
        .into_iter()
        .find(|n| n.name == "a" && n.kind == NodeKind::Module)
        .unwrap();
    drop(store);
    fs::write(
        dir.path().join("src/lib.rs"),
        format!("mod a {{}}\n{FUNCTION}"),
    )
    .unwrap();
    let mut function_id = None;
    for _ in 0..2 {
        compile(dir.path());
        let store =
            SqliteGraphStore::open(dir.path().join(".keel/graph.db").to_str().unwrap()).unwrap();
        let id = assert_local_function_call(&store);
        assert_ne!(id, module.id);
        assert_eq!(*function_id.get_or_insert(id), id);
        assert_eq!(
            store.get_node_by_id(module.id).unwrap().kind,
            NodeKind::Module
        );
    }
}
