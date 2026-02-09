// Tests for `keel explain` CLI command (Spec 007 - CLI Commands)
//
// use std::process::Command;

#[test]
#[ignore = "Not yet implemented"]
/// `keel explain <code> <hash>` should output the resolution explanation.
fn test_explain_cli_output() {
    // GIVEN an E001 violation on hash "abc12345678"
    // WHEN `keel explain E001 abc12345678` is run
    // THEN the resolution chain and candidates are printed
}

#[test]
#[ignore = "Not yet implemented"]
/// `keel explain` should complete in under 50ms.
fn test_explain_cli_performance() {
    // GIVEN a populated graph
    // WHEN `keel explain E001 <hash>` is run
    // THEN the response is returned in under 50ms
}

#[test]
#[ignore = "Not yet implemented"]
/// `keel explain` with invalid error code should return an error.
fn test_explain_cli_invalid_code() {
    // GIVEN an invalid error code "E999"
    // WHEN `keel explain E999 <hash>` is run
    // THEN a clear error message is returned
}

#[test]
#[ignore = "Not yet implemented"]
/// `keel explain` output should include the resolution tier that produced the result.
fn test_explain_cli_shows_tier() {
    // GIVEN a call resolved by Tier 2
    // WHEN `keel explain` is run
    // THEN the output includes "resolution_tier: 2"
}
