// Oracle 1: Node completeness — all functions/classes found vs LSP
//
// Validates that keel detects all function and class definitions that the
// LSP reports, ensuring no structural elements are missed during parsing.
//
// use keel_core::graph::{GraphStore, GraphNode, NodeKind};
// use std::path::Path;

#[test]
#[ignore = "Not yet implemented"]
fn test_all_top_level_functions_found_typescript() {
    // GIVEN a reference TypeScript project with known top-level function definitions
    // WHEN keel maps the project
    // THEN every top-level function reported by the LSP has a corresponding graph node
}

#[test]
#[ignore = "Not yet implemented"]
fn test_all_class_methods_found_typescript() {
    // GIVEN a reference TypeScript project with classes and their methods
    // WHEN keel maps the project
    // THEN every class method reported by the LSP has a corresponding graph node
}

#[test]
#[ignore = "Not yet implemented"]
fn test_all_top_level_functions_found_python() {
    // GIVEN a reference Python project with known top-level function definitions
    // WHEN keel maps the project
    // THEN every top-level function reported by the LSP has a corresponding graph node
}

#[test]
#[ignore = "Not yet implemented"]
fn test_all_class_methods_found_python() {
    // GIVEN a reference Python project with classes and their methods
    // WHEN keel maps the project
    // THEN every class method reported by the LSP has a corresponding graph node
}

#[test]
#[ignore = "Not yet implemented"]
fn test_exported_functions_detected_go() {
    // GIVEN a reference Go project with exported (capitalized) and unexported functions
    // WHEN keel maps the project
    // THEN both exported and unexported functions are captured as graph nodes
}

#[test]
#[ignore = "Not yet implemented"]
fn test_impl_methods_detected_rust() {
    // GIVEN a reference Rust project with struct impl blocks containing methods
    // WHEN keel maps the project
    // THEN every impl method is captured as a graph node with correct parent association
}

#[test]
#[ignore = "Not yet implemented"]
fn test_no_phantom_nodes_in_graph() {
    // GIVEN a mapped project
    // WHEN all graph nodes are compared against actual source files on disk
    // THEN every node maps to an existing function/class in an existing file (no phantoms)
}
