// Tests for LLM output token budget management (Spec 008 - Output Formats)
//
// use keel_output::llm::LlmFormatter;

#[test]
#[ignore = "Not yet implemented"]
/// LLM output should respect the token budget limit when specified.
fn test_llm_token_budget_respected() {
    // GIVEN a compile result with 50 violations and a token budget of 2000
    // WHEN formatted for LLM output with the budget
    // THEN the output fits within ~2000 tokens
}

#[test]
#[ignore = "Not yet implemented"]
/// When token budget is exceeded, violations should be prioritized by severity.
fn test_llm_token_budget_prioritizes_errors() {
    // GIVEN 20 errors and 30 warnings with a limited token budget
    // WHEN formatted for LLM output
    // THEN errors appear first and warnings are truncated if needed
}

#[test]
#[ignore = "Not yet implemented"]
/// Truncated output should include a count of omitted violations.
fn test_llm_token_budget_truncation_notice() {
    // GIVEN 50 violations truncated to fit budget
    // WHEN formatted for LLM output
    // THEN the output ends with "... and 35 more violations omitted"
}

#[test]
#[ignore = "Not yet implemented"]
/// No token budget (unlimited) should output all violations.
fn test_llm_no_token_budget() {
    // GIVEN 50 violations with no token budget set
    // WHEN formatted for LLM output
    // THEN all 50 violations are included
}

#[test]
#[ignore = "Not yet implemented"]
/// Token budget should account for fix_hints which can be lengthy.
fn test_llm_token_budget_accounts_for_fix_hints() {
    // GIVEN violations with long fix_hints
    // WHEN token budget is applied
    // THEN fix_hints are included in the token count calculation
}

#[test]
#[ignore = "Not yet implemented"]
/// Default token budget should be sensible for typical LLM context windows.
fn test_llm_default_token_budget() {
    // GIVEN no explicit token budget
    // WHEN the default is used
    // THEN it is set to a sensible default (e.g., 4000 tokens)
}
