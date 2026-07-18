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
    // `None` = the member was absent (a notification). An explicit
    // `"id": null` must survive as `Some(Value::Null)` — plain
    // `Option<Value>` would collapse it to `None` — because JSON-RPC 2.0
    // treats a present-but-null id as a request that requires a response.
    #[serde(default, deserialize_with = "deserialize_present_id")]
    id: Option<Value>,
}

/// Deserializer for `id` that maps a *present* JSON `null` to
/// `Some(Value::Null)` instead of `None`, so absent and null differ.
fn deserialize_present_id<'de, D>(d: D) -> Result<Option<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Value::deserialize(d).map(Some)
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

#[derive(Debug, Serialize)]
pub(crate) struct JsonRpcError {
    pub(crate) code: i64,
    pub(crate) message: String,
}

pub(crate) use crate::mcp_tools::{dispatch_tool, tool_list};

/// Protocol version advertised when the client does not request one (or
/// requests one we don't know).
const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";

/// Protocol revisions this server knows how to speak.
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

/// Build the `initialize` result, negotiating the protocol version: echo the
/// client's requested version when we support it, otherwise answer with our
/// default — per MCP, the server must never claim a version it doesn't speak.
fn handle_initialize(params: Option<Value>) -> Value {
    let protocol_version = params
        .as_ref()
        .and_then(|p| p.get("protocolVersion"))
        .and_then(|v| v.as_str())
        .filter(|v| SUPPORTED_PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(DEFAULT_PROTOCOL_VERSION);

    serde_json::json!({
        "protocolVersion": protocol_version,
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "keel",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

fn dispatch(
    store: &SharedStore,
    engine: &SharedEngine,
    method: &str,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    match method {
        "initialize" => Ok(handle_initialize(params)),
        // MCP requires the manifest be wrapped as {"tools": [...]}, not a bare array.
        "tools/list" => Ok(serde_json::json!({
            "tools": serde_json::to_value(tool_list()).map_err(internal_err)?
        })),
        "tools/call" => crate::mcp_tools::handle_tools_call(store, engine, params),
        // Legacy back-compat: tool names accepted as direct JSON-RPC methods.
        _ => dispatch_tool(store, engine, method, params).unwrap_or_else(|| {
            Err(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", method),
            })
        }),
    }
}

/// Process a single JSON-RPC line and return the response JSON string.
///
/// Returns an empty string when no response must be sent: for blank lines and
/// for JSON-RPC *notifications* (messages without an `id`), which per the
/// JSON-RPC 2.0 spec MUST NOT be answered.
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

    // JSON-RPC 2.0: a message with NO `id` member is a notification and gets
    // no response (we ignore them entirely — none of ours have side effects).
    // An explicit `"id": null` is a regular request and MUST be answered.
    let id = match request.id.clone() {
        Some(id) => id,
        None => return String::new(),
    };

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
