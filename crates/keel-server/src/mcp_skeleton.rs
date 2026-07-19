//! MCP skeleton handler — a compressed, signature-only view of a file.
//!
//! Delegates the whole path-resolve → read → parse preamble to
//! `keel_enforce::skeleton::build_skeleton_from_path`, the same function the
//! CLI `keel skeleton` command uses, so the two return identical results for
//! the same file and flags.

use std::path::Path;

use serde_json::Value;

use crate::mcp::{internal_err, param_bool, param_str, JsonRpcError};

/// Handle the `keel/skeleton` MCP tool call.
///
/// `root` is the server's authoritative project root; a relative `file` is
/// resolved against it rather than the ambient process cwd.
pub(crate) fn handle_skeleton(root: &Path, params: Option<Value>) -> Result<Value, JsonRpcError> {
    let file = param_str(&params, "file")?.to_string();
    let docs = param_bool(&params, "docs", false);
    let private = param_bool(&params, "private", false);

    let result = keel_enforce::skeleton::build_skeleton_from_path(root, &file, private, docs)
        .map_err(|message| JsonRpcError {
            code: -32602,
            message,
        })?;

    serde_json::to_value(result).map_err(internal_err)
}
