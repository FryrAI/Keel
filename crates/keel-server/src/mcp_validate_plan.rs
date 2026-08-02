//! MCP `keel/validate-plan` handler.
//!
//! Shares the deterministic [`keel_enforce::validate_plan`] core with the CLI.

use serde_json::Value;

use crate::mcp::{internal_err, lock_store, param_bool, param_str, JsonRpcError, SharedStore};

/// Handle the `keel/validate-plan` MCP tool call.
///
/// The envelope is the CLI's `PlanValidationResult`. `strict` (default `false`)
/// is the only new input: it adds a single `strict_failed` boolean saying
/// whether the CLI's `--strict` would have exited 1. Callers that omit it see
/// exactly the payload they saw before the `P` namespace existed — the
/// `findings` array is skipped when empty.
pub(crate) fn handle_validate_plan(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let plan = param_str(&params, "plan")?.to_string();
    let strict = param_bool(&params, "strict", false);

    let result = {
        let store = lock_store(store)?;
        keel_enforce::validate_plan::validate_plan(&*store, &plan)
    };

    let mut value = serde_json::to_value(&result).map_err(internal_err)?;
    if strict {
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "strict_failed".to_string(),
                Value::Bool(result.has_live_findings()),
            );
        }
    }
    Ok(value)
}
