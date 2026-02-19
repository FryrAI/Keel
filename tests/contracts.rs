// Integration test entry point for contract tests.
#[path = "contracts/test_graph_store_contract.rs"]
mod test_graph_store_contract;
#[path = "contracts/test_language_resolver_contract.rs"]
mod test_language_resolver_contract;
#[path = "contracts/test_result_structs_contract.rs"]
mod test_result_structs_contract;

// JSON schema contract tests (decomposed from test_json_schema_contract.rs)
#[path = "contracts/test_compile_schema.rs"]
mod test_compile_schema;
#[path = "contracts/test_discover_schema.rs"]
mod test_discover_schema;
#[path = "contracts/test_explain_schema.rs"]
mod test_explain_schema;
#[path = "contracts/test_map_schema.rs"]
mod test_map_schema;
#[path = "contracts/test_schema_helpers.rs"]
mod test_schema_helpers;
