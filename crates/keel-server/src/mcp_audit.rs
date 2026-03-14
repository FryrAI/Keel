//! MCP audit handler — AI-readiness scorecard via JSON-RPC.

use serde_json::Value;

use crate::mcp::{internal_err, lock_store, JsonRpcError, SharedStore};

/// Handle the `keel/audit` MCP tool call.
pub(crate) fn handle_audit(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let dimension = params
        .as_ref()
        .and_then(|p| p.get("dimension"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let strict = params
        .as_ref()
        .and_then(|p| p.get("strict"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let options = keel_enforce::types::AuditOptions {
        changed_only: false,
        strict,
        min_score: None,
        dimension,
    };

    let root_dir = std::env::current_dir().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Failed to get current directory: {}", e),
    })?;

    let store = lock_store(store)?;
    let result = keel_enforce::audit::audit_repo(&*store, &root_dir, &options, None);

    serde_json::to_value(result).map_err(internal_err)
}
