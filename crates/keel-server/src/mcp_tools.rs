//! MCP tool registry and `tools/call` routing.
//!
//! Holds the tool manifest returned by `tools/list` and the dispatcher that
//! maps a tool name to its handler. Split out of `mcp.rs` to keep both files
//! under the 400-line cap.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::mcp::{JsonRpcError, SharedEngine, SharedStore};

/// A single entry in the MCP `tools/list` manifest.
#[derive(Serialize, Deserialize)]
pub(crate) struct ToolInfo {
    pub(crate) name: String,
    pub(crate) description: String,
    #[serde(rename = "inputSchema")]
    pub(crate) input_schema: Value,
}

/// Build the manifest of every tool this server exposes.
pub(crate) fn tool_list() -> Vec<ToolInfo> {
    vec![
        ToolInfo {
            name: "keel/compile".into(),
            description: "Compile files and check for violations".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "files": { "type": "array", "items": { "type": "string" } },
                    "batch_start": { "type": "boolean" },
                    "batch_end": { "type": "boolean" }
                }
            }),
        },
        ToolInfo {
            name: "keel/discover".into(),
            description: "Discover callers and callees of a node by hash".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["hash"],
                "properties": {
                    "hash": { "type": "string" },
                    "depth": { "type": "integer", "default": 1 }
                }
            }),
        },
        ToolInfo {
            name: "keel/where".into(),
            description: "Find file and line for a hash".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["hash"],
                "properties": {
                    "hash": { "type": "string" }
                }
            }),
        },
        ToolInfo {
            name: "keel/explain".into(),
            description: "Explain a violation with resolution chain".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["error_code", "hash"],
                "properties": {
                    "error_code": { "type": "string" },
                    "hash": { "type": "string" }
                }
            }),
        },
        ToolInfo {
            name: "keel/map".into(),
            description: "Full re-map of the codebase graph".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "format": { "type": "string", "enum": ["json", "llm"] },
                    "scope": { "type": "array", "items": { "type": "string" } },
                    "file": { "type": "string", "description": "Scope map to a single file" }
                }
            }),
        },
        ToolInfo {
            name: "keel/check".into(),
            description: "Pre-edit risk assessment: callers, callees, risk level, suggestions"
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["hash"],
                "properties": {
                    "hash": { "type": "string" }
                }
            }),
        },
        ToolInfo {
            name: "keel/fix".into(),
            description: "Compile files and generate fix plans for violations".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "files": { "type": "array", "items": { "type": "string" } }
                }
            }),
        },
        ToolInfo {
            name: "keel/search".into(),
            description: "Search graph nodes by name substring".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": { "type": "string" },
                    "kind": { "type": "string", "enum": ["function", "class", "module"] },
                    "limit": { "type": "integer", "default": 20 }
                }
            }),
        },
        ToolInfo {
            name: "keel/name".into(),
            description: "Suggest name and location for new code based on description".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["description"],
                "properties": {
                    "description": { "type": "string" },
                    "module": { "type": "string", "description": "Filter to modules matching this path substring" },
                    "kind": { "type": "string", "enum": ["function", "class"] }
                }
            }),
        },
        ToolInfo {
            name: "keel/analyze".into(),
            description: "Analyze a file for structure, code smells, and refactoring opportunities"
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["file"],
                "properties": {
                    "file": { "type": "string" }
                }
            }),
        },
        ToolInfo {
            name: "keel/audit".into(),
            description:
                "AI-readiness scorecard: structure, discoverability, navigation, agent config"
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "dimension": { "type": "string", "enum": ["structure", "discoverability", "navigation", "config"] },
                    "strict": { "type": "boolean", "default": false }
                }
            }),
        },
        ToolInfo {
            name: "keel/context".into(),
            description: "Minimal structural context for safely editing a file: symbols, external callers, and external callees".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["file"],
                "properties": {
                    "file": { "type": "string" }
                }
            }),
        },
    ]
}

/// Route a tool name to its handler.
///
/// Returns `None` when `name` is not a registered tool, letting callers choose
/// the appropriate error (`-32601` for legacy direct methods, `-32602` for a
/// bad `tools/call` argument).
pub(crate) fn dispatch_tool(
    store: &SharedStore,
    engine: &SharedEngine,
    name: &str,
    arguments: Option<Value>,
) -> Option<Result<Value, JsonRpcError>> {
    Some(match name {
        "keel/compile" => crate::mcp_compile::handle_compile(engine, arguments),
        "keel/discover" => crate::mcp_discover::handle_discover(engine, arguments),
        "keel/where" => crate::mcp_discover::handle_where(store, arguments),
        "keel/explain" => crate::mcp_discover::handle_explain(engine, arguments),
        "keel/map" => crate::mcp_discover::handle_map(store, arguments),
        "keel/check" => crate::mcp_check::handle_check(engine, arguments),
        "keel/fix" => crate::mcp_fix::handle_fix(store, engine, arguments),
        "keel/search" => crate::mcp_search::handle_search(store, arguments),
        "keel/name" => crate::mcp_name::handle_name(store, arguments),
        "keel/analyze" => crate::mcp_analyze::handle_analyze(store, arguments),
        "keel/audit" => crate::mcp_audit::handle_audit(store, arguments),
        "keel/context" => crate::mcp_context::handle_context(store, arguments),
        _ => return None,
    })
}

/// Handle an MCP `tools/call` request.
///
/// Extracts `params.name` and `params.arguments`, routes to the tool handler,
/// and wraps the outcome in an MCP `CallToolResult` (`content` array carrying
/// text). Three outcomes: success → payload JSON with `isError: false`; tool
/// EXECUTION failure → the error message with `isError: true` (in-band, per
/// MCP spec, so the model can read it); unregistered tool name → JSON-RPC
/// `-32602` (a protocol fault, not a tool failure).
pub(crate) fn handle_tools_call(
    store: &SharedStore,
    engine: &SharedEngine,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let name = params
        .as_ref()
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::mcp::missing_param("name"))?
        .to_string();

    let arguments = params.as_ref().and_then(|p| p.get("arguments").cloned());

    let outcome = dispatch_tool(store, engine, &name, arguments).ok_or_else(|| JsonRpcError {
        code: -32602,
        message: format!("Unknown tool: {}", name),
    })?;

    match outcome {
        // `Value`'s `Display` is compact JSON — no human reads it, and the
        // client parses it straight back out.
        Ok(payload) => Ok(call_tool_result(payload.to_string(), false)),
        // MCP: failures while EXECUTING a tool are reported in-band via
        // `isError: true` so the model can read the message and recover;
        // JSON-RPC errors are reserved for protocol faults (unknown tool).
        Err(e) => Ok(call_tool_result(e.message, true)),
    }
}

/// Build an MCP `CallToolResult` carrying `text` as its single content block.
///
/// [`tool_payload`] is the inverse for success results; keep the pair in
/// sync (see the roundtrip test below).
fn call_tool_result(text: String, is_error: bool) -> Value {
    serde_json::json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    })
}

/// Unwrap a tool's payload from the `result` of a JSON-RPC response.
///
/// `tools/call` responses carry the payload as JSON text inside a
/// `CallToolResult` (built by [`call_tool_result`]), so unwrap it; legacy
/// direct-method responses already *are* the payload. Keeps compile
/// error/warning extraction working on both paths.
pub(crate) fn tool_payload(result: &Value) -> Value {
    result
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|blocks| blocks.first())
        .and_then(|block| block.get("text"))
        .and_then(|text| text.as_str())
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_else(|| result.clone())
}

#[cfg(test)]
mod wrap_roundtrip_tests {
    use super::*;

    #[test]
    fn test_tool_payload_inverts_call_tool_result() {
        let payload = serde_json::json!({
            "errors": [{ "code": "E001" }, { "code": "E004" }],
            "warnings": [{ "code": "W001" }],
        });
        let wrapped = call_tool_result(payload.to_string(), false);
        assert_eq!(tool_payload(&wrapped), payload);
    }

    #[test]
    fn test_error_shape_carries_message_and_flag() {
        let wrapped = call_tool_result("Node not found: x".into(), true);
        assert_eq!(wrapped["isError"], true);
        assert_eq!(wrapped["content"][0]["text"], "Node not found: x");
    }
}
