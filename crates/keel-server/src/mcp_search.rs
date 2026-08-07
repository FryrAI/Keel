//! MCP search handler — search graph nodes by name substring.

use serde_json::Value;

use crate::mcp::{lock_store, param_str, param_str_opt, param_u64, JsonRpcError, SharedStore};

/// Handle the `keel/search` MCP tool call to search graph nodes by name substring.
pub(crate) fn handle_search(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let query = param_str(&params, "query")?.to_string();
    let kind_filter = param_str_opt(&params, "kind").map(str::to_string);
    let limit = param_u64(&params, "limit", 20) as usize;

    let store = lock_store(store)?;

    // Route through the shared search implementation so MCP `keel/search`,
    // the CLI `keel search`, and the HTTP `/search` route return identical
    // results in identical order (exact-match-first, then substring).
    let nodes = keel_enforce::queries::search_graph(&*store, &query, kind_filter.as_deref(), limit);

    let results: Vec<Value> = nodes
        .iter()
        .map(|node| {
            serde_json::json!({
                "hash": node.hash,
                "name": node.name,
                "kind": node.kind.as_str(),
                "file": node.file_path,
                "line_start": node.line_start,
                "line_end": node.line_end,
                "signature": node.signature,
                "is_public": node.is_public,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "query": query,
        "count": results.len(),
        "results": results,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use keel_core::sqlite::SqliteGraphStore;
    use keel_core::types::{GraphNode, NodeKind};

    fn node(id: u64, name: &str, kind: NodeKind) -> GraphNode {
        GraphNode {
            complexity: 0,
            id,
            hash: format!("hash{id:08}"),
            kind,
            name: name.to_string(),
            signature: format!("fn {name}()"),
            file_path: "src/lib.rs".to_string(),
            line_start: id as u32,
            line_end: id as u32 + 1,
            docstring: None,
            is_public: true,
            type_hints_present: true,
            has_docstring: false,
            is_associated: false,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 0,
            package: None,
        }
    }

    fn fixture() -> Arc<Mutex<SqliteGraphStore>> {
        let store = SqliteGraphStore::in_memory().unwrap();
        store
            .insert_node(&node(1, "module_lib", NodeKind::Module))
            .unwrap();
        store
            .insert_node(&node(2, "parse", NodeKind::Function))
            .unwrap();
        store
            .insert_node(&node(3, "parse_body", NodeKind::Function))
            .unwrap();
        store
            .insert_node(&node(4, "reparse_all", NodeKind::Function))
            .unwrap();
        Arc::new(Mutex::new(store))
    }

    /// The MCP `keel/search` tool and the shared `search_graph` (which the CLI
    /// `keel search` also calls) must return identical results in identical
    /// order — that is the CLI↔MCP parity guarantee.
    #[test]
    fn mcp_search_matches_shared_search_graph() {
        let store = fixture();
        let params = serde_json::json!({ "query": "pars", "limit": 20 });
        let resp = handle_search(&store, Some(params)).unwrap();

        let mcp_hashes: Vec<String> = resp["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["hash"].as_str().unwrap().to_string())
            .collect();

        let shared = keel_enforce::queries::search_graph(&*store.lock().unwrap(), "pars", None, 20);
        let shared_hashes: Vec<String> = shared.iter().map(|n| n.hash.clone()).collect();

        assert_eq!(mcp_hashes, shared_hashes);
        assert!(!mcp_hashes.is_empty());
    }

    #[test]
    fn mcp_search_uses_file_key() {
        let store = fixture();
        let params = serde_json::json!({ "query": "parse" });
        let resp = handle_search(&store, Some(params)).unwrap();
        let first = &resp["results"][0];
        assert!(
            first.get("file").is_some(),
            "results must use the `file` key"
        );
        assert!(first.get("file_path").is_none());
    }
}
