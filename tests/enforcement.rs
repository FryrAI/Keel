// Integration test entry point for enforcement behavioral tests.
#[path = "common/mod.rs"]
mod common;

#[path = "enforcement/test_broken_callers.rs"]
mod test_broken_callers;
#[path = "enforcement/test_type_hints.rs"]
mod test_type_hints;
#[path = "enforcement/test_docstrings.rs"]
mod test_docstrings;
#[path = "enforcement/test_placement.rs"]
mod test_placement;
#[path = "enforcement/test_duplicate_detection.rs"]
mod test_duplicate_detection;
#[path = "enforcement/test_circuit_breaker.rs"]
mod test_circuit_breaker;
#[path = "enforcement/test_batch_mode.rs"]
mod test_batch_mode;
#[path = "enforcement/test_suppress.rs"]
mod test_suppress;
#[path = "enforcement/test_explain.rs"]
mod test_explain;
#[path = "enforcement/test_clean_compile.rs"]
mod test_clean_compile;
#[path = "enforcement/test_progressive_adoption.rs"]
mod test_progressive_adoption;
#[path = "enforcement/test_arity_mismatch.rs"]
mod test_arity_mismatch;
#[path = "enforcement/test_function_removed.rs"]
mod test_function_removed;
