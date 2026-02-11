// Oracle 1: Cross-language graph correctness
//
// Validates that keel produces correct graphs for projects containing
// multiple languages, ensuring no interference between language resolvers.
//
// use keel_core::store::GraphStore;
// use std::path::Path;

#[test]
#[ignore = "Not yet implemented"]
fn test_mixed_project_total_node_count() {
    // GIVEN a project with 50 TS files, 50 Py files, 50 Go files, and 50 Rust files
    // WHEN keel maps the entire project
    // THEN the total node count equals the sum of per-language node counts (no duplication)
}

#[test]
#[ignore = "Not yet implemented"]
fn test_mixed_project_no_cross_language_edges() {
    // GIVEN a project with TS and Python files that don't call each other
    // WHEN keel maps the project
    // THEN no call edges exist between TS nodes and Python nodes
}

#[test]
#[ignore = "Not yet implemented"]
fn test_mixed_project_per_language_accuracy() {
    // GIVEN a mixed-language project with LSP baselines for each language
    // WHEN keel maps the project
    // THEN per-language node and edge counts match their individual LSP baselines
}

#[test]
#[ignore = "Not yet implemented"]
fn test_language_detection_by_extension() {
    // GIVEN a project with .ts, .tsx, .py, .go, .rs, .js, .jsx files
    // WHEN keel maps the project
    // THEN each file is assigned the correct language and processed by the right parser
}

#[test]
#[ignore = "Not yet implemented"]
fn test_same_function_name_different_languages() {
    // GIVEN a project with a function named "process" in both TS and Python files
    // WHEN keel maps the project
    // THEN two separate nodes are created with different hashes (no collision)
}

#[test]
#[ignore = "Not yet implemented"]
fn test_mixed_project_graph_completeness() {
    // GIVEN a mixed-language project where all files are syntactically valid
    // WHEN keel maps the project
    // THEN every source file has at least one Module node in the graph (no files skipped)
}
