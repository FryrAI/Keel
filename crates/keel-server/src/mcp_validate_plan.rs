//! MCP `keel/validate-plan` handler.
//!
//! Shares the deterministic [`keel_enforce::validate_plan`] core with the CLI.

use serde_json::Value;

use crate::mcp::{internal_err, lock_store, JsonRpcError, SharedStore};

/// Handle the `keel/validate-plan` MCP tool call.
pub(crate) fn handle_validate_plan(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let plan = params
        .as_ref()
        .and_then(|p| p.get("plan"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError {
            code: -32602,
            message: "Missing 'plan' parameter".into(),
        })?
        .to_string();

    let result = {
        let store = lock_store(store)?;
        keel_enforce::validate_plan::validate_plan(&*store, &plan)
    };

    serde_json::to_value(result).map_err(internal_err)
}
