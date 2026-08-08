//! MCP `keel/checkpoint` handler — compact session-state summary.
//!
//! Shares the deterministic [`keel_enforce::checkpoint`] core with the CLI so
//! the two interfaces never diverge. Runs git in the server's working
//! directory (the project root).

use std::path::Path;

use serde_json::Value;

use keel_enforce::checkpoint::{self, CheckpointMode};

use crate::mcp::{
    internal_err, lock_engine, lock_store, param_bool, param_str_opt, JsonRpcError, SharedEngine,
    SharedStore,
};
use crate::parse_shared::FileParser;

/// Handle the `keel/checkpoint` MCP tool call.
///
/// `root` is the server's authoritative project root; git and file parsing run
/// there rather than in the ambient process cwd.
pub(crate) fn handle_checkpoint(
    store: &SharedStore,
    engine: &SharedEngine,
    root: &Path,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let since = param_str_opt(&params, "since").map(String::from);
    let staged = param_bool(&params, "staged", false);

    let mode = if staged {
        CheckpointMode::Staged
    } else {
        CheckpointMode::Since(since)
    };

    // Parse changed files. Git returns repo-relative paths and the server runs
    // at the repo root, so pass them straight through — that keeps the parsed
    // `file_path` relative and matching the stored graph.
    let changed = checkpoint::changed_files(root, &mode);
    let mut parser = FileParser::new();
    let file_indices: Vec<_> = changed.iter().filter_map(|f| parser.parse(f)).collect();

    // Diff against the PRE-edit graph BEFORE compiling: `engine.compile`
    // persists re-baselined hashes, which would erase the reported change.
    let diff = {
        let store = lock_store(store)?;
        checkpoint::diff_changed_files(&*store, &file_indices)
    };

    let compile_result = {
        let mut eng = lock_engine(engine)?;
        eng.compile(&file_indices)
    };

    let commits = checkpoint::commit_subjects(root, &mode);
    let range = checkpoint::range_label(&mode);
    let result = checkpoint::build_checkpoint(diff, &compile_result, range, commits);

    serde_json::to_value(result).map_err(internal_err)
}
