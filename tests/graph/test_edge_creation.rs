// Tests for GraphEdge creation and EdgeKind variants (Spec 000 - Graph Schema)

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{
    EdgeChange, EdgeDirection, EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind,
};
use keel_parsers::resolver::ResolvedEdge;

/// Helper: build a GraphNode with sensible defaults.
fn make_node(id: u64, hash: &str, kind: NodeKind, name: &str, module_id: u64) -> GraphNode {
    GraphNode {
        id,
        hash: hash.to_string(),
        kind,
        name: name.to_string(),
        signature: format!("fn {}()", name),
        file_path: "src/test.rs".to_string(),
        line_start: id as u32,
        line_end: id as u32 + 5,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id,
        package: None,
    }
}

/// Helper: build a GraphEdge.
fn make_edge(
    id: u64,
    source_id: u64,
    target_id: u64,
    kind: EdgeKind,
    file_path: &str,
    line: u32,
) -> GraphEdge {
    GraphEdge {
        id,
        source_id,
        target_id,
        kind,
        file_path: file_path.to_string(),
        line,
        confidence: 1.0,
    }
}

/// Helper: create an in-memory store seeded with a module and two function nodes.
///
/// Returns (store, module_id=100, caller_id=1, callee_id=2).
fn store_with_two_functions() -> SqliteGraphStore {
    let mut store = SqliteGraphStore::in_memory().expect("in-memory store");
    let nodes = vec![
        NodeChange::Add(make_node(
            100,
            "mod_test_hash",
            NodeKind::Module,
            "test_mod",
            0,
        )),
        NodeChange::Add(make_node(
            1,
            "fn_caller_hsh",
            NodeKind::Function,
            "caller",
            100,
        )),
        NodeChange::Add(make_node(
            2,
            "fn_callee_hsh",
            NodeKind::Function,
            "callee",
            100,
        )),
    ];
    store.update_nodes(nodes).expect("insert nodes");
    store
}

#[test]
/// Creating a Calls edge should link caller to callee with the correct kind.
fn test_create_calls_edge() {
    // GIVEN two function nodes (caller and callee)
    let caller_id = 1_u64;
    let callee_id = 2_u64;

    // WHEN a GraphEdge with EdgeKind::Calls is created between them
    let edge = make_edge(10, caller_id, callee_id, EdgeKind::Calls, "src/test.rs", 5);

    // THEN the edge links source to target with the Calls kind
    assert_eq!(edge.source_id, caller_id);
    assert_eq!(edge.target_id, callee_id);
    assert_eq!(edge.kind, EdgeKind::Calls);
    assert_eq!(edge.file_path, "src/test.rs");
    assert_eq!(edge.line, 5);
}

#[test]
/// Creating an Imports edge should represent a module-level import relationship.
fn test_create_imports_edge() {
    // GIVEN a module node and a target module node
    let importer_id = 100_u64;
    let imported_id = 101_u64;

    // WHEN a GraphEdge with EdgeKind::Imports is created
    let edge = make_edge(
        20,
        importer_id,
        imported_id,
        EdgeKind::Imports,
        "src/api.rs",
        1,
    );

    // THEN the edge captures the import relationship and source location
    assert_eq!(edge.source_id, importer_id);
    assert_eq!(edge.target_id, imported_id);
    assert_eq!(edge.kind, EdgeKind::Imports);
    assert_eq!(edge.file_path, "src/api.rs");
    assert_eq!(edge.line, 1);
}

#[test]
/// Creating a Contains edge should represent parent-child containment (module->function).
fn test_create_contains_edge() {
    // GIVEN a module node and a function node within that module
    let module_id = 100_u64;
    let function_id = 1_u64;

    // WHEN a GraphEdge with EdgeKind::Contains is created
    let edge = make_edge(
        30,
        module_id,
        function_id,
        EdgeKind::Contains,
        "src/test.rs",
        5,
    );

    // THEN the edge represents structural containment
    assert_eq!(edge.source_id, module_id);
    assert_eq!(edge.target_id, function_id);
    assert_eq!(edge.kind, EdgeKind::Contains);
    assert_eq!(edge.kind.as_str(), "contains");
}

#[test]
/// Creating an Implements relationship uses EdgeKind::Inherits (no Implements variant exists).
fn test_create_implements_edge() {
    // GIVEN a class node and an interface/trait node
    let implementor_id = 10_u64;
    let interface_id = 20_u64;

    // WHEN a GraphEdge with EdgeKind::Inherits is created (Inherits covers implements)
    let edge = make_edge(
        40,
        implementor_id,
        interface_id,
        EdgeKind::Inherits,
        "src/impl.rs",
        3,
    );

    // THEN the edge links the implementor to the interface using Inherits
    assert_eq!(edge.source_id, implementor_id);
    assert_eq!(edge.target_id, interface_id);
    assert_eq!(edge.kind, EdgeKind::Inherits);
    assert_eq!(edge.kind.as_str(), "inherits");
}

#[test]
/// Creating an Inherits edge should capture class inheritance relationships.
fn test_create_inherits_edge() {
    // GIVEN a child class node and a parent class node
    let child_id = 10_u64;
    let parent_id = 20_u64;

    // WHEN a GraphEdge with EdgeKind::Inherits is created
    let edge = make_edge(
        50,
        child_id,
        parent_id,
        EdgeKind::Inherits,
        "src/models.py",
        8,
    );

    // THEN the edge represents the inheritance chain
    assert_eq!(edge.source_id, child_id);
    assert_eq!(edge.target_id, parent_id);
    assert_eq!(edge.kind, EdgeKind::Inherits);
    assert_eq!(edge.file_path, "src/models.py");
    assert_eq!(edge.line, 8);
}

#[test]
/// Edges should support bidirectional lookup (find all callers of a function).
fn test_bidirectional_edge_lookup() {
    // GIVEN a graph with multiple Calls edges pointing to the same function
    let mut store = SqliteGraphStore::in_memory().expect("in-memory store");

    let nodes = vec![
        NodeChange::Add(make_node(
            100,
            "mod_bidi_hash",
            NodeKind::Module,
            "bidi_mod",
            0,
        )),
        NodeChange::Add(make_node(
            1,
            "fn_target_hsh",
            NodeKind::Function,
            "target_fn",
            100,
        )),
        NodeChange::Add(make_node(
            2,
            "fn_call_a_hsh",
            NodeKind::Function,
            "caller_a",
            100,
        )),
        NodeChange::Add(make_node(
            3,
            "fn_call_b_hsh",
            NodeKind::Function,
            "caller_b",
            100,
        )),
    ];
    store.update_nodes(nodes).expect("insert nodes");

    let edges = vec![
        EdgeChange::Add(make_edge(1, 2, 1, EdgeKind::Calls, "src/test.rs", 10)),
        EdgeChange::Add(make_edge(2, 3, 1, EdgeKind::Calls, "src/test.rs", 20)),
        // Also give target_fn an outgoing call to caller_a for bidirectional test
        EdgeChange::Add(make_edge(3, 1, 2, EdgeKind::Calls, "src/test.rs", 5)),
    ];
    store.update_edges(edges).expect("insert edges");

    // WHEN querying for incoming edges on that function
    let incoming = store.get_edges(1, EdgeDirection::Incoming);
    assert_eq!(
        incoming.len(),
        2,
        "target_fn should have 2 incoming call edges"
    );

    // Verify callers are nodes 2 and 3
    let caller_ids: Vec<u64> = incoming.iter().map(|e| e.source_id).collect();
    assert!(
        caller_ids.contains(&2),
        "caller_a should be in incoming edges"
    );
    assert!(
        caller_ids.contains(&3),
        "caller_b should be in incoming edges"
    );

    // THEN querying with EdgeDirection::Both returns all connected edges
    let both = store.get_edges(1, EdgeDirection::Both);
    assert_eq!(
        both.len(),
        3,
        "target_fn should have 3 total edges (2 in + 1 out)"
    );
}

#[test]
/// ResolvedEdge (from keel_parsers) carries a confidence score.
/// GraphEdge itself does NOT have resolution_tier or confidence fields.
fn test_edge_resolution_tier() {
    // GIVEN a resolved edge from tree-sitter (Tier 1, high confidence)
    let resolved = ResolvedEdge {
        target_file: "src/utils.rs".to_string(),
        target_name: "hash_password".to_string(),
        confidence: 0.95,
        resolution_tier: "tier1".into(),
    };

    // THEN the confidence is stored and reflects high-confidence resolution
    assert!(
        resolved.confidence > 0.9,
        "Tier 1 tree-sitter resolution should be high confidence"
    );
    assert_eq!(resolved.target_name, "hash_password");
    assert_eq!(resolved.target_file, "src/utils.rs");
}

#[test]
/// ResolvedEdge should carry a confidence score between 0.0 and 1.0.
fn test_edge_confidence_score() {
    // GIVEN a call edge with ambiguous resolution (low confidence)
    let ambiguous = ResolvedEdge {
        target_file: "src/handler.rs".to_string(),
        target_name: "process".to_string(),
        confidence: 0.6,
        resolution_tier: "tier1".into(),
    };

    // THEN the confidence is stored and within valid range
    assert!(
        (0.0..=1.0).contains(&ambiguous.confidence),
        "Confidence must be between 0.0 and 1.0"
    );
    assert!(
        ambiguous.confidence < 0.7,
        "Ambiguous resolution should have low confidence"
    );

    // GIVEN a high-confidence resolution
    let certain = ResolvedEdge {
        target_file: "src/lib.rs".to_string(),
        target_name: "init".to_string(),
        confidence: 1.0,
        resolution_tier: "tier1".into(),
    };
    assert!(
        (0.0..=1.0).contains(&certain.confidence),
        "Confidence must be between 0.0 and 1.0"
    );
    assert_eq!(certain.confidence, 1.0);
}

#[test]
/// Creating an edge with invalid node references should fail gracefully.
fn test_edge_with_invalid_node_references() {
    // GIVEN an in-memory store with no nodes
    let mut store = SqliteGraphStore::in_memory().expect("in-memory store");

    // WHEN attempting to create an edge from a non-existent source node
    let bad_edge = make_edge(999, 9999, 8888, EdgeKind::Calls, "src/ghost.rs", 1);
    let result = store.update_edges(vec![EdgeChange::Add(bad_edge)]);

    // THEN the operation returns an error due to foreign key constraint
    assert!(
        result.is_err(),
        "Inserting edge with non-existent node references should fail"
    );
}

#[test]
/// Self-referential edges (node calls itself) should be valid for recursive functions.
fn test_self_referential_edge() {
    // GIVEN a recursive function node
    let mut store = store_with_two_functions();
    let recursive_id = 1_u64;

    // WHEN a Calls edge is created from the node to itself
    let self_edge = make_edge(
        99,
        recursive_id,
        recursive_id,
        EdgeKind::Calls,
        "src/test.rs",
        3,
    );
    let result = store.update_edges(vec![EdgeChange::Add(self_edge)]);

    // THEN the edge is valid and stored correctly
    assert!(result.is_ok(), "Self-referential edge should be allowed");

    let outgoing = store.get_edges(recursive_id, EdgeDirection::Outgoing);
    let self_edges: Vec<&GraphEdge> = outgoing
        .iter()
        .filter(|e| e.source_id == recursive_id && e.target_id == recursive_id)
        .collect();
    assert_eq!(
        self_edges.len(),
        1,
        "Should have exactly one self-referential edge"
    );
    assert_eq!(self_edges[0].kind, EdgeKind::Calls);
}
