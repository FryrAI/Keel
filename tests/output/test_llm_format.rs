// Tests for LLM-friendly output format (Spec 008 - Output Formats)
//
// use keel_output::llm::LlmFormatter;

#[test]
#[ignore = "Not yet implemented"]
/// LLM format should use structured text optimized for LLM consumption.
fn test_llm_format_structure() {
    // GIVEN a compile result with violations
    // WHEN formatted for LLM output
    // THEN the output uses clear section headers and structured text
}

#[test]
#[ignore = "Not yet implemented"]
/// LLM format should include fix_hint prominently for each violation.
fn test_llm_format_fix_hint_prominent() {
    // GIVEN a compile result with E001 violation
    // WHEN formatted for LLM output
    // THEN fix_hint is prominently displayed for easy LLM parsing
}

#[test]
#[ignore = "Not yet implemented"]
/// LLM format should include file path and line number for each violation.
fn test_llm_format_includes_location() {
    // GIVEN a violation at src/parser.ts:42
    // WHEN formatted for LLM output
    // THEN the file path and line number are clearly visible
}

#[test]
#[ignore = "Not yet implemented"]
/// LLM format should group violations by file for easier processing.
fn test_llm_format_groups_by_file() {
    // GIVEN violations in 3 different files
    // WHEN formatted for LLM output
    // THEN violations are grouped by file path
}

#[test]
#[ignore = "Not yet implemented"]
/// LLM format should include circuit breaker escalation context.
fn test_llm_format_circuit_breaker_context() {
    // GIVEN a violation at circuit breaker attempt 2 (wider discover)
    // WHEN formatted for LLM output
    // THEN additional discover context is included in the output
}

#[test]
#[ignore = "Not yet implemented"]
/// LLM format for clean compile should produce minimal output.
fn test_llm_format_clean_compile() {
    // GIVEN a clean compile result
    // WHEN formatted for LLM output
    // THEN output is minimal (e.g., "No violations found.")
}

#[test]
#[ignore = "Not yet implemented"]
/// LLM format should include the error code category for each violation.
fn test_llm_format_error_code_category() {
    // GIVEN E001 (broken_caller) and W001 (placement) violations
    // WHEN formatted for LLM output
    // THEN each shows its category (broken_caller, placement) alongside the code
}
