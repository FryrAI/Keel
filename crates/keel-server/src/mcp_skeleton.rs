//! MCP skeleton handler — a compressed, signature-only view of a file.
//!
//! Reads the file and delegates to `keel_enforce::skeleton::build_skeleton`,
//! the same function the CLI `keel skeleton` command uses, so the two return
//! identical results for the same file and flags.

use std::path::Path;

use serde_json::Value;

use crate::mcp::{internal_err, missing_param, JsonRpcError};

/// Handle the `keel/skeleton` MCP tool call.
pub(crate) fn handle_skeleton(params: Option<Value>) -> Result<Value, JsonRpcError> {
    let file = params
        .as_ref()
        .and_then(|p| p.get("file"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| missing_param("file"))?
        .to_string();

    let docs = params
        .as_ref()
        .and_then(|p| p.get("docs"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let private = params
        .as_ref()
        .and_then(|p| p.get("private"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let cwd = std::env::current_dir().map_err(internal_err)?;
    let path = Path::new(&file);
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };

    let content = std::fs::read_to_string(&full).map_err(|e| JsonRpcError {
        code: -32602,
        message: format!("cannot read {}: {}", file, e),
    })?;

    let result = keel_enforce::skeleton::build_skeleton(&cwd, path, &content, private, docs)
        .ok_or_else(|| JsonRpcError {
            code: -32602,
            message: format!("unsupported file type: {}", file),
        })?;

    serde_json::to_value(result).map_err(internal_err)
}
