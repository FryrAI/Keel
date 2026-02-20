//! MCP name handler — suggest name and location for new code.

use serde_json::Value;

use keel_enforce::naming::suggest_name;

use crate::mcp::{internal_err, lock_store, JsonRpcError, SharedStore};

pub(crate) fn handle_name(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let description = params
        .as_ref()
        .and_then(|p| p.get("description"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError {
            code: -32602,
            message: "Missing 'description' parameter".into(),
        })?
        .to_string();

    let module_filter = params
        .as_ref()
        .and_then(|p| p.get("module"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let kind_filter = params
        .as_ref()
        .and_then(|p| p.get("kind"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let store = lock_store(store)?;
    let result = suggest_name(
        &*store,
        &description,
        module_filter.as_deref(),
        kind_filter.as_deref(),
    );

    serde_json::to_value(result).map_err(internal_err)
}
