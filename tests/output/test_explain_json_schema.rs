// Tests for explain command JSON output schema (Spec 008 - Output Formats)
//
// use keel_output::json::ExplainJsonOutput;
// use serde_json::Value;

#[test]
#[ignore = "Not yet implemented"]
/// Explain JSON should include the error_code and hash queried.
fn test_explain_json_includes_query() {
    // GIVEN an explain result for E001 on hash "abc12345678"
    // WHEN serialized to JSON
    // THEN it includes error_code="E001" and hash="abc12345678"
}

#[test]
#[ignore = "Not yet implemented"]
/// Explain JSON should include the resolution_chain array.
fn test_explain_json_resolution_chain() {
    // GIVEN an explain result with a 3-step resolution chain
    // WHEN serialized to JSON
    // THEN resolution_chain array has 3 entries with tier and confidence per step
}

#[test]
#[ignore = "Not yet implemented"]
/// Explain JSON should include all candidate targets considered.
fn test_explain_json_candidates() {
    // GIVEN an explain result with 4 resolution candidates
    // WHEN serialized to JSON
    // THEN candidates array has 4 entries with scores
}

#[test]
#[ignore = "Not yet implemented"]
/// Explain JSON should include the final resolution result.
fn test_explain_json_final_result() {
    // GIVEN an explain result that resolved to a specific target
    // WHEN serialized to JSON
    // THEN the result includes the final target hash, file path, and line number
}

#[test]
#[ignore = "Not yet implemented"]
/// Explain JSON should include the node's source code context.
fn test_explain_json_source_context() {
    // GIVEN an explain result for a specific node
    // WHEN serialized to JSON
    // THEN it includes source code context (surrounding lines) for the node
}
