//! MCP focus handler — the minimal context set for safely modifying a target.
//!
//! Delegates to `EnforcementEngine::focus`, the same function the CLI `keel
//! focus` command uses, so the two return identical results.

use serde_json::Value;

use crate::mcp::{internal_err, not_found, param_str, param_u32, JsonRpcError, SharedEngine};

/// Handle the `keel/focus` MCP tool call.
pub(crate) fn handle_focus(
    engine: &SharedEngine,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let target = param_str(&params, "target")?.to_string();
    let depth = param_u32(&params, "depth", 2);

    let engine = engine.lock().map_err(|_| JsonRpcError {
        code: -32603,
        message: "Engine lock poisoned".into(),
    })?;

    let result = engine
        .focus(&target, depth)
        .ok_or_else(|| not_found(&target))?;
    serde_json::to_value(result).map_err(internal_err)
}
