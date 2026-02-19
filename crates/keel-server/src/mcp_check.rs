//! MCP check handler — pre-edit risk assessment for a node.

use serde_json::Value;

use crate::mcp::{internal_err, JsonRpcError, SharedEngine};

pub(crate) fn handle_check(
    engine: &SharedEngine,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let hash = params
        .as_ref()
        .and_then(|p| p.get("hash"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError {
            code: -32602,
            message: "Missing 'hash' parameter".into(),
        })?
        .to_string();

    let engine = engine.lock().map_err(|_| JsonRpcError {
        code: -32603,
        message: "Engine lock poisoned".into(),
    })?;

    let result = engine.check(&hash).ok_or_else(|| JsonRpcError {
        code: -32602,
        message: format!("Node not found: {}", hash),
    })?;

    serde_json::to_value(result).map_err(internal_err)
}
