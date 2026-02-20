//! MCP (Model Context Protocol) JSON-RPC server over stdin/stdout.

use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_enforce::engine::EnforcementEngine;
use keel_enforce::types::{ExplainResult, ResolutionStep};

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
        "keel/discover" => handle_discover(engine, params),
        "keel/where" => handle_where(store, params),
        "keel/explain" => handle_explain(store, engine, params),
        "keel/map" => handle_map(store, params),
        "keel/check" => crate::mcp_check::handle_check(engine, params),
        "keel/fix" => crate::mcp_fix::handle_fix(store, engine, params),
        "keel/search" => crate::mcp_search::handle_search(store, params),
        "keel/name" => crate::mcp_name::handle_name(store, params),
        "keel/analyze" => crate::mcp_analyze::handle_analyze(store, params),
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

pub(crate) fn internal_err(e: impl std::fmt::Display) -> JsonRpcError {
    JsonRpcError {
        code: -32603,
        message: e.to_string(),
    }
}

fn missing_param(name: &str) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: format!("Missing '{}' parameter", name),
    }
}

fn not_found(hash: &str) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: format!("Node not found: {}", hash),
    }
}

pub(crate) fn lock_store(
    store: &SharedStore,
) -> Result<std::sync::MutexGuard<'_, SqliteGraphStore>, JsonRpcError> {
    store.lock().map_err(|_| JsonRpcError {
        code: -32603,
        message: "Store lock poisoned".into(),
    })
}

fn handle_discover(engine: &SharedEngine, params: Option<Value>) -> Result<Value, JsonRpcError> {
    let hash = params
        .as_ref()
        .and_then(|p| p.get("hash"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| missing_param("hash"))?
        .to_string();

    let depth = params
        .as_ref()
        .and_then(|p| p.get("depth"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;

    let engine = engine.lock().map_err(|_| JsonRpcError {
        code: -32603,
        message: "Engine lock poisoned".into(),
    })?;

    let result = engine
        .discover(&hash, depth)
        .ok_or_else(|| not_found(&hash))?;
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

fn handle_explain(
    store: &SharedStore,
    engine: &SharedEngine,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
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

    // Check circuit breaker state for this error_code+hash to get real confidence
    let eng = engine.lock().map_err(|_| JsonRpcError {
        code: -32603,
        message: "Engine lock poisoned".into(),
    })?;
    let cb_failures = eng.circuit_breaker_failures(&error_code, &hash, &node.file_path);
    let confidence = if cb_failures > 0 {
        (1.0 - (cb_failures as f64 * 0.2)).max(0.3)
    } else {
        1.0
    };

    let result = ExplainResult {
        version: env!("CARGO_PKG_VERSION").into(),
        command: "explain".into(),
        error_code,
        hash: node.hash.clone(),
        confidence,
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

fn handle_map(store: &SharedStore, params: Option<Value>) -> Result<Value, JsonRpcError> {
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

    let file_path = params
        .as_ref()
        .and_then(|p| p.get("file_path"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let store = lock_store(store)?;

    if let Some(ref path) = file_path {
        // File-scoped map: return nodes for a single file
        let nodes = store.get_nodes_in_file(path);
        let node_entries: Vec<Value> = nodes
            .iter()
            .map(|n| {
                serde_json::json!({
                    "name": n.name,
                    "hash": n.hash,
                    "kind": n.kind.as_str(),
                    "file": n.file_path,
                    "line_start": n.line_start,
                    "line_end": n.line_end,
                    "signature": n.signature,
                    "is_public": n.is_public,
                })
            })
            .collect();

        Ok(serde_json::json!({
            "status": "ok",
            "format": format,
            "scope": scope,
            "file_path": path,
            "nodes": node_entries,
        }))
    } else {
        // Full-graph summary: enumerate all modules and their nodes
        let modules = store.get_all_modules();
        let mut total_nodes: usize = 0;
        let module_entries: Vec<Value> = modules
            .iter()
            .map(|m| {
                let nodes = store.get_nodes_in_file(&m.file_path);
                total_nodes += nodes.len();
                serde_json::json!({
                    "name": m.name,
                    "file": m.file_path,
                    "node_count": nodes.len(),
                })
            })
            .collect();

        Ok(serde_json::json!({
            "status": "ok",
            "format": format,
            "scope": scope,
            "module_count": modules.len(),
            "total_nodes": total_nodes,
            "modules": module_entries,
        }))
    }
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
pub fn run_stdio(store: SharedStore, db_path: Option<&str>) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let engine = create_shared_engine(db_path);

    for line in stdin.lock().lines() {
        let line = line?;
        let response = process_line(&store, &engine, &line);
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
