// Output format integration tests (Spec 008 - Output Formats)
// Entry point that wires up all output test modules.

#[path = "output/test_clean_compile.rs"]
mod test_clean_compile;
#[path = "output/test_compile_json_schema.rs"]
mod test_compile_json_schema;
#[path = "output/test_discover_json_schema.rs"]
mod test_discover_json_schema;
#[path = "output/test_error_codes.rs"]
mod test_error_codes;
#[path = "output/test_explain_json_schema.rs"]
mod test_explain_json_schema;
#[path = "output/test_llm_format.rs"]
mod test_llm_format;
#[path = "output/test_llm_token_budget.rs"]
mod test_llm_token_budget;
#[path = "output/test_map_json_schema.rs"]
mod test_map_json_schema;
