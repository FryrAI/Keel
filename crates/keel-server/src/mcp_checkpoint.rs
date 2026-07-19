//! MCP `keel/checkpoint` handler — compact session-state summary.
//!
//! Shares the deterministic [`keel_enforce::checkpoint`] core with the CLI so
//! the two interfaces never diverge. Runs git in the server's working
//! directory (the project root).

use serde_json::Value;

use keel_enforce::checkpoint::{self, CheckpointMode};

use crate::mcp::{internal_err, lock_store, JsonRpcError, SharedEngine, SharedStore};
use crate::parse_shared::FileParser;

/// Handle the `keel/checkpoint` MCP tool call.
pub(crate) fn handle_checkpoint(
    store: &SharedStore,
    engine: &SharedEngine,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let since = params
        .as_ref()
        .and_then(|p| p.get("since"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let staged = params
        .as_ref()
        .and_then(|p| p.get("staged"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let cwd = std::env::current_dir().map_err(internal_err)?;
    let mode = if staged {
        CheckpointMode::Staged
    } else {
        CheckpointMode::Since(since)
    };

    // Parse changed files. Git returns repo-relative paths and the server runs
    // at the repo root, so pass them straight through — that keeps the parsed
    // `file_path` relative and matching the stored graph.
    let changed = checkpoint::changed_files(&cwd, &mode);
    let mut parser = FileParser::new();
    let file_indices: Vec<_> = changed.iter().filter_map(|f| parser.parse(f)).collect();

    // Diff against the PRE-edit graph BEFORE compiling: `engine.compile`
    // persists re-baselined hashes, which would erase the reported change.
    let diff = {
        let store = lock_store(store)?;
        checkpoint::diff_changed_files(&*store, &file_indices)
    };

    let compile_result = {
        let mut eng = engine.lock().map_err(|_| JsonRpcError {
            code: -32603,
            message: "Engine lock poisoned".into(),
        })?;
        eng.compile(&file_indices)
    };

    let commits = checkpoint::commit_subjects(&cwd, &mode);
    let range = checkpoint::range_label(&mode);
    let result = checkpoint::build_checkpoint(diff, &compile_result, range, commits);

    serde_json::to_value(result).map_err(internal_err)
}
