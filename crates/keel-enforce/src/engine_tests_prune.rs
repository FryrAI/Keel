//! Nested module removal and rollback when a surviving node blocks pruning.

use super::*;
use keel_core::types::EdgeDirection;

#[test]
fn test_prune_file_removes_nested_modules_deepest_first() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    // Parent ids sort before descendants: a kind-only sort deletes them first.
    for (id, parent, kind) in [
        (1, 0, NodeKind::Module),
        (2, 1, NodeKind::Module),
        (3, 2, NodeKind::Module),
        (4, 3, NodeKind::Function),
        (5, 1, NodeKind::Function),
    ] {
        let name = format!("node_{id}");
        let mut node = make_node(id, &name, &name, "", "src/gone.rs");
        node.kind = kind;
        node.module_id = parent;
        store.insert_node(&node).unwrap();
    }
    store
        .insert_node(&make_node(6, "caller", "caller", "", "src/keep.rs"))
        .unwrap();
    store
        .update_edges(vec![
            EdgeChange::Add(make_call_edge(1, 6, 4, "src/keep.rs")),
            EdgeChange::Add(make_call_edge(2, 5, 4, "src/gone.rs")),
        ])
        .unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));
    assert_eq!(engine.prune_file("src/gone.rs").unwrap(), 5);
    assert!(engine.store.get_nodes_in_file("src/gone.rs").is_empty());
    for id in 1..=6 {
        assert!(engine.store.get_edges(id, EdgeDirection::Both).is_empty());
    }
    assert!(engine.store.get_node_by_id(6).is_some());
}

#[test]
fn test_prune_file_failed_node_removal_preserves_nodes_and_edges() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let mut module = make_node(1, "module", "gone", "", "src/gone.rs");
    module.kind = NodeKind::Module;
    module.previous_hashes = vec!["old_module".into()];
    store.insert_node(&module).unwrap();
    let mut child = make_node(2, "child", "child", "", "src/gone.rs");
    child.module_id = 1;
    child.previous_hashes = vec!["old_child".into()];
    store.insert_node(&child).unwrap();
    // FK-valid, but never emitted by keel's parsers: a surviving file refers
    // to this module, forcing the node transaction to roll back.
    let mut keeper = make_node(3, "keeper", "keeper", "", "src/keep.rs");
    keeper.module_id = 1;
    store.insert_node(&keeper).unwrap();
    store
        .update_edges(vec![
            EdgeChange::Add(make_call_edge(1, 3, 2, "src/keep.rs")),
            EdgeChange::Add(make_call_edge(2, 1, 2, "src/gone.rs")),
        ])
        .unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let before_nodes = serde_json::to_value(engine.store.get_nodes_in_file("src/gone.rs")).unwrap();
    let before_edges =
        serde_json::to_value(engine.store.get_edges(2, EdgeDirection::Both)).unwrap();
    let error = engine.prune_file("src/gone.rs").unwrap_err();
    assert!(error.to_string().contains("FOREIGN KEY"), "{error}");
    assert_eq!(
        serde_json::to_value(engine.store.get_nodes_in_file("src/gone.rs")).unwrap(),
        before_nodes
    );
    assert_eq!(
        serde_json::to_value(engine.store.get_edges(2, EdgeDirection::Both)).unwrap(),
        before_edges
    );
    assert_eq!(
        serde_json::to_value(engine.store.get_node_by_id(3)).unwrap(),
        serde_json::to_value(Some(keeper)).unwrap()
    );
    assert_eq!(engine.store.get_previous_hashes(1), vec!["old_module"]);
    assert_eq!(engine.store.get_previous_hashes(2), vec!["old_child"]);
}
