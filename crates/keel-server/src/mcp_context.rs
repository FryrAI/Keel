//! MCP context handler — minimal structural context for safely editing a file.

use serde_json::Value;

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, NodeKind};

use crate::mcp::{lock_store, JsonRpcError, SharedStore};

/// Handle the `keel/context` MCP tool call to return symbols and their external callers/callees.
pub(crate) fn handle_context(
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
        .map(|node| {
            let incoming = store.get_edges(node.id, EdgeDirection::Incoming);
            let outgoing = store.get_edges(node.id, EdgeDirection::Outgoing);

            let callers: Vec<Value> = incoming
                .iter()
                .filter_map(|e| {
                    let src = store.get_node_by_id(e.source_id)?;
                    if src.file_path == file {
                        return None;
                    }
                    Some(serde_json::json!({
                        "name": src.name, "file": src.file_path, "line": src.line_start
                    }))
                })
                .collect();

            let callees: Vec<Value> = outgoing
                .iter()
                .filter_map(|e| {
                    let tgt = store.get_node_by_id(e.target_id)?;
                    if tgt.file_path == file {
                        return None;
                    }
                    Some(serde_json::json!({
                        "name": tgt.name, "file": tgt.file_path, "line": tgt.line_start
                    }))
                })
                .collect();

            serde_json::json!({
                "name": node.name,
                "hash": node.hash,
                "kind": node.kind.as_str(),
                "line_start": node.line_start,
                "line_end": node.line_end,
                "is_public": node.is_public,
                "signature": node.signature,
                "callers": callers,
                "callees": callees,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "command": "context",
        "file": file,
        "symbols": symbols,
    }))
}
