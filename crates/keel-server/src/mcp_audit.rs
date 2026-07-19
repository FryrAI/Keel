//! MCP audit handler — AI-readiness scorecard via JSON-RPC.

use std::path::Path;

use serde_json::Value;

use crate::mcp::{internal_err, lock_store, param_bool, param_str_opt, JsonRpcError, SharedStore};

/// Handle the `keel/audit` MCP tool call.
///
/// `root` is the server's authoritative project root (not the ambient cwd), so
/// the audit always scores the same tree the server was started against.
pub(crate) fn handle_audit(
    store: &SharedStore,
    root: &Path,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let options = keel_enforce::types::AuditOptions {
        changed_only: false,
        strict: param_bool(&params, "strict", false),
        min_score: None,
        dimension: param_str_opt(&params, "dimension").map(str::to_string),
    };

    let store = lock_store(store)?;
    let result = keel_enforce::audit::audit_repo(&*store, root, &options, None);

    serde_json::to_value(result).map_err(internal_err)
}
