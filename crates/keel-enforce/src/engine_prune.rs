//! Remove a deleted file's nodes, ordered by their module dependencies.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeDirection, GraphError, GraphNode, NodeChange, NodeKind};

/// Remove every node (and its edges) belonging to a deleted file.
///
/// Works entirely through the frozen [`GraphStore`] trait: collect the file's
/// nodes, drop children before their modules, then drop remaining edges.
/// Returns the number of nodes removed.
pub fn prune_file(store: &mut dyn GraphStore, file_path: &str) -> Result<usize, GraphError> {
    let mut nodes = store.get_nodes_in_file(file_path);
    if nodes.is_empty() {
        return Ok(0);
    }

    // Collect before deleting nodes: stores without cascades still need
    // an explicit edge-removal pass after the node transaction succeeds.
    let mut seen_edges = HashSet::new();
    let mut edge_changes = Vec::new();
    for node in &nodes {
        for edge in store.get_edges(node.id, EdgeDirection::Both) {
            if seen_edges.insert(edge.id) {
                edge_changes.push(EdgeChange::Remove(edge.id));
            }
        }
    }
    let count = nodes.len();
    order_file_nodes_for_removal(&mut nodes);
    let node_changes = nodes
        .into_iter()
        .map(|n| NodeChange::Remove(n.id))
        .collect();
    store.update_nodes(node_changes)?;
    // SQLite cascades edges in the node transaction, so a failed node
    // removal leaves nodes and edges intact. This pass is then a no-op.
    if !edge_changes.is_empty() {
        store.update_edges(edge_changes)?;
    }
    Ok(count)
}

/// Put descendants before their parents, with non-modules first at equal depth.
pub(crate) fn order_file_nodes_for_removal(nodes: &mut [GraphNode]) {
    let parents: HashMap<u64, u64> = nodes.iter().map(|n| (n.id, n.module_id)).collect();
    nodes.sort_by_cached_key(|node| {
        let mut parent = node.module_id;
        let mut depth = 0;
        // Only same-file dependencies affect ordering. Bound the walk so a
        // malformed cycle cannot hang pruning; SQLite will reject its removal.
        while let Some(next) = parents.get(&parent) {
            depth += 1;
            if depth == parents.len() {
                break;
            }
            parent = *next;
        }
        (Reverse(depth), node.kind == NodeKind::Module)
    });
}
