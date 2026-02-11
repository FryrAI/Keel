// Tests for E001 broken caller detection (Spec 006 - Enforcement Engine)
//
// use keel_enforce::violations::check_broken_callers;
// use keel_core::types::{GraphNode, GraphEdge};

#[test]
#[ignore = "Not yet implemented"]
/// Changing a function signature should produce E001 for all callers.
fn test_e001_signature_change_breaks_callers() {
    // GIVEN function foo(a: int) with 3 callers
    // WHEN foo's signature changes to foo(a: int, b: str)
    // THEN E001 is produced for all 3 callers with fix_hint
}

#[test]
#[ignore = "Not yet implemented"]
/// Changing a parameter type should produce E001 for callers using the old type.
fn test_e001_parameter_type_change() {
    // GIVEN function process(data: string) with callers passing string
    // WHEN parameter type changes to process(data: Buffer)
    // THEN E001 is produced for callers still passing string
}

#[test]
#[ignore = "Not yet implemented"]
/// Changing a return type should produce E001 for callers using the old return type.
fn test_e001_return_type_change() {
    // GIVEN function fetch() -> string with callers expecting string
    // WHEN return type changes to fetch() -> Result<string, Error>
    // THEN E001 is produced for callers expecting the old return type
}

#[test]
#[ignore = "Not yet implemented"]
/// Adding an optional parameter should NOT produce E001 (backward compatible).
fn test_e001_optional_parameter_no_break() {
    // GIVEN function foo(a: int) with callers
    // WHEN foo changes to foo(a: int, b: str = "default")
    // THEN no E001 is produced (optional parameter is backward compatible)
}

#[test]
#[ignore = "Not yet implemented"]
/// Renaming a function should produce E001 for all callers of the old name.
fn test_e001_function_rename() {
    // GIVEN function processData() with callers
    // WHEN the function is renamed to handleData()
    // THEN E001 is produced for all callers of processData()
}

#[test]
#[ignore = "Not yet implemented"]
/// E001 should include a fix_hint suggesting the new signature.
fn test_e001_includes_fix_hint() {
    // GIVEN a signature change that breaks callers
    // WHEN E001 violations are produced
    // THEN each violation includes a fix_hint with the new expected signature
}

#[test]
#[ignore = "Not yet implemented"]
/// E001 should include the file path and line number of each broken caller.
fn test_e001_includes_location() {
    // GIVEN a signature change that breaks callers in multiple files
    // WHEN E001 violations are produced
    // THEN each violation includes the file path and line number of the caller
}

#[test]
#[ignore = "Not yet implemented"]
/// E001 confidence should reflect the resolution tier of the call edge.
fn test_e001_confidence_from_resolution_tier() {
    // GIVEN a broken caller detected via Tier 1 (tree-sitter, high confidence)
    // WHEN E001 is produced
    // THEN the violation confidence matches the edge's resolution confidence
}

#[test]
#[ignore = "Not yet implemented"]
/// E001 for dynamic dispatch calls should be WARNING not ERROR.
fn test_e001_dynamic_dispatch_is_warning() {
    // GIVEN a broken caller detected through a dynamic dispatch call (low confidence)
    // WHEN the violation is categorized
    // THEN it is a WARNING, not an ERROR
}

#[test]
#[ignore = "Not yet implemented"]
/// Multiple signature changes in the same compile should all produce separate E001s.
fn test_e001_multiple_changes_multiple_errors() {
    // GIVEN 3 functions with signature changes in the same compile
    // WHEN enforcement runs
    // THEN separate E001 violations are produced for each function's broken callers
}

#[test]
#[ignore = "Not yet implemented"]
/// E001 for a function with no callers should not produce any violation.
fn test_e001_no_callers_no_violation() {
    // GIVEN a function with changed signature but zero callers
    // WHEN enforcement runs
    // THEN no E001 violation is produced
}

#[test]
#[ignore = "Not yet implemented"]
/// E001 should track the resolution_tier (1, 2, or 3) that detected the broken call.
fn test_e001_reports_resolution_tier() {
    // GIVEN a broken caller detected via Tier 2 (Oxc enhancer)
    // WHEN E001 is produced
    // THEN the violation reports resolution_tier=2
}

#[test]
#[ignore = "Not yet implemented"]
/// Changing method visibility from public to private should produce E001 for external callers.
fn test_e001_visibility_change() {
    // GIVEN a public method called from another module
    // WHEN the method is changed to private
    // THEN E001 is produced for external callers
}

#[test]
#[ignore = "Not yet implemented"]
/// E001 severity should always be ERROR.
fn test_e001_severity_is_error() {
    // GIVEN any broken caller scenario
    // WHEN E001 is produced
    // THEN the severity is ERROR (not WARNING or INFO)
}

#[test]
#[ignore = "Not yet implemented"]
/// E001 error code should be exactly "E001".
fn test_e001_error_code_format() {
    // GIVEN a broken caller violation
    // WHEN the violation is serialized
    // THEN the error_code field is "E001"
}
