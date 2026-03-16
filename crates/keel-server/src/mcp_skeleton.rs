//! MCP skeleton handler — signature-only view of a file (no function bodies).

use serde_json::Value;

use keel_core::store::GraphStore;
use keel_core::types::NodeKind;

use crate::mcp::{lock_store, JsonRpcError, SharedStore};

/// Handle the `keel/skeleton` MCP tool call to return a signature-only view of a file.
pub(crate) fn handle_skeleton(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let file = params
        .as_ref()
        .and_then(|p| p.get("file"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError {
            code: -32602,
            message: "Missing 'file' parameter".into(),
        })?
        .to_string();

    let docs = params
        .as_ref()
        .and_then(|p| p.get("docs"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let private = params
        .as_ref()
        .and_then(|p| p.get("private"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let store = lock_store(store)?;
    let nodes = store.get_nodes_in_file(&file);
    if nodes.is_empty() {
        return Err(JsonRpcError {
            code: -32602,
            message: format!("No graph data for file: {}", file),
        });
    }

    let symbols: Vec<Value> = nodes
        .iter()
        .filter(|n| n.kind != NodeKind::Module)
        .filter(|n| private || n.is_public)
        .map(|node| {
            let mut obj = serde_json::json!({
                "name": node.name,
                "hash": node.hash,
                "kind": node.kind.as_str(),
                "line_start": node.line_start,
                "line_end": node.line_end,
                "is_public": node.is_public,
                "signature": node.signature,
            });
            if docs {
                if let Some(ref ds) = node.docstring {
                    obj["docstring"] = Value::String(ds.clone());
                }
            }
            obj
        })
        .collect();

    let func_count = symbols
        .iter()
        .filter(|s| s.get("kind").and_then(|k| k.as_str()) == Some("function"))
        .count();
    let class_count = symbols
        .iter()
        .filter(|s| s.get("kind").and_then(|k| k.as_str()) == Some("class"))
        .count();

    Ok(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "command": "skeleton",
        "file": file,
        "summary": {
            "functions": func_count,
            "classes": class_count,
            "total": symbols.len(),
        },
        "symbols": symbols,
    }))
}
