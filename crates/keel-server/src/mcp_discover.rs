//! MCP handlers for discover, where, explain, and map operations.

use serde_json::Value;

use crate::mcp::{
    internal_err, lock_store, missing_param, not_found, JsonRpcError, SharedEngine, SharedStore,
};
use keel_core::store::GraphStore;
use keel_enforce::types::{ExplainResult, ResolutionStep};

pub(crate) fn handle_discover(
    engine: &SharedEngine,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
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

pub(crate) fn handle_where(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
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

pub(crate) fn handle_explain(
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

pub(crate) fn handle_map(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
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

        if format == "llm" {
            let mut text = format!("FILE {} ({} nodes):\n", path, nodes.len());
            for n in &nodes {
                text.push_str(&format!(
                    "  {} [{}] hash={} pub={} L{}-L{}\n",
                    n.name,
                    n.kind.as_str(),
                    n.hash,
                    n.is_public,
                    n.line_start,
                    n.line_end,
                ));
            }
            Ok(serde_json::json!({
                "status": "ok",
                "format": "llm",
                "text": text,
            }))
        } else {
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
                "format": "json",
                "scope": scope,
                "file_path": path,
                "nodes": node_entries,
            }))
        }
    } else {
        // Full-graph summary: enumerate all modules and their nodes
        let modules = store.get_all_modules();
        let mut total_nodes: usize = 0;

        if format == "llm" {
            let mut text = String::new();
            for m in &modules {
                let nodes = store.get_nodes_in_file(&m.file_path);
                total_nodes += nodes.len();
                text.push_str(&format!("MODULE {} nodes={}\n", m.file_path, nodes.len(),));
            }
            let header = format!("MAP modules={} nodes={}\n", modules.len(), total_nodes,);
            Ok(serde_json::json!({
                "status": "ok",
                "format": "llm",
                "text": format!("{}{}", header, text),
            }))
        } else {
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
                "format": "json",
                "scope": scope,
                "module_count": modules.len(),
                "total_nodes": total_nodes,
                "modules": module_entries,
            }))
        }
    }
}
