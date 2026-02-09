// Tests for E004 function removed detection (Spec 006 - Enforcement Engine)
//
// use keel_enforce::rules::FunctionRemovedRule;

#[test]
#[ignore = "Not yet implemented"]
/// Deleting a function with callers should produce E004 for each caller.
fn test_e004_deleted_function_with_callers() {
    // GIVEN function process() with 5 callers
    // WHEN process() is deleted from the codebase
    // THEN E004 is produced for all 5 callers
}

#[test]
#[ignore = "Not yet implemented"]
/// Deleting a function with no callers should NOT produce E004.
fn test_e004_deleted_function_no_callers() {
    // GIVEN function unused() with 0 callers
    // WHEN unused() is deleted
    // THEN no E004 is produced (no broken references)
}

#[test]
#[ignore = "Not yet implemented"]
/// E004 should include the file path where the function was previously defined.
fn test_e004_includes_original_location() {
    // GIVEN a deleted function that was in src/utils.ts:42
    // WHEN E004 is produced
    // THEN the violation includes the original file path and line number
}

#[test]
#[ignore = "Not yet implemented"]
/// E004 fix_hint should suggest alternatives (similar function names).
fn test_e004_fix_hint_suggests_alternatives() {
    // GIVEN deleted function processData() and existing function handleData()
    // WHEN E004 is produced
    // THEN fix_hint suggests handleData() as a possible replacement
}

#[test]
#[ignore = "Not yet implemented"]
/// E004 should detect function removal even when the entire file is deleted.
fn test_e004_detects_file_deletion() {
    // GIVEN a file utils.ts with 3 functions that have external callers
    // WHEN the entire file utils.ts is deleted
    // THEN E004 is produced for all functions that had callers
}
