use keel_enforce::types::StatsResult;
use keel_output::OutputFormatter;

/// Run `keel stats` — display telemetry dashboard.
///
/// Rendering is the formatter's job (`--json`/`--llm` already picked it in
/// `main`); this function only measures the graph.
pub fn run(formatter: &dyn OutputFormatter, verbose: bool) -> i32 {
    let repo = match super::open_repo("stats") {
        Ok(x) => x,
        Err(code) => return code,
    };
    let db_path = repo.db_path();
    let store = repo.store;
    let keel_dir = repo.keel_dir;

    // Gather basic stats from the graph store
    let modules = keel_core::store::GraphStore::get_all_modules(&store);
    let module_count = modules.len();

    let mut function_count = 0u32;
    let mut file_set = std::collections::HashSet::new();
    let mut all_node_ids = Vec::new();
    for module in &modules {
        let nodes = keel_core::store::GraphStore::get_nodes_in_file(&store, &module.file_path);
        function_count += nodes
            .iter()
            .filter(|n| n.kind == keel_core::types::NodeKind::Function)
            .count() as u32;
        file_set.insert(module.file_path.clone());
        for node in &nodes {
            all_node_ids.push(node.id);
        }
        all_node_ids.push(module.id);
    }

    // Count edges by kind
    let mut calls_count = 0u32;
    let mut imports_count = 0u32;
    let mut contains_count = 0u32;
    let mut uses_count = 0u32;
    let mut seen_edges = std::collections::HashSet::new();
    for module in &modules {
        let nodes = keel_core::store::GraphStore::get_nodes_in_file(&store, &module.file_path);
        let all_ids: Vec<u64> = std::iter::once(module.id)
            .chain(nodes.iter().map(|n| n.id))
            .collect();
        for nid in all_ids {
            let edges = keel_core::store::GraphStore::get_edges(
                &store,
                nid,
                keel_core::types::EdgeDirection::Outgoing,
            );
            for edge in &edges {
                if seen_edges.insert(edge.id) {
                    match edge.kind {
                        keel_core::types::EdgeKind::Calls => calls_count += 1,
                        keel_core::types::EdgeKind::Imports => imports_count += 1,
                        keel_core::types::EdgeKind::Contains => contains_count += 1,
                        keel_core::types::EdgeKind::Uses => uses_count += 1,
                        _ => {}
                    }
                }
            }
        }
    }
    let edge_count = calls_count + imports_count + contains_count + uses_count;

    let result = StatsResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "stats".to_string(),
        modules: module_count,
        functions: function_count,
        files: file_set.len(),
        edges: edge_count,
        uses_edges: uses_count,
        calls_edges: calls_count,
        imports_edges: imports_count,
        contains_edges: contains_count,
        telemetry: load_telemetry_aggregate(&keel_dir),
        // The two --verbose extras are carried on the result rather than
        // passed to the formatter: whether they are shown is the command's
        // decision, rendering them is the formatter's.
        db_path: verbose.then(|| db_path.display().to_string()),
        schema_version: verbose.then(|| store.schema_version().ok()).flatten(),
    };

    println!("{}", formatter.format_stats(&result));

    0
}

fn load_telemetry_aggregate(
    keel_dir: &std::path::Path,
) -> Option<keel_core::telemetry::TelemetryAggregate> {
    let telemetry_path = keel_dir.join("telemetry.db");
    if !telemetry_path.exists() {
        return None;
    }
    let store = keel_core::telemetry::TelemetryStore::open(&telemetry_path).ok()?;
    let agg = store.aggregate(30).ok()?;
    if agg.total_invocations == 0 {
        return None;
    }
    Some(agg)
}
