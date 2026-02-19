//! MCP search handler — search graph nodes by name substring.

use serde_json::Value;

use keel_core::store::GraphStore;

use crate::mcp::{lock_store, JsonRpcError, SharedStore};

pub(crate) fn handle_search(store: &SharedStore, params: Option<Value>) -> Result<Value, JsonRpcError> {
    let query = params
        .as_ref()
        .and_then(|p| p.get("query"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError { code: -32602, message: "Missing 'query' parameter".into() })?
        .to_lowercase();

    let kind_filter = params
        .as_ref()
        .and_then(|p| p.get("kind"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let limit = params
        .as_ref()
        .and_then(|p| p.get("limit"))
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;

    let store = lock_store(store)?;
    let modules = store.get_all_modules();

    let mut results = Vec::new();
    for module in &modules {
        let nodes = store.get_nodes_in_file(&module.file_path);
        for node in &nodes {
            if node.name.to_lowercase().contains(&query) {
                if let Some(ref kind) = kind_filter {
                    if node.kind.as_str() != kind {
                        continue;
                    }
                }
                results.push(serde_json::json!({
                    "hash": node.hash,
                    "name": node.name,
                    "kind": node.kind.as_str(),
                    "file": node.file_path,
                    "line_start": node.line_start,
                    "line_end": node.line_end,
                    "signature": node.signature,
                    "is_public": node.is_public,
                }));
                if results.len() >= limit {
                    break;
                }
            }
        }
        if results.len() >= limit {
            break;
        }
    }

    Ok(serde_json::json!({
        "query": query,
        "count": results.len(),
        "results": results,
    }))
}
