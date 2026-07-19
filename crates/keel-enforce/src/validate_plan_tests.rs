use super::*;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode};

fn node(id: u64, hash: &str, name: &str, file: &str, kind: NodeKind) -> GraphNode {
    GraphNode {
        id,
        hash: hash.into(),
        kind,
        name: name.into(),
        signature: format!("fn {name}()"),
        file_path: file.into(),
        line_start: id as u32,
        line_end: id as u32 + 5,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

/// `handleRequest` in src/handler.rs, called by two functions.
fn fixture_store() -> SqliteGraphStore {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&node(
            1,
            "modhash00001",
            "handler",
            "src/handler.rs",
            NodeKind::Module,
        ))
        .unwrap();
    store
        .insert_node(&node(
            2,
            "TARGETHASH1",
            "handleRequest",
            "src/handler.rs",
            NodeKind::Function,
        ))
        .unwrap();
    store
        .insert_node(&node(
            3,
            "CALLER00001",
            "main",
            "src/main.rs",
            NodeKind::Function,
        ))
        .unwrap();
    store
        .insert_node(&node(
            4,
            "CALLER00002",
            "route",
            "src/router.rs",
            NodeKind::Function,
        ))
        .unwrap();
    store
        .update_edges(vec![
            EdgeChange::Add(GraphEdge {
                id: 1,
                source_id: 3,
                target_id: 2,
                kind: EdgeKind::Calls,
                file_path: "src/main.rs".into(),
                line: 3,
                confidence: 1.0,
            }),
            EdgeChange::Add(GraphEdge {
                id: 2,
                source_id: 4,
                target_id: 2,
                kind: EdgeKind::Calls,
                file_path: "src/router.rs".into(),
                line: 7,
                confidence: 1.0,
            }),
        ])
        .unwrap();
    store
}

#[test]
fn removal_with_callers_is_high_risk() {
    let store = fixture_store();
    let plan = "Step 1: Remove handleRequest since it is no longer needed.";
    let result = validate_plan(&store, plan);

    assert!(!result.unrecognized);
    assert_eq!(result.actions.len(), 1);
    let action = &result.actions[0];
    assert_eq!(action.action, "remove");
    assert_eq!(action.symbol, "handleRequest");
    assert_eq!(action.risk, "HIGH");
    assert_eq!(action.caller_count, 2);
    let caller_names: Vec<&str> = action.callers.iter().map(|c| c.name.as_str()).collect();
    assert!(caller_names.contains(&"main"));
    assert!(caller_names.contains(&"route"));
    assert!(action.suggested_order.contains("caller"));
}

#[test]
fn nonsense_plan_detects_nothing() {
    let store = fixture_store();
    let plan = "Buy milk, water the plants, and go for a walk in the park.";
    let result = validate_plan(&store, plan);
    assert!(result.unrecognized);
    assert!(result.actions.is_empty());
}

#[test]
fn rename_is_high_risk() {
    let store = fixture_store();
    let result = validate_plan(&store, "Rename handleRequest to handleReq");
    assert_eq!(result.actions[0].action, "rename");
    assert_eq!(result.actions[0].risk, "HIGH");
}

#[test]
fn signature_change_is_medium_risk() {
    let store = fixture_store();
    let result = validate_plan(
        &store,
        "Change the signature of handleRequest to add a context arg",
    );
    // "signature" wins over "add ... arg" by higher risk rank.
    assert_eq!(result.actions[0].action, "change_signature");
    assert_eq!(result.actions[0].risk, "MEDIUM");
}

#[test]
fn symbol_without_action_is_not_reported() {
    let store = fixture_store();
    // Mentions handleRequest but with no action keyword nearby.
    let result = validate_plan(&store, "Read handleRequest to understand the flow.");
    assert!(result.unrecognized);
    assert_eq!(result.symbols_detected, 1);
}

#[test]
fn file_path_is_detected() {
    let store = fixture_store();
    let result = validate_plan(&store, "We will delete handleRequest in src/handler.rs");
    assert!(result
        .files_detected
        .contains(&"src/handler.rs".to_string()));
}
