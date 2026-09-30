//! Order a deleted file's nodes by their module dependencies.

use std::cmp::Reverse;
use std::collections::HashMap;

use keel_core::types::{GraphNode, NodeKind};

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
