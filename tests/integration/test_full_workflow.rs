// Integration tests: full command workflow (E2E)
//
// Validates the complete lifecycle of keel commands from init through deinit,
// exercising every major command in sequence.
//
// use std::process::Command;
// use tempfile::TempDir;
// use std::fs;

#[test]
#[ignore = "Not yet implemented"]
fn test_full_lifecycle_init_map_compile_deinit() {
    // GIVEN a fresh project directory with TypeScript source files
    // WHEN init, map, compile, and deinit are run in sequence
    // THEN each command succeeds and deinit removes the .keel/ directory completely
}

#[test]
#[ignore = "Not yet implemented"]
fn test_discover_returns_valid_adjacency_after_map() {
    // GIVEN a mapped project with function A calling functions B and C
    // WHEN `keel discover <hash_of_A>` is run
    // THEN the output lists B and C as callees of A with correct metadata
}

#[test]
#[ignore = "Not yet implemented"]
fn test_where_resolves_hash_to_file_and_line() {
    // GIVEN a mapped project with function A at line 42 of src/main.ts
    // WHEN `keel where <hash_of_A>` is run
    // THEN the output contains "src/main.ts:42" (or equivalent path:line format)
}

#[test]
#[ignore = "Not yet implemented"]
fn test_explain_shows_resolution_chain() {
    // GIVEN a mapped project where a compile produced an E001 broken_caller
    // WHEN `keel explain E001 <hash>` is run
    // THEN the output includes the resolution tier, confidence, and step-by-step chain
}

#[test]
#[ignore = "Not yet implemented"]
fn test_stats_shows_graph_summary() {
    // GIVEN a mapped project with known node and edge counts
    // WHEN `keel stats` is run
    // THEN the output includes total nodes, edges, files, and per-language breakdown
}

#[test]
#[ignore = "Not yet implemented"]
fn test_deinit_removes_all_keel_artifacts() {
    // GIVEN a project with .keel/ directory containing graph.db and config.toml
    // WHEN `keel deinit` is run
    // THEN the .keel/ directory is completely removed and no keel artifacts remain
}
