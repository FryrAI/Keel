// Integration tests: multi-language project support (E2E)
//
// Validates that keel correctly handles projects containing a mix of
// TypeScript, Python, Go, and Rust source files simultaneously.
//
// use std::process::Command;
// use tempfile::TempDir;
// use std::fs;

#[test]
#[ignore = "Not yet implemented"]
fn test_map_detects_all_four_languages() {
    // GIVEN a project containing .ts, .py, .go, and .rs files
    // WHEN `keel map` is run
    // THEN stats show nodes for all four languages in the graph
}

#[test]
#[ignore = "Not yet implemented"]
fn test_compile_typescript_in_mixed_project() {
    // GIVEN a mixed-language project that has been mapped
    // WHEN a TypeScript file is modified to break a caller and `keel compile` is run
    // THEN the violation is detected correctly using Oxc resolution (Tier 2)
}

#[test]
#[ignore = "Not yet implemented"]
fn test_compile_python_in_mixed_project() {
    // GIVEN a mixed-language project that has been mapped
    // WHEN a Python file is modified to remove type hints and `keel compile` is run
    // THEN the E002 missing_type_hints violation is detected using ty resolution
}

#[test]
#[ignore = "Not yet implemented"]
fn test_compile_go_in_mixed_project() {
    // GIVEN a mixed-language project that has been mapped
    // WHEN a Go file is modified to break a function signature and `keel compile` is run
    // THEN the E005 arity_mismatch violation is detected using tree-sitter heuristics
}

#[test]
#[ignore = "Not yet implemented"]
fn test_compile_rust_in_mixed_project() {
    // GIVEN a mixed-language project that has been mapped
    // WHEN a Rust file is modified to remove a public function and `keel compile` is run
    // THEN the E004 function_removed violation is detected
}

#[test]
#[ignore = "Not yet implemented"]
fn test_discover_works_across_languages() {
    // GIVEN a mapped mixed-language project
    // WHEN `keel discover` is called for nodes in each language
    // THEN adjacency results are correct for TS, Python, Go, and Rust nodes alike
}
