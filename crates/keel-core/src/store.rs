use crate::types::{
    EdgeChange, EdgeDirection, GraphEdge, GraphError, GraphNode, ModuleProfile, NodeChange,
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
}
