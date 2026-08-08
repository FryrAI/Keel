use super::*;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};
use keel_parsers::resolver::Definition;

// Shared fixtures used by the engine_tests_* submodules below. The suite is
// split by topic (rather than one large file) to stay under the project's
// file-size cap; each submodule pulls these in via `use super::*;`.

fn make_node(id: u64, hash: &str, name: &str, sig: &str, file: &str) -> GraphNode {
    GraphNode {
        complexity: 0,
        is_trivial_wrapper: false,
        in_test_context: false,
        id,
        hash: hash.to_string(),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: sig.to_string(),
        file_path: file.to_string(),
        line_start: 10,
        line_end: 20,
        docstring: Some(format!("Doc for {}", name)),
        is_public: true,
        type_hints_present: true,
        has_docstring: true,
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

fn make_call_edge(id: u64, src: u64, tgt: u64, file: &str) -> GraphEdge {
    GraphEdge {
        id,
        source_id: src,
        target_id: tgt,
        kind: EdgeKind::Calls,
        file_path: file.to_string(),
        line: 15,
        confidence: 1.0,
    }
}

fn make_definition(name: &str, sig: &str, body: &str, file: &str) -> Definition {
    Definition {
        complexity: 1,
        name: name.to_string(),
        kind: NodeKind::Function,
        signature: sig.to_string(),
        file_path: file.to_string(),
        line_start: 10,
        line_end: 20,
        docstring: Some(format!("Doc for {}", name)),
        is_public: true,
        type_hints_present: true,
        body_text: body.to_string(),
        in_test_context: false,
        in_trait_context: false,
        is_associated: false,
        is_auto_invoked: false,
        is_decorated: false,
        has_keep_marker: false,
        is_macro: false,
        is_trivial_wrapper_body: false,
    }
}

#[path = "engine_tests_batch_suppress.rs"]
mod batch_suppress;
#[path = "engine_tests_circuit_breaker.rs"]
mod circuit_breaker_reset;
#[path = "engine_tests_e001.rs"]
mod e001;
#[path = "engine_tests_e002_e003.rs"]
mod e002_e003;
#[path = "engine_tests_e004_misc.rs"]
mod e004_misc;
#[path = "engine_tests_economy.rs"]
mod economy;
#[path = "engine_tests_module_identity.rs"]
mod module_identity;

#[test]
fn test_prune_file_removes_nodes_and_edges() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    // Two nodes in the doomed file, one in another file that calls into it.
    store
        .insert_node(&make_node(1, "h1", "foo", "fn foo()", "src/gone.rs"))
        .unwrap();
    store
        .insert_node(&make_node(2, "h2", "bar", "fn bar()", "src/gone.rs"))
        .unwrap();
    store
        .insert_node(&make_node(3, "h3", "caller", "fn caller()", "src/keep.rs"))
        .unwrap();
    // Edge caller(keep) -> foo(gone), and internal foo -> bar.
    store
        .update_edges(vec![
            EdgeChange::Add(make_call_edge(1, 3, 1, "src/keep.rs")),
            EdgeChange::Add(make_call_edge(2, 1, 2, "src/gone.rs")),
        ])
        .unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store));

    let pruned = engine.prune_file("src/gone.rs").unwrap();
    assert_eq!(pruned, 2, "both nodes in the deleted file are removed");

    // The deleted file's nodes are gone; the surviving file keeps its node.
    assert!(engine.store.get_nodes_in_file("src/gone.rs").is_empty());
    assert_eq!(engine.store.get_nodes_in_file("src/keep.rs").len(), 1);
    // Edges touching the pruned nodes are gone (both the inbound cross-file
    // call and the internal one).
    use keel_core::types::EdgeDirection;
    assert!(engine.store.get_edges(3, EdgeDirection::Both).is_empty());
}

#[test]
fn test_prune_file_missing_is_noop() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));
    assert_eq!(engine.prune_file("src/never_existed.rs").unwrap(), 0);
}

#[test]
fn test_compile_update_refreshes_parse_derived_node_facts() {
    // The hash-moved update path used to clone the stored node and copy only
    // hash/signature/docstring/visibility — complexity, is_trivial_wrapper
    // and in_test_context stayed stale until the next full `keel map`,
    // silently corrupting high_cc_mass_share and the trivial_wrapper audit
    // for anything edited through the hook-driven compile path.
    let store = SqliteGraphStore::in_memory().unwrap();
    let old_hash =
        keel_core::hash::compute_hash("fn shifty(x: i32)", "{ x + 1 }", "Doc for shifty");
    let mut node = make_node(1, &old_hash, "shifty", "fn shifty(x: i32)", "src/lib.rs");
    node.docstring = Some("Doc for shifty".to_string());
    node.complexity = 1;
    node.is_trivial_wrapper = false;
    node.in_test_context = false;
    store.insert_node(&node).unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    let mut def = make_definition(
        "shifty",
        "fn shifty(x: i32)",
        "{ if x > 0 { helper(x) } else { helper(-x) } }",
        "src/lib.rs",
    );
    def.complexity = 3;
    def.in_test_context = true;
    let file = FileIndex {
        file_path: "src/lib.rs".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };
    engine.compile(&[file]);

    let updated = engine
        .store
        .get_nodes_in_file("src/lib.rs")
        .into_iter()
        .find(|n| n.name == "shifty")
        .expect("node survives the update");
    assert_eq!(updated.complexity, 3, "complexity must move with the hash");
    assert!(
        updated.in_test_context,
        "test context must move with the hash"
    );
}
