// Tests for keel explain command (Spec 006 - Enforcement Engine)
//
// use keel_enforce::types::ExplainResult;

#[test]
#[ignore = "Not yet implemented"]
/// Explain should return the full resolution chain for a given error code + hash.
fn test_explain_resolution_chain() {
    // GIVEN an E001 violation on hash "abc12345678"
    // WHEN `keel explain E001 abc12345678` is run
    // THEN the full resolution chain is returned (tiers traversed, candidates considered)
}

#[test]
#[ignore = "Not yet implemented"]
/// Explain should include the confidence score at each resolution step.
fn test_explain_includes_confidence() {
    // GIVEN a resolution chain with multiple tiers
    // WHEN explain is run
    // THEN each step shows the confidence score
}

#[test]
#[ignore = "Not yet implemented"]
/// Explain should show which resolution tier produced the final result.
fn test_explain_shows_resolution_tier() {
    // GIVEN a call resolved by Tier 2 (Oxc)
    // WHEN explain is run
    // THEN the result shows resolution_tier=2
}

#[test]
#[ignore = "Not yet implemented"]
/// Explain should show all candidate targets considered during resolution.
fn test_explain_shows_all_candidates() {
    // GIVEN a call with 3 possible resolution candidates
    // WHEN explain is run
    // THEN all 3 candidates are listed with their scores
}

#[test]
#[ignore = "Not yet implemented"]
/// Explain with an invalid hash should return a clear error message.
fn test_explain_invalid_hash() {
    // GIVEN a hash that doesn't exist in the graph
    // WHEN `keel explain E001 invalid_hash` is run
    // THEN a clear error message is returned
}

#[test]
#[ignore = "Not yet implemented"]
/// Explain should show the file path and line number of the relevant code.
fn test_explain_includes_location() {
    // GIVEN a valid error code and hash
    // WHEN explain is run
    // THEN file path and line number are included in the output
}

#[test]
#[ignore = "Not yet implemented"]
/// Explain should complete in under 50ms (performance target).
fn test_explain_performance_target() {
    // GIVEN a populated graph with 10k nodes
    // WHEN explain is run
    // THEN the response is returned in under 50ms
}

#[test]
#[ignore = "Not yet implemented"]
/// Explain should return ExplainResult struct with all required fields.
fn test_explain_result_structure() {
    // GIVEN a valid explain query
    // WHEN the ExplainResult is returned
    // THEN it has fields: error_code, hash, resolution_chain, candidates, confidence, tier
}
