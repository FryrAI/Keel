// Tests for compile command JSON output schema (Spec 008 - Output Formats)
//
// use keel_output::json::CompileJsonOutput;
// use serde_json::Value;

#[test]
#[ignore = "Not yet implemented"]
/// Compile JSON output should have a "violations" array at the top level.
fn test_compile_json_has_violations_array() {
    // GIVEN a compile result with violations
    // WHEN serialized to JSON
    // THEN the top-level object has a "violations" array
}

#[test]
#[ignore = "Not yet implemented"]
/// Each violation in the JSON should have error_code, severity, message, and fix_hint.
fn test_compile_json_violation_fields() {
    // GIVEN a compile result with an E001 violation
    // WHEN serialized to JSON
    // THEN the violation object has error_code, severity, message, fix_hint fields
}

#[test]
#[ignore = "Not yet implemented"]
/// Each violation should include file_path and line_number.
fn test_compile_json_violation_location() {
    // GIVEN a compile result with a violation at src/parser.ts:42
    // WHEN serialized to JSON
    // THEN the violation has file_path="src/parser.ts" and line_number=42
}

#[test]
#[ignore = "Not yet implemented"]
/// Each violation should include confidence score and resolution_tier.
fn test_compile_json_violation_metadata() {
    // GIVEN a compile result with a Tier 2 resolved violation
    // WHEN serialized to JSON
    // THEN the violation has confidence (0.0-1.0) and resolution_tier (1, 2, or 3)
}

#[test]
#[ignore = "Not yet implemented"]
/// Compile JSON should include a summary object with counts.
fn test_compile_json_summary() {
    // GIVEN a compile result with 3 errors and 2 warnings
    // WHEN serialized to JSON
    // THEN the summary shows error_count=3 and warning_count=2
}

#[test]
#[ignore = "Not yet implemented"]
/// Compile JSON with no violations should have an empty violations array.
fn test_compile_json_empty_violations() {
    // GIVEN a clean compile result
    // WHEN serialized to JSON
    // THEN the violations array is empty and summary counts are zero
}

#[test]
#[ignore = "Not yet implemented"]
/// Compile JSON schema should validate against the JSON schema in tests/schemas/.
fn test_compile_json_validates_schema() {
    // GIVEN a compile JSON output
    // WHEN validated against tests/schemas/compile_output.json
    // THEN validation passes
}
