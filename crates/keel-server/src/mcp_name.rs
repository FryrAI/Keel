//! MCP name handler — suggest name and location for new code.

use serde_json::Value;

use keel_enforce::naming::{suggest_name_with_options, NameOptions};

use crate::mcp::{
    internal_err, lock_store, param_bool, param_str, param_str_opt, JsonRpcError, SharedStore,
};

/// Handle the `keel/name` MCP tool call to suggest a name and location for new code.
pub(crate) fn handle_name(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let description = param_str(&params, "description")?.to_string();
    let module_filter = param_str_opt(&params, "module").map(str::to_string);
    let kind_filter = param_str_opt(&params, "kind").map(str::to_string);
    let semantic = param_bool(&params, "semantic", false);

    let store = lock_store(store)?;
    let result = suggest_name_with_options(
        &*store,
        &description,
        module_filter.as_deref(),
        kind_filter.as_deref(),
        NameOptions {
            semantic_candidates: semantic,
        },
    );

    serde_json::to_value(result).map_err(internal_err)
}
