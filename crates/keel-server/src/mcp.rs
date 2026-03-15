//! MCP (Model Context Protocol) JSON-RPC server over stdin/stdout.

use std::io;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use keel_core::sqlite::SqliteGraphStore;
use keel_enforce::engine::EnforcementEngine;

pub(crate) type SharedStore = Arc<Mutex<SqliteGraphStore>>;
pub type SharedEngine = Arc<Mutex<EnforcementEngine>>;

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
                    "scope": { "type": "array", "items": { "type": "string" } },
                    "file_path": { "type": "string", "description": "Scope map to a single file" }
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

fn dispatch(
    store: &SharedStore,
    engine: &SharedEngine,
    method: &str,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
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
        "keel/compile" => crate::mcp_compile::handle_compile(engine, params),
        "keel/discover" => crate::mcp_discover::handle_discover(engine, params),
        "keel/where" => crate::mcp_discover::handle_where(store, params),
        "keel/explain" => crate::mcp_discover::handle_explain(store, engine, params),
        "keel/map" => crate::mcp_discover::handle_map(store, params),
        "keel/check" => crate::mcp_check::handle_check(engine, params),
        "keel/fix" => crate::mcp_fix::handle_fix(store, engine, params),
        "keel/search" => crate::mcp_search::handle_search(store, params),
        "keel/name" => crate::mcp_name::handle_name(store, params),
        "keel/analyze" => crate::mcp_analyze::handle_analyze(store, params),
        "keel/audit" => crate::mcp_audit::handle_audit(store, params),
        "keel/context" => crate::mcp_context::handle_context(store, params),
        _ => Err(JsonRpcError {
            code: -32601,
            message: format!("Method not found: {}", method),
        }),
    }
}

/// Process a single JSON-RPC line and return the response JSON string.
pub fn process_line(store: &SharedStore, engine: &SharedEngine, line: &str) -> String {
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
    let response = match dispatch(store, engine, &request.method, request.params) {
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

/// Convert any `Display`-able error into a JSON-RPC internal error response.
pub(crate) fn internal_err(e: impl std::fmt::Display) -> JsonRpcError {
    JsonRpcError {
        code: -32603,
        message: e.to_string(),
    }
}

pub(crate) fn missing_param(name: &str) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: format!("Missing '{}' parameter", name),
    }
}

pub(crate) fn not_found(hash: &str) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: format!("Node not found: {}", hash),
    }
}

/// Acquire the shared graph store mutex, returning a JSON-RPC error if poisoned.
pub(crate) fn lock_store(
    store: &SharedStore,
) -> Result<std::sync::MutexGuard<'_, SqliteGraphStore>, JsonRpcError> {
    store.lock().map_err(|_| JsonRpcError {
        code: -32603,
        message: "Store lock poisoned".into(),
    })
}

/// Create a shared enforcement engine backed by a disk store with project config.
/// Falls back to in-memory store if db_path is None.
/// Circuit breaker and batch state persist across MCP calls within a session.
pub fn create_shared_engine(db_path: Option<&str>) -> SharedEngine {
    let engine_store: Box<dyn keel_core::store::GraphStore + Send> = match db_path {
        Some(path) => match SqliteGraphStore::open(path) {
            Ok(s) => Box::new(s),
            Err(_) => Box::new(
                SqliteGraphStore::in_memory()
                    .expect("Failed to create in-memory store for enforcement engine"),
            ),
        },
        None => Box::new(
            SqliteGraphStore::in_memory()
                .expect("Failed to create in-memory store for enforcement engine"),
        ),
    };

    // Load project config for enforce settings
    let config = db_path
        .and_then(|p| {
            std::path::Path::new(p)
                .parent() // .keel/
                .map(keel_core::config::KeelConfig::load)
        })
        .unwrap_or_default();

    Arc::new(Mutex::new(EnforcementEngine::with_config(
        engine_store,
        &config,
    )))
}

/// Run the MCP server loop, reading JSON-RPC from stdin and writing to stdout.
/// **Deprecated**: Use `mcp_stdio::run_stdio` for telemetry-instrumented version.
pub fn run_stdio(store: SharedStore, db_path: Option<&str>) -> io::Result<()> {
    // Delegate to the instrumented version with no keel_dir (backwards compat)
    crate::mcp_stdio::run_stdio(store, db_path, None, false)
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
