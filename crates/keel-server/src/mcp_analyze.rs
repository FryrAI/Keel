//! MCP analyze handler — file structure, smells, and refactoring opportunities.

use serde_json::Value;

use keel_enforce::analyze::analyze_file;

use crate::mcp::{internal_err, lock_store, JsonRpcError, SharedStore};

/// Handle the `keel/analyze` MCP tool call to return file structure and code smells.
pub(crate) fn handle_analyze(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let file = params
        .as_ref()
        .and_then(|p| p.get("file"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError {
            code: -32602,
            message: "Missing 'file' parameter".into(),
        })?
        .to_string();

    let store = lock_store(store)?;
    let result = analyze_file(&*store, &file).ok_or_else(|| JsonRpcError {
        code: -32602,
        message: format!("No graph data for file: {}", file),
    })?;

    serde_json::to_value(result).map_err(internal_err)
}
