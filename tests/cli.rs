// CLI test entry point for keel command tests.
#[path = "cli/test_init.rs"]
mod test_init;
#[path = "cli/test_init_merge.rs"]
mod test_init_merge;
#[path = "cli/test_map.rs"]
mod test_map;
#[path = "cli/test_discover.rs"]
mod test_discover;
#[path = "cli/test_compile.rs"]
mod test_compile;
#[path = "cli/test_compile_batch.rs"]
mod test_compile_batch;
#[path = "cli/test_where.rs"]
mod test_where;
#[path = "cli/test_explain.rs"]
mod test_explain;
#[path = "cli/test_deinit.rs"]
mod test_deinit;
#[path = "cli/test_stats.rs"]
mod test_stats;
#[path = "cli/test_exit_codes.rs"]
mod test_exit_codes;
