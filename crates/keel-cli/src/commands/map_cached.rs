//! Cached map: read from existing graph.db without re-parsing.
//! Used by `keel map --cached` for fast session-start hooks.

use keel_core::config::KeelConfig;
use keel_core::store::GraphStore;
use keel_output::OutputFormatter;
use std::path::Path;

use crate::telemetry_recorder::EventMetrics;

/// Read map from existing graph.db without re-parsing.
///
/// Refuse an unfinished baseline; announce a completed map that is not HEAD's.
/// This path never acquires the graph lock or delegates to a full map.
pub fn run_cached(
    root: &Path,
    cwd: &Path,
    config: &KeelConfig,
    store: &dyn GraphStore,
    formatter: &dyn OutputFormatter,
    verbose: bool,
    depth: u32,
) -> (i32, EventMetrics) {
    if let Some(reason) = super::graph_resync::never_mapped_reason(root, store) {
        eprintln!("keel map --cached: no usable graph — {reason}");
        return (2, EventMetrics::default());
    }

    let stale = match (
        keel_enforce::gitdiff::head_commit(root),
        store.meta_value(keel_core::sqlite_meta::LAST_MAP_COMMIT),
    ) {
        (Some(head), Some(stored)) if stored != head => {
            eprintln!(
                "keel map --cached: the stored graph was built at commit {}, HEAD is {}. Run `keel map` to rebuild it.",
                stored.chars().take(12).collect::<String>(),
                head.chars().take(12).collect::<String>()
            );
            true
        }
        _ => false,
    };

    if !stale {
        super::version_drift::warn(cwd, config);
    }

    // Reconstruct the MapResult from the store using the same shared assembly
    // the MCP server uses, so cached output matches a fresh `keel map`.
    let map_result = keel_enforce::map::build_map_from_store(store, depth);

    if verbose && !stale {
        eprintln!(
            "keel map --cached: read {} nodes, {} edges from graph.db",
            map_result.summary.total_nodes, map_result.summary.total_edges
        );
    }

    let metrics = EventMetrics {
        node_count: map_result.summary.total_nodes,
        edge_count: map_result.summary.total_edges,
        ..Default::default()
    };

    let output = formatter.format_map(&map_result);
    if !output.is_empty() {
        println!("{}", output);
    }
    (0, metrics)
}
