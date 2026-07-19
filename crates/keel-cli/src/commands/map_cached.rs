//! Cached map: read from existing graph.db without re-parsing.
//! Used by `keel map --cached` for fast session-start hooks.

use keel_core::store::GraphStore;
use keel_output::OutputFormatter;

use crate::telemetry_recorder::EventMetrics;

/// Read map from existing graph.db without re-parsing.
///
/// Falls back to a full, non-cached map when the graph is empty (e.g. right
/// after `keel init`, which creates the database but does not populate it —
/// or in a fresh worktree/clone that never ran `keel map`). Without this
/// fallback, the first session-start hook on a fresh repo would inject an
/// error instead of a real structural map. The fast path (cache present)
/// remains unchanged.
pub fn run_cached(
    store: &dyn GraphStore,
    formatter: &dyn OutputFormatter,
    verbose: bool,
    depth: u32,
) -> (i32, EventMetrics) {
    if store.get_all_modules().is_empty() {
        // Always announce the fallback: it swaps a fast cache read for a full
        // repo parse, and a silent slow path is undebuggable from a hook.
        eprintln!("keel map --cached: graph.db is empty, falling back to full map");
        // Delegate to the existing non-cached map path, which opens its own
        // store and performs a full parse. `llm_verbose`/`scope`/`strict` are
        // unused by that path (see its `_`-prefixed params); `tier3_enabled`
        // is left off here — project-level tier3 config (.keel/keel.json)
        // still applies via that path's own config load.
        return super::map::run(formatter, verbose, false, None, false, depth, false, false);
    }

    // Reconstruct the MapResult from the store using the same shared assembly
    // the MCP server uses, so cached output matches a fresh `keel map`.
    let map_result = keel_enforce::map::build_map_from_store(store, depth);

    if verbose {
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
