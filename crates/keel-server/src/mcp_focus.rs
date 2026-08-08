//! MCP focus handler — the minimal context set for safely modifying a target.
//!
//! Delegates to `EnforcementEngine::focus`, the same function the CLI `keel
//! focus` command uses, so the two return identical results.

use serde_json::Value;

use crate::mcp::{engine_lookup, JsonRpcError, SharedEngine};

/// Handle the `keel/focus` MCP tool call.
pub(crate) fn handle_focus(
    engine: &SharedEngine,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    engine_lookup(engine, params, "target", 2, |e, target, depth| {
        e.focus(target, depth)
    })
}
