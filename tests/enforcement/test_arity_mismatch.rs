// Tests for E005 arity mismatch detection (Spec 006 - Enforcement Engine)
//
// use keel_enforce::violations_extended::check_arity_mismatch;

#[test]
#[ignore = "Not yet implemented"]
/// Adding a required parameter should produce E005 for all callers.
fn test_e005_added_required_parameter() {
    // GIVEN function foo(a) with callers calling foo(1)
    // WHEN foo changes to foo(a, b) (b is required)
    // THEN E005 is produced for all callers still passing 1 argument
}

#[test]
#[ignore = "Not yet implemented"]
/// Removing a parameter should produce E005 for callers passing extra arguments.
fn test_e005_removed_parameter() {
    // GIVEN function foo(a, b) with callers calling foo(1, 2)
    // WHEN foo changes to foo(a)
    // THEN E005 is produced for callers passing 2 arguments
}

#[test]
#[ignore = "Not yet implemented"]
/// Adding an optional parameter should NOT produce E005.
fn test_e005_optional_parameter_no_violation() {
    // GIVEN function foo(a) with callers
    // WHEN foo changes to foo(a, b=None) (b is optional)
    // THEN no E005 is produced (backward compatible)
}

#[test]
#[ignore = "Not yet implemented"]
/// E005 should include the expected vs actual parameter count.
fn test_e005_includes_count_info() {
    // GIVEN foo(a, b, c) and a caller using foo(1, 2)
    // WHEN E005 is produced
    // THEN it includes "expected 3 arguments, found 2"
}

#[test]
#[ignore = "Not yet implemented"]
/// E005 should include a fix_hint with the new function signature.
fn test_e005_includes_fix_hint() {
    // GIVEN an arity mismatch
    // WHEN E005 is produced
    // THEN fix_hint shows the current function signature
}
