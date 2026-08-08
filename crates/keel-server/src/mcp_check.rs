//! MCP check handler — pre-edit risk assessment for a node.

use serde_json::Value;

use crate::mcp::{internal_err, lock_engine, param_str, JsonRpcError, SharedEngine};

/// Handle the `keel/check` MCP tool call to perform pre-edit risk assessment on a node.
pub(crate) fn handle_check(
    engine: &SharedEngine,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let hash = param_str(&params, "hash")?.to_string();

    let engine = lock_engine(engine)?;

    let result = engine.check(&hash).ok_or_else(|| JsonRpcError {
        code: -32602,
        message: format!("Node not found: {}", hash),
    })?;

    serde_json::to_value(result).map_err(internal_err)
}
