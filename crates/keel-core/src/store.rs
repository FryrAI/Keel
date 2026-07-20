use crate::types::{
    BodyIndexEntry, EdgeChange, EdgeDirection, GraphEdge, GraphError, GraphNode, ModuleProfile,
    NodeChange, ResolutionCacheEntry,
};

/// FROZEN CONTRACT — GraphStore trait.
///
/// Agent A owns this interface. Agents B and C consume it.
/// Do NOT modify this trait signature without coordination.
pub trait GraphStore {
    /// Look up a node by its content hash.
    fn get_node(&self, hash: &str) -> Option<GraphNode>;

    /// Look up a node by its internal ID.
    fn get_node_by_id(&self, id: u64) -> Option<GraphNode>;

    /// Get edges connected to a node in the specified direction.
    fn get_edges(&self, node_id: u64, direction: EdgeDirection) -> Vec<GraphEdge>;

    /// Get the module profile for a given module node.
    fn get_module_profile(&self, module_id: u64) -> Option<ModuleProfile>;

    /// Get all nodes in a specific file.
    fn get_nodes_in_file(&self, file_path: &str) -> Vec<GraphNode>;

    /// Get all module-type nodes in the graph.
    fn get_all_modules(&self) -> Vec<GraphNode>;

    /// Apply a batch of node changes (add, update, remove).
    fn update_nodes(&mut self, changes: Vec<NodeChange>) -> Result<(), GraphError>;

    /// Apply a batch of edge changes (add, remove).
    fn update_edges(&mut self, changes: Vec<EdgeChange>) -> Result<(), GraphError>;

    /// Get previous hashes for rename tracking.
    fn get_previous_hashes(&self, node_id: u64) -> Vec<String>;

    /// Find modules whose function_name_prefixes contain the given prefix,
    /// excluding modules in the specified file. Used by W001 placement check.
    fn find_modules_by_prefix(&self, prefix: &str, exclude_file: &str) -> Vec<ModuleProfile>;

    /// Find nodes with the given name and kind, excluding a specific file.
    /// Used by W002 duplicate_name check.
    fn find_nodes_by_name(&self, name: &str, kind: &str, exclude_file: &str) -> Vec<GraphNode>;

    /// Replace the entire body-hash index with `entries`.
    ///
    /// Populated during `keel map`. Implementations should apply this
    /// atomically — the index must never be observed half-rebuilt.
    ///
    /// Additive with a no-op default so backends that do not support
    /// duplicate detection keep compiling; such backends simply report no
    /// duplicates via [`GraphStore::find_body_matches`].
    fn replace_body_index(&mut self, entries: Vec<BodyIndexEntry>) -> Result<(), GraphError> {
        let _ = entries;
        Ok(())
    }

    /// Find every indexed node whose normalized body matches `body_hash`.
    ///
    /// Queried by W006 duplicate-implementation enforcement. Returns an empty
    /// vec when the backend does not maintain a body index (the default).
    fn find_body_matches(&self, body_hash: &str) -> Vec<BodyIndexEntry> {
        let _ = body_hash;
        Vec::new()
    }

    /// Load the persisted Tier 3 resolution cache (rows tagged `tier3`).
    ///
    /// Lets SCIP/LSP resolutions survive across `keel map` runs. Additive with
    /// a no-op default so backends without a resolution cache keep compiling;
    /// such backends simply start every run with a cold Tier 3 cache.
    fn load_resolution_cache(&self) -> Vec<ResolutionCacheEntry> {
        Vec::new()
    }

    /// Replace the persisted Tier 3 resolution cache with `entries`.
    ///
    /// Wholesale rebuild of the `tier3`-tagged rows, applied atomically. Rows
    /// owned by other tiers are left untouched. Additive with a no-op default,
    /// mirroring `load_resolution_cache`.
    fn replace_resolution_cache(
        &mut self,
        entries: Vec<ResolutionCacheEntry>,
    ) -> Result<(), GraphError> {
        let _ = entries;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A backend that implements only the pre-v5 required methods.
    ///
    /// Its existence is the test: if `replace_body_index` or
    /// `find_body_matches` ever stops being a defaulted method, this stops
    /// compiling — which is exactly the additive guarantee we promised
    /// downstream implementors.
    struct MinimalStore;

    impl GraphStore for MinimalStore {
        fn get_node(&self, _hash: &str) -> Option<GraphNode> {
            None
        }
        fn get_node_by_id(&self, _id: u64) -> Option<GraphNode> {
            None
        }
        fn get_edges(&self, _node_id: u64, _direction: EdgeDirection) -> Vec<GraphEdge> {
            Vec::new()
        }
        fn get_module_profile(&self, _module_id: u64) -> Option<ModuleProfile> {
            None
        }
        fn get_nodes_in_file(&self, _file_path: &str) -> Vec<GraphNode> {
            Vec::new()
        }
        fn get_all_modules(&self) -> Vec<GraphNode> {
            Vec::new()
        }
        fn update_nodes(&mut self, _changes: Vec<NodeChange>) -> Result<(), GraphError> {
            Ok(())
        }
        fn update_edges(&mut self, _changes: Vec<EdgeChange>) -> Result<(), GraphError> {
            Ok(())
        }
        fn get_previous_hashes(&self, _node_id: u64) -> Vec<String> {
            Vec::new()
        }
        fn find_modules_by_prefix(&self, _prefix: &str, _exclude_file: &str) -> Vec<ModuleProfile> {
            Vec::new()
        }
        fn find_nodes_by_name(
            &self,
            _name: &str,
            _kind: &str,
            _exclude_file: &str,
        ) -> Vec<GraphNode> {
            Vec::new()
        }
    }

    #[test]
    fn test_body_index_defaults_are_no_ops() {
        let mut store = MinimalStore;
        store
            .replace_body_index(vec![BodyIndexEntry {
                body_hash: "b".into(),
                node_hash: "n".into(),
                name: "f".into(),
                file_path: "src/a.rs".into(),
                line: 1,
            }])
            .expect("default replace_body_index must succeed");

        assert!(
            store.find_body_matches("b").is_empty(),
            "a backend without a body index reports no duplicates"
        );
    }

    #[test]
    fn test_resolution_cache_defaults_are_no_ops() {
        let mut store = MinimalStore;
        store
            .replace_resolution_cache(vec![ResolutionCacheEntry {
                call_site_hash: "h".into(),
                target_file: Some("src/a.rs".into()),
                target_name: Some("f".into()),
                confidence: 0.9,
                provider: Some("scip".into()),
            }])
            .expect("default replace_resolution_cache must succeed");

        assert!(
            store.load_resolution_cache().is_empty(),
            "a backend without a resolution cache starts cold"
        );
    }
}
