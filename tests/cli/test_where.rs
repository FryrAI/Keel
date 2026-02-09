// Tests for `keel where` command (Spec 007 - CLI Commands)
//
// use std::process::Command;

#[test]
#[ignore = "Not yet implemented"]
/// `keel where <hash>` should return the file path and line number.
fn test_where_returns_file_and_line() {
    // GIVEN a node with hash "abc12345678" at src/parser.ts:42
    // WHEN `keel where abc12345678` is run
    // THEN output is "src/parser.ts:42"
}

#[test]
#[ignore = "Not yet implemented"]
/// `keel where` should complete in under 50ms.
fn test_where_performance_target() {
    // GIVEN a populated graph
    // WHEN `keel where <hash>` is run
    // THEN the response is returned in under 50ms
}

#[test]
#[ignore = "Not yet implemented"]
/// `keel where` with an invalid hash should return a clear error.
fn test_where_invalid_hash() {
    // GIVEN a hash that doesn't exist in the graph
    // WHEN `keel where nonexistent` is run
    // THEN a clear error message is returned
}

#[test]
#[ignore = "Not yet implemented"]
/// `keel where` should look up previous hashes if current hash not found.
fn test_where_checks_previous_hashes() {
    // GIVEN a node whose hash changed from old_hash to new_hash
    // WHEN `keel where old_hash` is run
    // THEN it finds the node via previous hash and returns current location
}
