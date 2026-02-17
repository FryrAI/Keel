//! Multi-language integration tests: init and map detection.
//!
//! Verifies that `keel init` + `keel map` correctly detects and indexes
//! functions across all four supported languages (TypeScript, Python, Go, Rust).

use super::test_multi_lang_setup::{init_and_map, setup_mixed_project};

#[test]
fn test_map_detects_all_four_languages() {
    let dir = setup_mixed_project();
    init_and_map(&dir);

    // Open the graph DB and check for nodes from each language
    let db_path = dir.path().join(".keel/graph.db");
    let store =
        keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap())
            .expect("should open graph.db");

    let modules = keel_core::store::GraphStore::get_all_modules(&store);

    // Collect all nodes across all modules
    let mut all_nodes = Vec::new();
    for module in &modules {
        let nodes =
            keel_core::store::GraphStore::get_nodes_in_file(&store, &module.file_path);
        all_nodes.extend(nodes);
    }

    // Check for nodes from each language by file extension
    let file_paths: Vec<&str> = all_nodes.iter().map(|n| n.file_path.as_str()).collect();
    let has_ts = file_paths.iter().any(|p| p.ends_with(".ts"));
    let has_py = file_paths.iter().any(|p| p.ends_with(".py"));
    let has_go = file_paths.iter().any(|p| p.ends_with(".go"));
    let has_rs = file_paths.iter().any(|p| p.ends_with(".rs"));

    assert!(
        has_ts,
        "graph should contain TypeScript nodes, found paths: {:?}",
        file_paths
    );
    assert!(
        has_py,
        "graph should contain Python nodes, found paths: {:?}",
        file_paths
    );
    assert!(
        has_go,
        "graph should contain Go nodes, found paths: {:?}",
        file_paths
    );
    assert!(
        has_rs,
        "graph should contain Rust nodes, found paths: {:?}",
        file_paths
    );

    // Verify specific function names from each language
    let names: Vec<&str> = all_nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"add"), "should find TS add function");
    assert!(names.contains(&"greet"), "should find Python greet function");
    assert!(
        names.contains(&"multiply"),
        "should find Go multiply function"
    );
    assert!(names.contains(&"divide"), "should find Rust divide function");
}
