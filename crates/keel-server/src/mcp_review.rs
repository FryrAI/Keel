//! MCP `keel/review` handler — the two-sided graph diff as a PR cover letter.
//!
//! Shares the deterministic [`keel_enforce::review`] core with the CLI. Git runs
//! in the server's authoritative project root, not the ambient process cwd.

use std::path::Path;

use serde_json::Value;

use crate::mcp::{internal_err, lock_store, param_str, JsonRpcError, SharedStore};

/// Handle the `keel/review` MCP tool call.
///
/// A base ref git cannot resolve is a tool *execution* failure (`-32603`), not
/// a protocol fault — the caller passed a well-formed argument that turned out
/// to be wrong, and the message says which ref.
pub(crate) fn handle_review(
    store: &SharedStore,
    root: &Path,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let base = param_str(&params, "base")?.to_string();
    // The repo's own check toggles, so the baseline diff cannot report a code
    // the repo turned off in `keel.json`.
    let enforce = keel_core::config::KeelConfig::load(&keel_core::paths::keel_dir(root)).enforce;

    let result = {
        let store = lock_store(store)?;
        keel_enforce::review::review(&*store, root, &base, &enforce).map_err(|e| JsonRpcError {
            code: -32603,
            message: e,
        })?
    };

    serde_json::to_value(result).map_err(internal_err)
}
