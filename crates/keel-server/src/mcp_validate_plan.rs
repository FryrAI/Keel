//! MCP `keel/validate-plan` handler.
//!
//! Shares the deterministic [`keel_enforce::validate_plan`] core with the CLI.

use serde_json::Value;

use crate::mcp::{internal_err, lock_store, param_str, JsonRpcError, SharedStore};

/// Handle the `keel/validate-plan` MCP tool call.
pub(crate) fn handle_validate_plan(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let plan = param_str(&params, "plan")?.to_string();

    let result = {
        let store = lock_store(store)?;
        keel_enforce::validate_plan::validate_plan(&*store, &plan)
    };

    serde_json::to_value(result).map_err(internal_err)
}
