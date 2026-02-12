//! MCP (Model Context Protocol) JSON-RPC server over stdin/stdout.

use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::EdgeDirection;
use keel_enforce::types::{
    CalleeInfo, CallerInfo, DiscoverResult, ExplainResult,
    ModuleContext, NodeInfo, ResolutionStep,
};

pub(crate) type SharedStore = Arc<Mutex<SqliteGraphStore>>;

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    method: String,
    params: Option<Value>,
    id: Option<Value>,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    id: Value,
}

#[derive(Serialize)]
pub(crate) struct JsonRpcError {
    pub(crate) code: i64,
    pub(crate) message: String,
}

#[derive(Serialize, Deserialize)]
struct ToolInfo {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

fn tool_list() -> Vec<ToolInfo> {
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
                    "scope": { "type": "array", "items": { "type": "string" } }
                }
            }),
        },
    ]
}

fn dispatch(store: &SharedStore, method: &str, params: Option<Value>) -> Result<Value, JsonRpcError> {
    match method {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "keel",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "tools/list" => serde_json::to_value(tool_list()).map_err(internal_err),
        "keel/compile" => crate::mcp_compile::handle_compile(store, params),
        "keel/discover" => handle_discover(store, params),
        "keel/where" => handle_where(store, params),
        "keel/explain" => handle_explain(store, params),
        "keel/map" => handle_map(params),
        _ => Err(JsonRpcError {
            code: -32601,
            message: format!("Method not found: {}", method),
        }),
    }
}

/// Process a single JSON-RPC line and return the response JSON string.
pub fn process_line(store: &SharedStore, line: &str) -> String {
    if line.trim().is_empty() {
        return String::new();
    }

    let request: JsonRpcRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            let err_resp = JsonRpcResponse {
                jsonrpc: "2.0".into(),
                result: None,
                error: Some(JsonRpcError {
                    code: -32700,
                    message: format!("Parse error: {}", e),
                }),
                id: Value::Null,
            };
            return serde_json::to_string(&err_resp).unwrap_or_default();
        }
    };

    let id = request.id.clone().unwrap_or(Value::Null);
    let response = match dispatch(store, &request.method, request.params) {
        Ok(result) => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(result),
            error: None,
            id,
        },
        Err(error) => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(error),
            id,
        },
    };

    serde_json::to_string(&response).unwrap_or_default()
}

pub(crate) fn internal_err(e: impl std::fmt::Display) -> JsonRpcError {
    JsonRpcError { code: -32603, message: e.to_string() }
}

fn missing_param(name: &str) -> JsonRpcError {
    JsonRpcError { code: -32602, message: format!("Missing '{}' parameter", name) }
}

fn not_found(hash: &str) -> JsonRpcError {
    JsonRpcError { code: -32602, message: format!("Node not found: {}", hash) }
}

pub(crate) fn lock_store(store: &SharedStore) -> Result<std::sync::MutexGuard<'_, SqliteGraphStore>, JsonRpcError> {
    store.lock().map_err(|_| JsonRpcError { code: -32603, message: "Store lock poisoned".into() })
}

fn handle_discover(store: &SharedStore, params: Option<Value>) -> Result<Value, JsonRpcError> {
    let hash = params
        .as_ref()
        .and_then(|p| p.get("hash"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| missing_param("hash"))?
        .to_string();

    let store = lock_store(store)?;
    let node = store.get_node(&hash).ok_or_else(|| not_found(&hash))?;

    let incoming = store.get_edges(node.id, EdgeDirection::Incoming);
    let outgoing = store.get_edges(node.id, EdgeDirection::Outgoing);

    let upstream: Vec<CallerInfo> = incoming
        .iter()
        .filter_map(|e| {
            store.get_node_by_id(e.source_id).map(|n| CallerInfo {
                hash: n.hash, name: n.name, signature: n.signature,
                file: n.file_path, line: n.line_start,
                docstring: n.docstring, call_line: e.line,
            })
        })
        .collect();

    let downstream: Vec<CalleeInfo> = outgoing
        .iter()
        .filter_map(|e| {
            store.get_node_by_id(e.target_id).map(|n| CalleeInfo {
                hash: n.hash, name: n.name, signature: n.signature,
                file: n.file_path, line: n.line_start,
                docstring: n.docstring, call_line: e.line,
            })
        })
        .collect();

    let result = DiscoverResult {
        version: env!("CARGO_PKG_VERSION").into(),
        command: "discover".into(),
        target: NodeInfo {
            hash: node.hash, name: node.name, signature: node.signature,
            file: node.file_path, line_start: node.line_start,
            line_end: node.line_end, docstring: node.docstring,
            type_hints_present: node.type_hints_present,
            has_docstring: node.has_docstring,
        },
        upstream,
        downstream,
        module_context: ModuleContext {
            module: String::new(),
            sibling_functions: vec![],
            responsibility_keywords: vec![],
            function_count: 0,
            external_endpoints: vec![],
        },
    };

    serde_json::to_value(result).map_err(internal_err)
}

fn handle_where(store: &SharedStore, params: Option<Value>) -> Result<Value, JsonRpcError> {
    let hash = params
        .as_ref()
        .and_then(|p| p.get("hash"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| missing_param("hash"))?
        .to_string();

    let store = lock_store(store)?;
    let node = store.get_node(&hash).ok_or_else(|| not_found(&hash))?;

    serde_json::to_value(serde_json::json!({
        "file": node.file_path,
        "line_start": node.line_start,
        "line_end": node.line_end,
        "stale": false,
    }))
    .map_err(internal_err)
}

fn handle_explain(store: &SharedStore, params: Option<Value>) -> Result<Value, JsonRpcError> {
    let error_code = params
        .as_ref()
        .and_then(|p| p.get("error_code"))
        .and_then(|v| v.as_str())
        .unwrap_or("E001")
        .to_string();

    let hash = params
        .as_ref()
        .and_then(|p| p.get("hash"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| missing_param("hash"))?
        .to_string();

    let store = lock_store(store)?;
    let node = store.get_node(&hash).ok_or_else(|| not_found(&hash))?;

    let result = ExplainResult {
        version: env!("CARGO_PKG_VERSION").into(),
        command: "explain".into(),
        error_code,
        hash: node.hash.clone(),
        confidence: 1.0,
        resolution_tier: "tree-sitter".into(),
        resolution_chain: vec![ResolutionStep {
            kind: "lookup".into(),
            file: node.file_path,
            line: node.line_start,
            text: format!("Node '{}' found via hash lookup", node.name),
        }],
        summary: format!("Resolved '{}' via tree-sitter (Tier 1)", node.name),
    };

    serde_json::to_value(result).map_err(internal_err)
}

fn handle_map(params: Option<Value>) -> Result<Value, JsonRpcError> {
    let format = params
        .as_ref()
        .and_then(|p| p.get("format"))
        .and_then(|v| v.as_str())
        .unwrap_or("json");

    let scope: Vec<String> = params
        .as_ref()
        .and_then(|p| p.get("scope").cloned())
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    Ok(serde_json::json!({
        "status": "ok",
        "format": format,
        "scope": scope,
        "message": "Map not yet implemented — requires keel-parsers integration"
    }))
}

/// Run the MCP server loop, reading JSON-RPC from stdin and writing to stdout.
pub fn run_stdio(store: SharedStore) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        let response = process_line(&store, &line);
        if response.is_empty() {
            continue;
        }
        let mut out = stdout.lock();
        writeln!(out, "{}", response)?;
        out.flush()?;
    }

    Ok(())
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
