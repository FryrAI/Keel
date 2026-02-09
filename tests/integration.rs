// Integration test entry point for E2E workflow tests.
#[path = "integration/test_init_to_compile.rs"]
mod test_init_to_compile;
#[path = "integration/test_full_workflow.rs"]
mod test_full_workflow;
#[path = "integration/test_multi_language.rs"]
mod test_multi_language;
#[path = "integration/test_large_codebase.rs"]
mod test_large_codebase;
#[path = "integration/test_config_roundtrip.rs"]
mod test_config_roundtrip;
#[path = "integration/test_error_recovery.rs"]
mod test_error_recovery;
