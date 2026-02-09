// Integration tests: init -> map -> edit -> compile workflow (E2E)
//
// Validates the most common developer workflow: initializing keel on a project,
// mapping it, making a code change, and compiling to catch structural violations.
//
// use std::process::Command;
// use tempfile::TempDir;
// use std::fs;

#[test]
#[ignore = "Not yet implemented"]
fn test_init_creates_keel_directory() {
    // GIVEN a fresh project directory with some TypeScript source files
    // WHEN `keel init` is run in the project root
    // THEN a .keel/ directory is created containing config.toml and an empty graph.db
}

#[test]
#[ignore = "Not yet implemented"]
fn test_init_then_map_populates_graph() {
    // GIVEN a project with `keel init` already run, containing 10 TypeScript files
    // WHEN `keel map` is run
    // THEN graph.db is populated with nodes and edges, and stats show non-zero counts
}

#[test]
#[ignore = "Not yet implemented"]
fn test_compile_after_map_returns_clean() {
    // GIVEN a project that has been initialized and mapped with no violations
    // WHEN `keel compile` is run on a file with no issues
    // THEN exit code is 0 and stdout is empty (clean compile)
}

#[test]
#[ignore = "Not yet implemented"]
fn test_edit_breaks_caller_then_compile_catches_it() {
    // GIVEN a mapped project where function A calls function B(x: int, y: int)
    // WHEN function B's signature is changed to B(x: int) and `keel compile` is run
    // THEN exit code is 1 and output contains E005 arity_mismatch for A's call to B
}

#[test]
#[ignore = "Not yet implemented"]
fn test_edit_removes_function_then_compile_catches_it() {
    // GIVEN a mapped project where function A calls function B
    // WHEN function B is deleted entirely and `keel compile` is run
    // THEN exit code is 1 and output contains E004 function_removed
}

#[test]
#[ignore = "Not yet implemented"]
fn test_compile_specific_file_only_checks_that_file() {
    // GIVEN a mapped project with violations in file A and file B
    // WHEN `keel compile file_a.ts` is run (specifying only file A)
    // THEN only violations from file A are reported, not file B
}
