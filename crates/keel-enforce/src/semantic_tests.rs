use super::*;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::types::GraphNode;

fn node(
    id: u64,
    hash: &str,
    name: &str,
    kind: NodeKind,
    is_public: bool,
    docstring: Option<&str>,
) -> GraphNode {
    GraphNode {
        id,
        hash: hash.into(),
        kind,
        name: name.into(),
        signature: format!("fn {name}()"),
        file_path: "src/lib.rs".into(),
        line_start: id as u32,
        line_end: id as u32 + 5,
        docstring: docstring.map(String::from),
        is_public,
        type_hints_present: true,
        has_docstring: docstring.is_some(),
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

#[test]
fn summary_from_module_docstring() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&node(
            1,
            "modhash00001",
            "lib",
            NodeKind::Module,
            true,
            Some("Core library utilities.\nMore detail here."),
        ))
        .unwrap();
    store
        .insert_node(&node(
            2,
            "fnhash00001",
            "parse",
            NodeKind::Function,
            true,
            None,
        ))
        .unwrap();

    let result = build_semantic_map(&store);
    assert_eq!(result.command, "map");
    assert_eq!(result.modules.len(), 1);
    let m = &result.modules[0];
    assert_eq!(m.summary, "Core library utilities.");
    assert_eq!(m.public_functions.len(), 1);
    assert_eq!(m.public_functions[0].name, "parse");
    assert!(m.when_to_use.contains("exports: parse"));
}

#[test]
fn summary_falls_back_to_first_public_symbol_doc() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&node(
            1,
            "modhash00001",
            "lib",
            NodeKind::Module,
            true,
            None,
        ))
        .unwrap();
    store
        .insert_node(&node(
            2,
            "fnhash00001",
            "parse",
            NodeKind::Function,
            true,
            Some("Parse the input string."),
        ))
        .unwrap();

    let result = build_semantic_map(&store);
    assert_eq!(result.modules[0].summary, "Parse the input string.");
}

#[test]
fn public_types_are_separated_and_private_excluded() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&node(
            1,
            "modhash00001",
            "lib",
            NodeKind::Module,
            true,
            None,
        ))
        .unwrap();
    store
        .insert_node(&node(
            2,
            "clshash00001",
            "Config",
            NodeKind::Class,
            true,
            None,
        ))
        .unwrap();
    store
        .insert_node(&node(
            3,
            "fnhash00001",
            "helper",
            NodeKind::Function,
            false,
            None,
        ))
        .unwrap();

    let result = build_semantic_map(&store);
    let m = &result.modules[0];
    assert_eq!(m.public_types.len(), 1);
    assert_eq!(m.public_types[0].name, "Config");
    // Private helper is excluded.
    assert!(m.public_functions.is_empty());
    assert_eq!(m.summary, "");
}
